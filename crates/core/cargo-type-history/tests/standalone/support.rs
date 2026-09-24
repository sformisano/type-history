use super::resources::{acquire, copy_tree, repository, Family};
use serde_json::Value;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::{Arc, Mutex, MutexGuard};
use tempfile::{Builder, TempDir};

// These external consumers reuse the assigned worktree's normal Cargo target.
static SERIAL: Mutex<()> = Mutex::new(());
pub const STABLE_NAME: &str = "billing.invoice.issued";
pub const LEDGER: &str = "type-history/schemas.json";
pub const V1: &str = include_str!("fixtures/v1.rs");
pub const V2: &str = include_str!("fixtures/v2.rs");
pub const V3: &str = include_str!("fixtures/v3.rs");

pub struct Fixture {
    // Drop private files before the family, and keep SERIAL until both are gone.
    _owner: TempDir,
    root: PathBuf,
    cli: PathBuf,
    family: Option<Arc<Family>>,
    _serial: MutexGuard<'static, ()>,
}

impl Fixture {
    /// Copy the actual checkout and its lock for the documented source installation.
    pub fn source_checkout() -> Self {
        let serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
        let owner = Builder::new()
            .prefix("setup-")
            .tempdir()
            .expect("owned setup below the configured short TMPDIR");
        let root = owner.path().join("my-shop-demo-project");
        let checkout = owner.path().join("type-history");
        let cli = owner.path().join("install/bin/cargo-type-history");
        let repository = repository();
        for directory in ["crates", "book/src"] {
            copy_tree(&repository.join(directory), &checkout.join(directory));
        }
        for file in [
            "Cargo.toml",
            "Cargo.lock",
            "rust-toolchain.toml",
            "README.md",
        ] {
            let source = fs::read(repository.join(file)).expect("checked checkout input");
            fs::write(checkout.join(file), &source).expect("copy exact checkout input");
            assert_eq!(
                fs::read(checkout.join(file)).expect("copied checkout input"),
                source,
                "preserve candidate {file} byte for byte"
            );
        }
        Self {
            _owner: owner,
            root,
            cli,
            family: None,
            _serial: serial,
        }
    }

    pub fn empty() -> Self {
        // Queued fixtures retain the family while waiting for the shared target.
        let (family, serial) = acquire(&SERIAL);
        let owner = Builder::new()
            .prefix("history-")
            .tempdir()
            .expect("owned fixture below the configured short TMPDIR");
        let root = owner.path().join("consumer");
        let cli = owner.path().join("cargo-type-history");
        fs::create_dir_all(&root).expect("consumer root");
        let fixture = Self {
            _owner: owner,
            root,
            cli,
            family: Some(family),
            _serial: serial,
        };
        fixture.family().copy_cli(&fixture.cli, |vendor| {
            success(&fixture.command_at(vendor, "cargo", &["generate-lockfile", "--offline"], &[]));
            success(&fixture.command_at(
                vendor,
                "cargo",
                &[
                    "build",
                    "--package",
                    "cargo-type-history",
                    "--locked",
                    "--offline",
                ],
                &[],
            ));
            repository().join("target/debug/cargo-type-history")
        });
        let vendor = fixture.family_root();
        fixture.write(
            "Cargo.toml",
            &format!(
                r#"[package]
name = "standalone-history-consumer"
version = "0.0.0"
edition = "2024"
publish = false
build = "build.rs"
[workspace]
[dependencies]
history_api = {{ package = "type-history", path = {:?} }}
serde = {{ version = "1.0", features = ["derive"] }}
serde_json = "1.0.149"
[build-dependencies]
history_build = {{ package = "type-history-build", path = {:?} }}
[profile.release_child]
inherits = "release"
debug-assertions = true
opt-level = 0
[profile.dev_child]
inherits = "dev"
debug-assertions = false
opt-level = 2
"#,
                vendor.join("crates/core/type-history"),
                vendor.join("crates/core/type-history-build"),
            ),
        );
        fixture.write("build.rs", "fn main() { history_build::compile(); }\n");
        fixture.write("src/lib.rs", "");
        success(&fixture.cargo(&["generate-lockfile", "--offline"]));
        fixture.assert_independent_graph();
        fixture
    }

    pub fn frozen() -> Self {
        let fixture = Self::empty();
        fixture.family().prepare_frozen(
            &fixture.root,
            || {
                success(&fixture.cli(&["init", "--package", "standalone-history-consumer"]));
                fixture.write("src/lib.rs", V1);
                success(&fixture.cargo(&["build", "--locked", "--offline"]));
                success(&fixture.cli(&["freeze", "--package", "standalone-history-consumer"]));
            },
            || success(&fixture.cargo(&["build", "--locked", "--offline"])),
        );
        fixture
    }

    fn family(&self) -> &Family {
        self.family.as_deref().expect("ordinary consumer family")
    }
    pub fn family_root(&self) -> &Path {
        self.family().root()
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn write(&self, relative: &str, value: &str) {
        let path = self.root.join(relative);
        fs::create_dir_all(path.parent().expect("fixture parent")).expect("fixture parents");
        fs::write(path, value).expect("fixture write");
    }
    pub fn read(&self, relative: &str) -> String {
        fs::read_to_string(self.root.join(relative)).expect("fixture read")
    }
    pub fn remove(&self, relative: &str) {
        fs::remove_file(self.root.join(relative)).expect("fixture removal");
    }
    pub fn ledger(&self) -> Value {
        serde_json::from_str(&self.read(LEDGER)).expect("committed ledger")
    }
    pub fn set_ledger(&self, value: &Value) {
        self.write(
            LEDGER,
            &serde_json::to_string_pretty(value).expect("encode ledger"),
        );
    }
    pub fn cargo(&self, arguments: &[&str]) -> Output {
        self.cargo_env(arguments, &[])
    }
    pub fn cargo_env(&self, arguments: &[&str], environment: &[(&str, Option<&str>)]) -> Output {
        self.command_at(&self.root, "cargo", arguments, environment)
    }
    pub fn cli(&self, arguments: &[&str]) -> Output {
        self.cli_env(arguments, &[])
    }
    pub fn cli_env(&self, arguments: &[&str], environment: &[(&str, Option<&str>)]) -> Output {
        self.command_at(&self.root, &self.cli, arguments, environment)
    }
    pub fn cli_command_env(
        &self,
        arguments: &[&str],
        environment: &[(&str, Option<&str>)],
    ) -> Command {
        self.configured_command(&self.root, &self.cli, arguments, environment)
    }
    pub fn cli_path(&self) -> &Path {
        &self.cli
    }
    pub fn cargo_at(&self, root: &Path, arguments: &[&str]) -> Output {
        self.command_at(root, "cargo", arguments, &[])
    }
    pub fn cli_at(&self, root: &Path, arguments: &[&str]) -> Output {
        self.command_at(root, &self.cli, arguments, &[])
    }
    fn command_at(
        &self,
        root: &Path,
        executable: impl AsRef<Path>,
        arguments: &[&str],
        environment: &[(&str, Option<&str>)],
    ) -> Output {
        self.configured_command(root, executable, arguments, environment)
            .output()
            .expect("consumer command")
    }
    fn configured_command(
        &self,
        root: &Path,
        executable: impl AsRef<Path>,
        arguments: &[&str],
        environment: &[(&str, Option<&str>)],
    ) -> Command {
        let mut command = Command::new(executable.as_ref());
        command
            .current_dir(root)
            .args(arguments)
            // Keep captured diagnostics stable when CI forces coloured output.
            .env("CARGO_TERM_COLOR", "never")
            .env("CARGO_TARGET_DIR", repository().join("target"));
        for key in [
            "TYPE_HISTORY_REQUIRE_FROZEN",
            "TYPE_HISTORY_SCHEMA_EXPORT",
            "TYPE_HISTORY_AUTHORITY_KIND",
            "TYPE_HISTORY_LEDGER_PATH",
            "TYPE_HISTORY_SCHEMA_ID_PREFIX",
            "TYPE_HISTORY_PRIVATE_SNAPSHOT",
        ] {
            command.env_remove(key);
        }
        for (key, value) in environment {
            match value {
                Some(value) => {
                    command.env(key, value);
                }
                None => {
                    command.env_remove(key);
                }
            }
        }
        command
    }

    pub fn assert_independent_graph(&self) {
        let output = self.cargo(&["metadata", "--format-version", "1", "--locked", "--offline"]);
        success(&output);
        let metadata: Value = serde_json::from_slice(&output.stdout).expect("Cargo metadata");
        let packages = metadata["packages"].as_array().expect("packages");
        for package in packages {
            let name = package["name"].as_str().expect("package name");
            assert!(!name.starts_with("sqlx"), "forbidden dependency {name}");
            let manifest = package["manifest_path"].as_str().expect("manifest path");
            assert!(
                !manifest.starts_with(repository().to_str().expect("UTF-8 repository")),
                "hidden workspace source {manifest}"
            );
        }
        let nodes = metadata["resolve"]["nodes"]
            .as_array()
            .expect("resolved graph");
        let facade = packages
            .iter()
            .find(|package| package["name"] == "type-history")
            .expect("facade");
        let mut remaining = vec![facade["id"].as_str().expect("facade id")];
        let mut visited = BTreeSet::new();
        while let Some(id) = remaining.pop() {
            if !visited.insert(id) {
                continue;
            }
            let package = packages
                .iter()
                .find(|package| package["id"] == id)
                .expect("runtime package");
            let name = package["name"].as_str().expect("runtime name");
            assert!(
                ![
                    "syn",
                    "quote",
                    "proc-macro2",
                    "toml",
                    "tempfile",
                    "cargo_metadata",
                    "type-history-build",
                    "type-history-codegen"
                ]
                .contains(&name),
                "host-only package on runtime library edges: {name}"
            );
            let node = nodes
                .iter()
                .find(|node| node["id"] == id)
                .expect("runtime node");
            for dependency in node["deps"].as_array().expect("dependencies") {
                if !dependency["dep_kinds"]
                    .as_array()
                    .expect("edge kinds")
                    .iter()
                    .any(|kind| kind["kind"].is_null())
                {
                    continue;
                }
                let target = packages
                    .iter()
                    .find(|package| package["id"] == dependency["pkg"])
                    .expect("dependency package");
                if target["targets"]
                    .as_array()
                    .expect("targets")
                    .iter()
                    .any(|target| {
                        target["kind"]
                            .as_array()
                            .expect("target kind")
                            .iter()
                            .any(|kind| kind == "proc-macro")
                    })
                {
                    continue;
                }
                remaining.push(dependency["pkg"].as_str().expect("dependency id"));
            }
        }
        assert!(self
            .family_root()
            .join("crates/core/type-history-build/src/lib.rs")
            .is_file());
    }
}

pub fn text(output: &Output) -> String {
    format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}
pub fn success(output: &Output) {
    assert!(output.status.success(), "{}", text(output));
}
pub fn failure(output: &Output, expected: &str) {
    assert!(
        !output.status.success(),
        "unexpected success: {}",
        text(output)
    );
    assert!(
        text(output).contains(expected),
        "missing {expected:?}: {}",
        text(output)
    );
}
