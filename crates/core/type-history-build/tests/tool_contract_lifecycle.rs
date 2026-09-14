//! Public lifecycle calls preserve a custom build integration's signal selection.

use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tempfile::{Builder as TempBuilder, TempDir};
use type_history_build::contract::{ToolContract, STANDALONE};
use type_history_build::lifecycle::{run, LifecycleOps};

const ACTION: &str = "TYPE_HISTORY_TEST_CONTRACT_ACTION";
const PACKAGE: &str = "custom-history-lifecycle";
const CUSTOM: ToolContract = ToolContract {
    strict_env: "CUSTOM_HISTORY_REQUIRE_FROZEN",
    export_env: "CUSTOM_HISTORY_SCHEMA_EXPORT",
    ..STANDALONE
};

const BUILD: &str = r#"
use std::env;
use std::path::PathBuf;
use history_build::compile::compile_package;
use history_build::contract::{ToolContract, STANDALONE};
use history_build::discover;
use history_build::inventory::Admission;

fn main() {
    for name in [STANDALONE.strict_env, STANDALONE.export_env] {
        assert_eq!(env::var(name).as_deref(), Ok("unrelated"));
    }
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let inventory = discover::read(&root, Admission::Ordinary).unwrap();
    let contract = ToolContract {
        strict_env: "CUSTOM_HISTORY_REQUIRE_FROZEN",
        export_env: "CUSTOM_HISTORY_SCHEMA_EXPORT",
        ..STANDALONE
    };
    compile_package(&root, &inventory, &contract).unwrap();
}
"#;

const ORDINARY_GUARD: &str = r#"
#[cfg(not(test))]
const _: () = assert!(option_env!("CUSTOM_HISTORY_SCHEMA_EXPORT").is_none());
"#;

const V1: &str = r#"
#[type_history::versioned(stable_name = "shop.receipt.created")]
pub struct ReceiptCreated {
    pub amount_cents: u64,
}
"#;

const V2: &str = r#"
#[type_history::versioned(stable_name = "shop.receipt.created")]
pub struct ReceiptCreated {
    pub amount_cents: u64,
    #[history(added_in = v2, backfill_value = String::from("USD"))]
    pub currency: String,
}
"#;

#[test]
fn lifecycle_child() {
    let Ok(action) = env::var(ACTION) else {
        return;
    };
    if let Err(error) = run(
        [action.as_str(), "--package", PACKAGE]
            .into_iter()
            .map(OsString::from),
        &CUSTOM,
        &LifecycleOps {
            discover: type_history_build::discover::read,
            decode_export: type_history_build::standalone_export::decode,
        },
    ) {
        eprintln!("custom integration lifecycle failed: {error}");
        std::process::exit(2);
    }
}

struct Fixture {
    _owner: TempDir,
    root: PathBuf,
    scratch: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let owner = TempBuilder::new().prefix("contract-").tempdir().unwrap();
        let root = owner.path().join("package");
        let scratch = owner.path().join("scratch");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir(&scratch).unwrap();
        let build = Path::new(env!("CARGO_MANIFEST_DIR"));
        let runtime = build.parent().unwrap().join("type-history");
        fs::write(
            root.join("Cargo.toml"),
            format!(
                "[package]\nname = {PACKAGE:?}\nversion = '0.0.0'\nedition = '2024'\n[workspace]\n[dependencies]\ntype-history = {{ path = {runtime:?} }}\n[build-dependencies]\nhistory_build = {{ package = 'type-history-build', path = {build:?} }}\n"
            ),
        )
        .unwrap();
        fs::write(root.join("build.rs"), BUILD).unwrap();
        fs::write(root.join("src/lib.rs"), ORDINARY_GUARD).unwrap();
        let fixture = Self {
            _owner: owner,
            root,
            scratch,
        };
        succeeded(&fixture.cargo(&["generate-lockfile", "--offline"]));
        fixture
    }

    fn command(&self, executable: impl AsRef<Path>) -> Command {
        let mut command = Command::new(executable.as_ref());
        command
            .current_dir(&self.root)
            .env("TMPDIR", &self.scratch)
            .env(
                "CARGO_TARGET_DIR",
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .ancestors()
                    .nth(3)
                    .unwrap()
                    .join("target"),
            )
            .env(STANDALONE.strict_env, "unrelated")
            .env(STANDALONE.export_env, "unrelated")
            .env(CUSTOM.strict_env, "1")
            .env_remove(CUSTOM.export_env);
        command
    }

    fn cargo(&self, arguments: &[&str]) -> Output {
        self.command(env!("CARGO"))
            .args(arguments)
            .output()
            .unwrap()
    }

    fn lifecycle(&self, action: &str) -> Output {
        let output = self
            .command(env::current_exe().unwrap())
            .args(["--exact", "lifecycle_child", "--nocapture"])
            .env(ACTION, action)
            .env(CUSTOM.export_env, "1")
            .output()
            .unwrap();
        assert!(
            text(&output).contains("running 1 test"),
            "{}",
            text(&output)
        );
        assert_eq!(fs::read_dir(&self.scratch).unwrap().count(), 0);
        output
    }

    fn source(&self, source: &str) {
        fs::write(
            self.root.join("src/lib.rs"),
            format!("{ORDINARY_GUARD}\n{source}"),
        )
        .unwrap();
    }

    fn ledger(&self) -> Vec<u8> {
        fs::read(self.root.join(CUSTOM.ledger_path)).unwrap()
    }
}

fn text(output: &Output) -> String {
    format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn succeeded(output: &Output) {
    assert!(output.status.success(), "{}", text(output));
}

#[test]
fn lifecycle_clears_selected_signals_and_keeps_other_frontend_settings() {
    let fixture = Fixture::new();
    succeeded(&fixture.lifecycle("init"));
    assert_eq!(fixture.ledger(), b"{}\n");

    fixture.source(V1);
    let rejected = fixture.cargo(&["check", "--lib", "--locked", "--offline"]);
    assert!(!rejected.status.success(), "{}", text(&rejected));
    assert!(text(&rejected).contains("is a draft"));
    assert_eq!(fixture.ledger(), b"{}\n");

    succeeded(&fixture.lifecycle("freeze"));
    let frozen_v1 = fixture.ledger();
    assert_ne!(frozen_v1, b"{}\n");
    succeeded(&fixture.cargo(&["check", "--lib", "--locked", "--offline"]));

    fixture.source(V2);
    let rejected = fixture.cargo(&["check", "--lib", "--locked", "--offline"]);
    assert!(!rejected.status.success(), "{}", text(&rejected));
    assert!(text(&rejected).contains("V2 is a draft"));
    assert_eq!(fixture.ledger(), frozen_v1);

    let config = fixture.root.join(".cargo/config.toml");
    fs::create_dir_all(config.parent().unwrap()).unwrap();
    fs::write(
        &config,
        format!(
            "[env]\n{} = {{ value = '1', force = true }}\n",
            CUSTOM.strict_env
        ),
    )
    .unwrap();
    let rejected = fixture.lifecycle("freeze");
    assert_eq!(rejected.status.code(), Some(2), "{}", text(&rejected));
    assert!(text(&rejected).contains(&format!("restores {}=1", CUSTOM.strict_env)));
    assert!(text(&rejected).contains("V2 is a draft"));
    assert_eq!(fixture.ledger(), frozen_v1);
    fs::remove_file(config).unwrap();

    succeeded(&fixture.lifecycle("freeze"));
    let frozen_v2 = fixture.ledger();
    assert_ne!(frozen_v2, frozen_v1);
    succeeded(&fixture.cargo(&["check", "--lib", "--locked", "--offline"]));
    succeeded(&fixture.lifecycle("check"));
    assert_eq!(fixture.ledger(), frozen_v2);
}
