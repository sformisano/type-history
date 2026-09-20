// Stays `mod.rs`: as `tests/support.rs` Cargo would compile it as its own test binary.
use rustix::process::{kill_process_group, Pid, Signal};
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::{ErrorKind, Read, Write};
use std::os::unix::fs::symlink;
use std::os::unix::net::UnixListener;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use tempfile::{Builder, TempDir};

const LEDGER: &str = "type-history/schemas.json";

pub fn run() {
    let fixture = Fixture::new();
    fixture.package("a");
    fixture.package("b");
    fixture.workspace(false);
    fixture.lock();
    fixture.cli(&["freeze", "-p", "a"], true);

    let original = fixture.read("a/type-history/schemas.json");
    let stale = fixture.interleave(&["freeze", "-p", "a"], "locate", || {
        fixture.write("a/type-history/schemas.json", "MALFORMED\n");
    });
    assert!(!stale.status.success(), "stale no-op: {}", text(&stale));
    assert_eq!(fixture.read("a/type-history/schemas.json"), "MALFORMED\n");
    fixture.write("a/type-history/schemas.json", &original);

    fixture.write(
        "b/src/lib.rs",
        "compile_error!(\"new member must be checked\");\n",
    );
    fixture.cli(&["check"], true);
    let membership = fixture.interleave(&["check", "--format", "json"], "metadata", || {
        fixture.workspace(true);
        fixture.lock();
    });
    assert!(
        !membership.status.success(),
        "stale selection: {}",
        text(&membership)
    );
    fixture.cli(&["check"], false);
    fixture.write("b/src/lib.rs", "");
    fixture.cli(&["check"], true);

    let aggregate = fixture.interleave(&["check", "--format", "json"], "second-package", || {
        fixture.write(
            "a/src/lib.rs",
            "compile_error!(\"earlier package changed\");\n",
        );
    });
    assert!(
        !aggregate.status.success(),
        "stale aggregate: {}",
        text(&aggregate)
    );
    fixture.cli(&["check"], false);
    fixture.write("a/src/lib.rs", "");

    fixture.write(
        "a/build.rs",
        "use std::path::Path; fn main() { println!(\"cargo::rustc-check-cfg=cfg(input_present)\"); if Path::new(\"inputs/node_modules/flag\").exists() { println!(\"cargo::rustc-cfg=input_present\"); } }\n",
    );
    fixture.write(
        "a/src/lib.rs",
        "#[cfg(input_present)] compile_error!(\"required build input must be captured\");\n",
    );
    fixture.cli(&["check", "-p", "a"], true);
    fixture.write("a/inputs/node_modules/flag", "present");
    fixture.cargo(
        &["check", "--lib", "-p", "a", "--locked", "--offline"],
        false,
    );
    fixture.cli(&["check", "-p", "a"], false);
    fixture.write("a/build.rs", "fn main() {}\n");
    fixture.write("a/src/lib.rs", "");

    // Initialization after the initial metadata response must enter selection.
    fs::remove_file(fixture.root.join("b").join(LEDGER)).unwrap();
    fixture.cli(&["check"], true);
    let initialized = fixture.interleave(&["check"], "metadata", || {
        fixture.write("b/type-history/schemas.json", "MALFORMED\n");
    });
    assert!(
        !initialized.status.success(),
        "omitted ledger: {}",
        text(&initialized)
    );
    fixture.write("b/type-history/schemas.json", "{}\n");

    for directory in ["ordinary", "node_modules", ".next", ".git", ".schemas.lock"] {
        fixture.write(
            "a/src/lib.rs",
            &format!("#[path = \"{directory}/mod.rs\"] pub mod input;\n"),
        );
        fixture.write(
            &format!("a/src/{directory}/mod.rs"),
            "pub const INPUT: u32 = 1;\n",
        );
        fixture.cargo(
            &["check", "--lib", "-p", "a", "--locked", "--offline"],
            true,
        );
        fixture.cli(&["check", "-p", "a"], true);
    }
    fixture.write("a/src/lib.rs", "");

    let lock = fixture.root.join("a/type-history/.schemas.lock");
    fs::remove_file(&lock).unwrap();
    symlink("../unowned-file", &lock).unwrap();
    fixture.cli(&["freeze", "-p", "a"], false);
    assert!(!fixture.root.join("a/unowned-file").exists());
    fixture.write("a/type-history/schemas.json", "MALFORMED\n");
    fixture.cli(&["freeze", "-p", "a"], false);
    assert!(!fixture.root.join("a/unowned-file").exists());
    fs::remove_file(lock).unwrap();
    fixture.write("a/type-history/schemas.json", &original);
    fixture.cli(&["freeze", "-p", "a"], true);
    assert_eq!(fixture.read("a/type-history/schemas.json"), original);
    assert_eq!(fs::read_dir(&fixture.scratch).unwrap().count(), 0);
}

struct Fixture {
    owner: TempDir,
    root: PathBuf,
    scratch: PathBuf,
    proxy: PathBuf,
    cargo: PathBuf,
    target: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let owner = Builder::new().prefix("ledger-").tempdir().unwrap();
        let root = owner.path().join("source");
        let scratch = owner.path().join("scratch");
        let proxy = owner.path().join("bin");
        for path in [&root, &scratch, &proxy] {
            fs::create_dir_all(path).unwrap();
        }
        let source = owner.path().join("proxy.rs");
        fs::write(&source, include_str!("proxy.rs")).unwrap();
        let output = Command::new("rustc")
            .args(["--edition=2021", "-C", "debuginfo=0"])
            .arg(&source)
            .arg("-o")
            .arg(proxy.join("cargo"))
            .output()
            .unwrap();
        assert!(output.status.success(), "{}", text(&output));
        Self {
            owner,
            root,
            scratch,
            proxy,
            cargo: PathBuf::from(env!("CARGO")),
            target: Path::new(env!("CARGO_MANIFEST_DIR"))
                .ancestors()
                .nth(3)
                .unwrap()
                .join("target"),
        }
    }
    fn write(&self, path: &str, text: &str) {
        let path = self.root.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    fn read(&self, path: &str) -> String {
        fs::read_to_string(self.root.join(path)).unwrap()
    }
    fn package(&self, name: &str) {
        self.write(
            &format!("{name}/Cargo.toml"),
            &format!(
                "[package]\nname={name:?}\nversion='0.0.0'\nedition='2024'\nbuild='build.rs'\n"
            ),
        );
        self.write(&format!("{name}/build.rs"), "fn main() {}\n");
        self.write(&format!("{name}/src/lib.rs"), "");
        self.write(&format!("{name}/{LEDGER}"), "{}\n");
    }
    fn workspace(&self, both: bool) {
        self.write(
            "Cargo.toml",
            if both {
                "[workspace]\nmembers=['a','b']\nresolver='2'\n"
            } else {
                "[workspace]\nmembers=['a']\nexclude=['b']\nresolver='2'\n"
            },
        );
    }
    fn command(&self, executable: &Path) -> Command {
        let mut command = Command::new(executable);
        command
            .current_dir(&self.root)
            .env("TMPDIR", &self.scratch)
            .env("CARGO_TARGET_DIR", &self.target)
            .env_remove("CARGO")
            .env_remove("TYPE_HISTORY_REQUIRE_FROZEN")
            .env_remove("TYPE_HISTORY_SCHEMA_EXPORT");
        command
    }
    fn cargo(&self, args: &[&str], success: bool) {
        let output = self.command(&self.cargo).args(args).output().unwrap();
        assert_eq!(output.status.success(), success, "{}", text(&output));
    }
    fn lock(&self) {
        self.cargo(&["generate-lockfile", "--offline"], true);
    }
    fn cli(&self, args: &[&str], success: bool) {
        let before = inputs(&self.root);
        let output = self
            .command(Path::new(env!("CARGO_BIN_EXE_cargo-type-history")))
            .args(args)
            .output()
            .unwrap();
        assert_eq!(output.status.success(), success, "{}", text(&output));
        assert_eq!(before, inputs(&self.root), "CLI changed source inputs");
    }
    fn interleave(&self, args: &[&str], boundary: &str, edit: impl FnOnce()) -> Output {
        let socket = self.owner.path().join("gate.sock");
        if socket.exists() {
            fs::remove_file(&socket).unwrap();
        }
        let listener = UnixListener::bind(&socket).unwrap();
        listener.set_nonblocking(true).unwrap();
        let mut paths = vec![self.proxy.clone()];
        paths.extend(env::split_paths(&env::var_os("PATH").unwrap()));
        let child = self
            .command(Path::new(env!("CARGO_BIN_EXE_cargo-type-history")))
            .args(args)
            .env("PATH", env::join_paths(paths).unwrap())
            .env("LEDGER_TEST_CARGO", &self.cargo)
            .env("LEDGER_TEST_ROOT", &self.root)
            .env("LEDGER_TEST_SOCKET", &socket)
            .env("LEDGER_TEST_BOUNDARY", boundary)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0)
            .spawn()
            .unwrap();
        let mut child = OwnedChild(Some(child));
        let deadline = Instant::now() + Duration::from_secs(60);
        let mut stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(error) if error.kind() == ErrorKind::WouldBlock => {}
                Err(error) => panic!("barrier accept: {error}"),
            }
            if child.0.as_mut().unwrap().try_wait().unwrap().is_some() {
                panic!(
                    "CLI exited before {boundary}: {}",
                    text(&child.0.take().unwrap().wait_with_output().unwrap())
                );
            }
            assert!(Instant::now() < deadline, "CLI did not reach {boundary}");
            thread::sleep(Duration::from_millis(10));
        };
        stream
            .set_read_timeout(Some(Duration::from_secs(60)))
            .unwrap();
        let mut ready = [0; 1];
        stream.read_exact(&mut ready).unwrap();
        assert_eq!(ready, [1]);
        edit();
        let after_edit = inputs(&self.root);
        stream.write_all(&[1]).unwrap();
        let output = child.0.take().unwrap().wait_with_output().unwrap();
        assert_eq!(after_edit, inputs(&self.root), "CLI changed edited inputs");
        output
    }
}

struct OwnedChild(Option<Child>);
impl Drop for OwnedChild {
    fn drop(&mut self) {
        if let Some(child) = &mut self.0 {
            if let Some(group) = Pid::from_raw(child.id() as i32) {
                let _ = kill_process_group(group, Signal::KILL);
            }
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Input {
    Directory,
    File(Vec<u8>),
    Symlink(PathBuf),
}

fn inputs(root: &Path) -> BTreeMap<PathBuf, Input> {
    fn walk(root: &Path, directory: &Path, result: &mut BTreeMap<PathBuf, Input>) {
        for entry in fs::read_dir(directory).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            let relative = path.strip_prefix(root).unwrap().to_owned();
            // A command may create its persistent operational lock.
            if [
                "a/type-history/.schemas.lock",
                "b/type-history/.schemas.lock",
            ]
            .iter()
            .any(|lock| relative == Path::new(lock))
            {
                continue;
            }
            let kind = entry.file_type().unwrap();
            let input = if kind.is_symlink() {
                Input::Symlink(fs::read_link(&path).unwrap())
            } else if kind.is_dir() {
                walk(root, &path, result);
                Input::Directory
            } else {
                assert!(kind.is_file(), "unexpected special fixture input");
                Input::File(fs::read(&path).unwrap())
            };
            result.insert(relative, input);
        }
    }
    let mut result = BTreeMap::new();
    walk(root, root, &mut result);
    result
}
fn text(output: &Output) -> String {
    format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}
