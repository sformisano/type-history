use serde_json::Value;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::{Mutex, MutexGuard};
use tempfile::{Builder, TempDir};
use toml::{Table, Value as TomlValue};

// These external consumers reuse the assigned worktree's normal Cargo target.
static SERIAL: Mutex<()> = Mutex::new(());
pub const STABLE_NAME: &str = "billing.invoice.issued";
pub const LEDGER: &str = "type-history/schemas.json";
pub const V1: &str = include_str!("fixtures/v1.rs");
pub const V2: &str = include_str!("fixtures/v2.rs");
pub const V3: &str = include_str!("fixtures/v3.rs");

pub struct Fixture {
    owner: TempDir,
    root: PathBuf,
    cli: PathBuf,
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
            owner,
            root,
            cli,
            _serial: serial,
        }
    }

    pub fn empty() -> Self {
        let serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
        let owner = Builder::new()
            .prefix("history-")
            .tempdir()
            .expect("owned fixture below the configured short TMPDIR");
        let root = owner.path().join("consumer");
        let vendor = owner.path().join("vendor");
        let cli = owner.path().join("cargo-type-history");
        fs::create_dir_all(&root).expect("consumer root");
        let fixture = Self {
            owner,
            root,
            cli,
            _serial: serial,
        };
        copy_family(&vendor);
        let output = fixture.command_at(&vendor, "cargo", &["generate-lockfile", "--offline"], &[]);
        success(&output);
        success(&fixture.command_at(
            &vendor,
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
        fs::copy(
            repository().join("target/debug/cargo-type-history"),
            &fixture.cli,
        )
        .expect("retain the CLI built from Type History sources");
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
        success(&fixture.cli(&["init", "--package", "standalone-history-consumer"]));
        fixture.write("src/lib.rs", V1);
        success(&fixture.cargo(&["build", "--locked", "--offline"]));
        success(&fixture.cli(&["freeze", "--package", "standalone-history-consumer"]));
        fixture
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
        let mut command = Command::new(executable.as_ref());
        command
            .current_dir(root)
            .args(arguments)
            .env("CARGO_TARGET_DIR", repository().join("target"));
        for key in [
            "TYPE_HISTORY_REQUIRE_FROZEN",
            "TYPE_HISTORY_SCHEMA_EXPORT",
            "TYPE_HISTORY_AUTHORITY_KIND",
            "TYPE_HISTORY_LEDGER_PATH",
            "TYPE_HISTORY_SCHEMA_ID_PREFIX",
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
        command.output().expect("consumer command")
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
            .owner
            .path()
            .join("vendor/crates/core/type-history-build/src/lib.rs")
            .is_file());
    }
}

fn copy_family(vendor: &Path) {
    let repository = repository();
    copy_tree(&repository.join("crates/core"), &vendor.join("crates/core"));
    copy_tree(&repository.join("book/src"), &vendor.join("book/src"));
    fs::copy(repository.join("README.md"), vendor.join("README.md"))
        .expect("preserve compiled tutorial source");
    let manifest: TomlValue = toml::from_str(
        &fs::read_to_string(repository.join("Cargo.toml")).expect("checked root manifest"),
    )
    .expect("root TOML");
    let mut dependencies = Table::new();
    for name in ["serde", "serde_json", "schemars", "thiserror", "sha2"] {
        dependencies.insert(
            name.into(),
            manifest["workspace"]["dependencies"][name].clone(),
        );
    }
    let mut workspace = Table::new();
    workspace.insert(
        "members".into(),
        TomlValue::Array(
            [
                "type-history-core",
                "type-history-codegen",
                "type-history-macros",
                "type-history",
                "type-history-build",
                "cargo-type-history",
            ]
            .into_iter()
            .map(|name| TomlValue::String(format!("crates/core/{name}")))
            .collect(),
        ),
    );
    workspace.insert("resolver".into(), "2".into());
    workspace.insert("package".into(), manifest["workspace"]["package"].clone());
    workspace.insert("dependencies".into(), dependencies.into());
    fs::write(
        vendor.join("Cargo.toml"),
        toml::to_string(&Table::from_iter([("workspace".into(), workspace.into())]))
            .expect("minimal workspace"),
    )
    .expect("write minimal workspace");
    fs::copy(
        repository.join("rust-toolchain.toml"),
        vendor.join("rust-toolchain.toml"),
    )
    .expect("preserve exact compiler");
}

fn copy_tree(source: &Path, destination: &Path) {
    fs::create_dir_all(destination).expect("vendor directory");
    for entry in fs::read_dir(source).expect("checked family source") {
        let entry = entry.expect("source entry");
        if ["target", ".git", ".agents"]
            .iter()
            .any(|name| entry.file_name() == *name)
        {
            continue;
        }
        let kind = entry.file_type().expect("source kind");
        assert!(
            !kind.is_symlink(),
            "family sources cannot silently escape the copied tree"
        );
        let to = destination.join(entry.file_name());
        if kind.is_dir() {
            copy_tree(&entry.path(), &to);
        } else {
            fs::copy(entry.path(), to).expect("checked source copy");
        }
    }
}

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("repository root")
        .to_owned()
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
