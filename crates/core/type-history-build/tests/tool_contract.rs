//! Public custom frontends select their own build signals.

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use tempfile::Builder as TempBuilder;
use type_history_build::compile::compile_package;
use type_history_build::contract::{ToolContract, STANDALONE};
use type_history_build::inventory::{Declaration, PackageInventory};
use type_history_codegen::ledger::{HistoryReadiness, RecordMetadata};

const PROBE_ROOT: &str = "TYPE_HISTORY_TEST_CONTRACT_ROOT";
const CUSTOM: ToolContract = ToolContract {
    strict_env: "CUSTOM_HISTORY_REQUIRE_FROZEN",
    export_env: "CUSTOM_HISTORY_SCHEMA_EXPORT",
    export_cfg: "custom_history_schema_export",
    ..STANDALONE
};

#[test]
fn custom_contract_probe() {
    let Some(root) = env::var_os(PROBE_ROOT) else {
        return;
    };
    let root = PathBuf::from(root).canonicalize().unwrap();
    if let Ok(source) = env::var("TYPE_HISTORY_TEST_SOURCE") {
        fs::write(root.join("src/lib.rs"), source).unwrap();
    }
    let inventory = PackageInventory {
        package: "custom-history-fixture".to_owned(),
        root: root.clone(),
        declarations: vec![Declaration {
            name: "ReceiptCreated".to_owned(),
            stable_name: "shop.receipt.created".to_owned(),
            metadata: RecordMetadata {},
            version: 1,
            retained_versions: vec![1],
            readiness: HistoryReadiness::Draft,
        }],
        tracked_paths: vec![root.join(CUSTOM.ledger_path)],
        declaration_count: 1,
    };
    if let Err(error) = compile_package(&root, &inventory, &CUSTOM) {
        eprintln!("custom history build failed: {error}");
        std::process::exit(2);
    }
}

fn probe(profile: &str, settings: &[(&str, &str)]) -> Output {
    let owner = TempBuilder::new()
        .prefix("history-contract-")
        .tempdir()
        .unwrap();
    let ledger = owner.path().join(CUSTOM.ledger_path);
    fs::create_dir_all(ledger.parent().unwrap()).unwrap();
    fs::write(&ledger, b"{}\n").unwrap();
    fs::create_dir(owner.path().join("src")).unwrap();
    fs::create_dir(owner.path().join("out")).unwrap();
    fs::write(owner.path().join("Cargo.toml"), "[package]\nname='custom-history-fixture'\nversion='0.1.0'\n[workspace]\n[dependencies]\nhistory_api={package='type-history',version='0.1.0'}\n").unwrap();
    fs::write(owner.path().join("src/lib.rs"), "#[history_api::versioned(stable_name=\"shop.receipt.created\")] pub struct ReceiptCreated {}\n").unwrap();
    let mut command = Command::new(env::current_exe().unwrap());
    command
        .args(["--exact", "custom_contract_probe", "--nocapture"])
        .env(PROBE_ROOT, owner.path())
        .env("OUT_DIR", owner.path().join("out"))
        .env("PROFILE", profile);
    for name in [
        STANDALONE.strict_env,
        STANDALONE.export_env,
        CUSTOM.strict_env,
        CUSTOM.export_env,
    ] {
        command.env_remove(name);
    }
    command.envs(settings.iter().copied());
    let output = command.output().unwrap();
    assert!(
        text(&output).contains("running 1 test"),
        "{}",
        text(&output)
    );
    assert_eq!(fs::read(&ledger).unwrap(), b"{}\n");
    output
}

fn text(output: &Output) -> String {
    format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

#[test]
fn standalone_contract_requires_matching_real_source_inventory() {
    for source in ["", "#[history_api::versioned(stable_name=\"different.record\")] pub struct ReceiptCreated {}", "#[history_api::versioned(stable_name=\"shop.receipt.created\")] pub struct ReceiptCreated { #[history(added_in=v2, backfill_value=0)] pub added: u32 }"] {
        let output = probe("debug", &[("TYPE_HISTORY_TEST_SOURCE", source)]);
        assert_eq!(output.status.code(), Some(2), "{}", text(&output));
        assert!(text(&output).contains("source differs from supplied inventory"));
        assert!(!text(&output).contains("cargo::rustc-env=TYPE_HISTORY_ADMISSION="));
    }
}

#[test]
fn custom_contract_enforces_its_strict_switch() {
    let development = probe("debug", &[]);
    assert!(development.status.success(), "{}", text(&development));
    assert!(text(&development).contains("is a draft"));

    for profile in ["debug", "release"] {
        let strict = probe(profile, &[(CUSTOM.strict_env, "1")]);
        assert_eq!(strict.status.code(), Some(2), "{}", text(&strict));
        assert!(text(&strict).contains("is a draft"));
        for invalid in ["", "0", "true", " 1"] {
            let rejected = probe(profile, &[(CUSTOM.strict_env, invalid)]);
            assert_eq!(rejected.status.code(), Some(2), "{}", text(&rejected));
            assert!(text(&rejected).contains(CUSTOM.strict_env));
            assert!(text(&rejected).contains("must be unset or exactly 1"));
        }
    }
    let release = probe("release", &[]);
    assert_eq!(release.status.code(), Some(2), "{}", text(&release));
    assert!(text(&release).contains("is a draft"));
}

#[test]
fn custom_contract_ignores_other_frontend_signals() {
    for value in ["1", "invalid"] {
        let output = probe(
            "debug",
            &[
                (STANDALONE.strict_env, value),
                (STANDALONE.export_env, value),
            ],
        );
        assert!(output.status.success(), "{}", text(&output));
        assert!(!text(&output).contains(&format!("cargo::rustc-cfg={}", CUSTOM.export_cfg)));
    }
}

#[test]
fn custom_contract_tracks_and_validates_its_export_switch() {
    for settings in [vec![], vec![(CUSTOM.export_env, "1")]] {
        let output = probe("debug", &settings);
        let output_text = text(&output);
        assert!(output.status.success(), "{output_text}");
        for name in [CUSTOM.strict_env, CUSTOM.export_env] {
            assert!(
                output_text.contains(&format!("cargo::rerun-if-env-changed={name}")),
                "{output_text}"
            );
        }
        for name in [STANDALONE.strict_env, STANDALONE.export_env] {
            assert!(
                !output_text.contains(&format!("cargo::rerun-if-env-changed={name}")),
                "{output_text}"
            );
        }
        assert_eq!(
            output_text.contains(&format!("cargo::rustc-cfg={}", CUSTOM.export_cfg)),
            !settings.is_empty(),
            "{output_text}"
        );
    }
    let invalid = probe("debug", &[(CUSTOM.export_env, "0")]);
    assert_eq!(invalid.status.code(), Some(2), "{}", text(&invalid));
    assert!(text(&invalid).contains(CUSTOM.export_env));
}
