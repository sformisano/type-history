//! Public consumers share one owned fixture and the existing serial Cargo lane.

#[path = "field_support/contracts.rs"]
mod contracts;
#[path = "field_support/derive_options.rs"]
mod derive_options;
#[path = "field_support/features.rs"]
mod features;
#[path = "field_support/profiles.rs"]
mod profiles;

use super::support::{success, Fixture, LEDGER};
use serde_json::Value;

const PACKAGE: &str = "standalone-history-consumer";

fn fixture(features: &[&str], dependencies: &str) -> Fixture {
    let fixture = Fixture::empty();
    let mut manifest = fixture.read("Cargo.toml");
    assert_eq!(
        manifest
            .matches("package = \"type-history\", path =")
            .count(),
        1
    );
    manifest = manifest.replace(
        "package = \"type-history\", path =",
        &format!("package = \"type-history\", features = {features:?}, path ="),
    );
    assert_eq!(manifest.matches("[build-dependencies]").count(), 1);
    manifest = manifest.replace(
        "[build-dependencies]",
        &format!("rmp-serde = \"1.3.1\"\n{dependencies}\n[build-dependencies]"),
    );
    fixture.write("Cargo.toml", &manifest);
    success(&fixture.cargo(&["generate-lockfile", "--offline"]));
    success(&fixture.cli(&["init", "--package", PACKAGE]));
    fixture.write("src/common.rs", include_str!("fixtures/fields/common.rs"));
    fixture
}

fn tests(fixture: &Fixture) {
    success(&fixture.cargo(&["test", "--lib", "--locked", "--offline"]));
}

fn freeze(fixture: &Fixture) {
    success(&fixture.cli(&["freeze", "--package", PACKAGE]));
}

fn check(fixture: &Fixture) {
    success(&fixture.cli(&["check", "--package", PACKAGE]));
}

fn replace(source: &str, before: &str, after: &str) -> String {
    assert_eq!(
        source.matches(before).count(),
        1,
        "unique fixture edit: {before}"
    );
    source.replace(before, after)
}

fn frozen_failure(fixture: &Fixture, source: &str, expected: &str) {
    let ledger = fixture.read(LEDGER);
    fixture.write("src/lib.rs", source);
    super::support::failure(
        &fixture.cargo(&["check", "--locked", "--offline"]),
        expected,
    );
    let output = fixture.cli(&["check", "--package", PACKAGE, "--format", "json"]);
    super::support::failure(&output, "history check failed");
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    let differences = report["packages"][0]["diagnostics"].as_array().unwrap();
    assert!(!differences.is_empty(), "{report:#}");
    assert!(
        differences.iter().all(|difference| {
            difference["origin"] == "current_ledger"
                && difference["stable_name"].is_string()
                && difference["expected"] != difference["actual"]
        }),
        "{report:#}"
    );
    assert_eq!(fixture.read(LEDGER), ledger, "failed check changed ledger");
}
