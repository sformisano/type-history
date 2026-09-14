//! Independently compiled services exchange bytes from two shared-crate versions.

use super::support::{failure, success, Fixture, LEDGER};
use serde_json::{json, Value as JsonValue};
use std::{
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};
use toml::Value;

const TYPES_V1: &str = include_str!("fixtures/distributed/types-v1.rs");
const TYPES_V2: &str = include_str!("fixtures/distributed/types-v2.rs");
const CHECKOUT_V1: &str = include_str!("fixtures/distributed/checkout-v1.rs");
const CHECKOUT_V2: &str = include_str!("fixtures/distributed/checkout-v2.rs");
const REPORTING_V1: &str = include_str!("fixtures/distributed/reporting-v1.rs");
const REPORTING_V2: &str = include_str!("fixtures/distributed/reporting-v2.rs");
const TYPES_FILES: &[&str] = &["Cargo.toml", "Cargo.lock", "build.rs", "src/lib.rs", LEDGER];

#[test]
fn consumer_first_rollout_preserves_old_binaries_and_rejects_future_events() {
    let fixture = Fixture::empty();
    let owner = fixture.root().parent().expect("owned fixture directory");
    let types_v1 = fixture.root();
    let mut manifest: Value = toml::from_str(&fixture.read("Cargo.toml")).unwrap();
    manifest["package"]["name"] = "shop-types".into();
    manifest["package"]["version"] = "0.1.0".into();
    fixture.write("Cargo.toml", &toml::to_string(&manifest).unwrap());
    success(&fixture.cargo(&["generate-lockfile", "--offline"]));
    success(&fixture.cli(&["init", "--package", "shop-types"]));
    fixture.write("src/lib.rs", TYPES_V1);
    success(&fixture.cli(&["freeze", "--package", "shop-types"]));
    let frozen_v1 = fixture.ledger();

    // These deployed executables are never rebuilt during the rollout.
    let checkout_v1 = build_service(&fixture, "checkout-v1", types_v1, "0.1.0", CHECKOUT_V1);
    let reporting_v1 = build_service(&fixture, "reporting-v1", types_v1, "0.1.0", REPORTING_V1);
    let original_checkout = fs::read(&checkout_v1).unwrap();
    let original_reporting = fs::read(&reporting_v1).unwrap();
    let original_types = TYPES_FILES
        .iter()
        .map(|name| (*name, fs::read(types_v1.join(name)).unwrap()))
        .collect::<Vec<_>>();

    let old_event = json!({
        "stable_name": "shop.receipt.created", "version": 1,
        "payload": {"amount_cents": 1200}
    });
    let queued_v1 = enqueue(owner, "receipt-v1.json", &checkout_v1, None);
    assert_eq!(queued_value(&queued_v1), old_event);
    assert_report(&reporting_v1, &queued_v1, 1200, "USD");

    // The next shared-crate version has its own source, lockfile, and ledger.
    let types_v2 = owner.join("types-v2");
    for (name, bytes) in &original_types {
        write(&types_v2.join(name), bytes);
    }
    manifest["package"]["version"] = "0.2.0".into();
    write(
        &types_v2.join("Cargo.toml"),
        toml::to_string(&manifest).unwrap(),
    );
    write(&types_v2.join("src/lib.rs"), TYPES_V2);
    success(&fixture.cargo_at(&types_v2, &["generate-lockfile", "--offline"]));
    success(&fixture.cli_at(&types_v2, &["freeze", "--package", "shop-types"]));
    let frozen_v2: JsonValue =
        serde_json::from_slice(&fs::read(types_v2.join(LEDGER)).unwrap()).unwrap();
    assert_eq!(
        frozen_v2["shop.receipt.created"]["1"],
        frozen_v1["shop.receipt.created"]["1"]
    );
    assert_eq!(
        frozen_v2["shop.receipt.created"].as_object().unwrap().len(),
        2
    );

    // Upgrade reporting while checkout continues to emit its original V1 bytes.
    let reporting_v2 = build_service(&fixture, "reporting-v2", &types_v2, "0.2.0", REPORTING_V2);
    assert_report(&reporting_v2, &queued_v1, 1200, "USD");
    let during_rollout = enqueue(owner, "receipt-during-rollout.json", &checkout_v1, None);
    assert_eq!(queued_value(&during_rollout), old_event);
    assert_report(&reporting_v2, &during_rollout, 1200, "USD");

    // After consumers upgrade, checkout can write both USD and EUR as V2.
    let checkout_v2 = build_service(&fixture, "checkout-v2", &types_v2, "0.2.0", CHECKOUT_V2);
    let mut queued_v2 = Vec::new();
    for currency in ["USD", "EUR"] {
        let path = enqueue(
            owner,
            &format!("receipt-v2-{currency}.json"),
            &checkout_v2,
            Some(currency),
        );
        assert_eq!(
            queued_value(&path),
            json!({
                "stable_name": "shop.receipt.created", "version": 2,
                "payload": {"amount_cents": 3000, "currency": currency}
            })
        );
        assert_report(&reporting_v2, &path, 3000, currency);
        assert_unsupported(&reporting_v1, &path);
        queued_v2.push((currency, path));
    }
    assert_report(&reporting_v2, &queued_v1, 1200, "USD");

    // Rolling checkout back does not remove V2 messages already in the queue.
    let rollback_event = enqueue(owner, "receipt-after-rollback.json", &checkout_v1, None);
    assert_eq!(queued_value(&rollback_event), old_event);
    assert_report(&reporting_v1, &rollback_event, 1200, "USD");
    assert_report(&reporting_v2, &rollback_event, 1200, "USD");
    for (currency, path) in queued_v2 {
        assert_report(&reporting_v2, &path, 3000, currency);
        assert_unsupported(&reporting_v1, &path);
    }

    assert_eq!(fs::read(checkout_v1).unwrap(), original_checkout);
    assert_eq!(fs::read(reporting_v1).unwrap(), original_reporting);
    for (name, bytes) in original_types {
        assert_eq!(
            fs::read(types_v1.join(name)).unwrap(),
            bytes,
            "unchanged V1 {name}"
        );
    }
}

fn build_service(
    fixture: &Fixture,
    name: &str,
    types: &Path,
    version: &str,
    source: &str,
) -> PathBuf {
    let owner = fixture.root().parent().unwrap();
    let service = owner.join(name);
    write(
        &service.join("Cargo.toml"),
        format!(
            r#"[package]
name = "{name}"
version = "0.0.0"
edition = "2024"
publish = false
[workspace]
[dependencies]
shop-types = {{ path = {types:?}, version = "={version}" }}
serde_json = "1.0.149"
"#
        ),
    );
    write(&service.join("src/main.rs"), source);
    success(&fixture.cargo_at(&service, &["generate-lockfile", "--offline"]));
    let output = fixture.cargo_at(
        &service,
        &["metadata", "--format-version", "1", "--locked", "--offline"],
    );
    success(&output);
    let metadata: JsonValue = serde_json::from_slice(&output.stdout).unwrap();
    let dependencies = metadata["packages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|package| package["name"] == "shop-types")
        .collect::<Vec<_>>();
    assert_eq!(
        dependencies.len(),
        1,
        "one shared-crate version per service"
    );
    assert_eq!(dependencies[0]["version"], version);
    assert_eq!(
        dependencies[0]["manifest_path"],
        types.join("Cargo.toml").to_str().unwrap()
    );
    success(&fixture.cargo_at(&service, &["build", "--bin", name, "--locked", "--offline"]));
    let executable = format!("{name}{}", std::env::consts::EXE_SUFFIX);
    let built = Path::new(metadata["target_directory"].as_str().unwrap())
        .join("debug")
        .join(&executable);
    let deployed = owner.join("deployed").join(executable);
    fs::create_dir_all(deployed.parent().unwrap()).unwrap();
    fs::copy(built, &deployed).expect("keep deployed binary outside Cargo's target");
    deployed
}

fn run(executable: &Path, argument: Option<&OsStr>) -> Output {
    let mut command = Command::new(executable);
    command.current_dir(executable.parent().unwrap());
    if let Some(argument) = argument {
        command.arg(argument);
    }
    command
        .output()
        .expect("run deployed service directly, without Cargo")
}

fn queued_value(path: &Path) -> JsonValue {
    serde_json::from_slice(&fs::read(path).unwrap()).expect("queued producer output")
}

fn enqueue(owner: &Path, name: &str, checkout: &Path, currency: Option<&str>) -> PathBuf {
    let output = run(checkout, currency.map(OsStr::new));
    success(&output);
    let path = owner.join("queue").join(name);
    // Deliver the producer's exact bytes, including its generated version metadata.
    write(&path, output.stdout);
    path
}

fn assert_report(reporting: &Path, queued: &Path, amount_cents: u64, currency: &str) {
    let output = run(reporting, Some(queued.as_os_str()));
    success(&output);
    let report: JsonValue = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        report,
        json!({"amount_cents": amount_cents, "currency": currency})
    );
}

fn assert_unsupported(reporting: &Path, queued: &Path) {
    let output = run(reporting, Some(queued.as_os_str()));
    failure(&output, "unsupported stored version");
    assert_eq!(output.status.code(), Some(2));
    assert!(
        output.stdout.is_empty(),
        "rejected events must not produce a report"
    );
    assert!(String::from_utf8(output.stderr)
        .unwrap()
        .contains("stored version 2"));
}

fn write(path: &Path, bytes: impl AsRef<[u8]>) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).expect("write owned fixture source or queued bytes");
}
