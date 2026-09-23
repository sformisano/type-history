use super::support::{success, Fixture, LEDGER};
use serde_json::{json, Value};
use std::process::Output;

const NESTED: &str = include_str!("fixtures/diagnostic-nested.rs");

#[test]
fn check_json_preserves_workspace_member_source_locations() {
    let fixture = Fixture::empty();
    let manifest = fixture.read("Cargo.toml").replace("[workspace]\n", "");
    let manifest = manifest.split("[profile.release_child]").next().unwrap();
    fixture.write("members/history/Cargo.toml", manifest);
    fixture.write("members/history/build.rs", &fixture.read("build.rs"));
    fixture.write("members/history/src/lib.rs", "");
    fixture.write(
        "Cargo.toml",
        "[workspace]\nmembers = ['members/history']\nresolver = '2'\n",
    );
    success(&fixture.cargo(&["generate-lockfile", "--offline"]));
    success(&fixture.cli(&["init", "--package", "standalone-history-consumer"]));
    fixture.write("members/history/src/lib.rs", NESTED);
    success(&fixture.cli(&["freeze", "--package", "standalone-history-consumer"]));
    let ledger_path = format!("members/history/{LEDGER}");
    let ledger = fixture.read(&ledger_path);
    let args = [
        "check",
        "--package",
        "standalone-history-consumer",
        "--format",
        "json",
    ];
    let source = fixture.root().join("members/history/src/lib.rs");
    let changed = NESTED.replace("reference: u32", "reference: u64");
    assert_ne!(changed, NESTED);
    fixture.write("members/history/src/lib.rs", &changed);
    let frozen = report(&fixture.cli(&args), false);
    let findings = frozen["packages"][0]["diagnostics"].as_array().unwrap();
    assert!(findings.iter().any(|d| d["code"] == "shape_kind_changed"));
    for finding in findings {
        assert_eq!(finding["location"]["file"], source.to_str().unwrap());
        let expected_line = NESTED
            .lines()
            .position(|line| line.contains("pub payment:"))
            .unwrap()
            + 1;
        assert_eq!(finding["location"]["line"], expected_line);
        assert_eq!(finding["origin"], "current_ledger");
    }
    let broken = format!("{NESTED}\npub fn broken() {{ let _: u32 = false; }}\n");
    let line = broken
        .lines()
        .position(|line| line.contains("pub fn broken"))
        .unwrap()
        + 1;
    fixture.write("members/history/src/lib.rs", &broken);
    let failed = report(
        &fixture.cli_at(&fixture.root().join("members/history"), &args),
        false,
    );
    let findings = failed["packages"][0]["diagnostics"].as_array().unwrap();
    assert_eq!(findings.len(), 1, "{failed:#}");
    assert_eq!(findings[0]["code"], "compiler_failed");
    assert_eq!(findings[0]["location"]["file"], source.to_str().unwrap());
    assert_eq!(findings[0]["location"]["line"], line);
    assert_eq!(fixture.read(&ledger_path), ledger);
    assert_eq!(fixture.read("members/history/src/lib.rs"), broken);
}

pub(super) fn report(output: &Output, ok: bool) -> Value {
    assert_eq!(
        output.status.success(),
        ok,
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "one parseable JSON document: {error}\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    });
    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["command"], "check");
    assert_eq!(report["ok"], ok);
    if !ok {
        assert_eq!(output.status.code(), Some(2));
    }
    report
}

#[test]
fn check_json_reports_nested_enum_differences_and_original_locations() {
    let fixture = Fixture::empty();
    success(&fixture.cli(&["init", "--package", "standalone-history-consumer"]));
    fixture.write("src/lib.rs", NESTED);
    success(&fixture.cli(&["freeze", "--package", "standalone-history-consumer"]));
    let ledger = fixture.read(LEDGER);
    fixture.write("released.json", &ledger);
    let changed = NESTED
        .replace("amount: u32", "amount: u64")
        .replace("active: bool", "active: String")
        .replace("reference: u32", "reference: u64")
        .replace("[u32; 2]", "[u32; 3]");
    fixture.write("src/lib.rs", &changed);
    let args = [
        "check",
        "--package",
        "standalone-history-consumer",
        "--format",
        "json",
    ];
    let first = report(&fixture.cli(&args), false);
    let diagnostics = first["packages"][0]["diagnostics"].as_array().unwrap();
    let shapes: Vec<_> = diagnostics
        .iter()
        .filter(|d| d["code"] == "shape_kind_changed" || d["code"] == "array_length_changed")
        .collect();
    assert_eq!(shapes.len(), 6, "{first:#}");
    let nested_path = json!([{ "kind":"field", "name":"details" }, { "kind":"option_value" }, { "kind":"sequence_item" }, { "kind":"field", "name":"amount" }]);
    let finding = shapes
        .iter()
        .find(|d| d["path"] == nested_path)
        .expect("full nested path");
    assert_eq!(finding["expected"], json!({"kind":"u32"}));
    assert_eq!(finding["actual"], json!({"kind":"u64"}));
    for d in shapes {
        assert_eq!(d["origin"], "current_ledger");
        assert_eq!(d["stable_name"], "billing.diagnostic");
        assert_eq!(d["version"], 1);
        assert!(d["hint"].as_str().is_some_and(|s| !s.is_empty()));
        assert_eq!(
            d["location"]["file"],
            fixture.root().join("src/lib.rs").to_str().unwrap()
        );
        let field = d["path"][0]["name"].as_str().unwrap();
        let expected_line = NESTED
            .lines()
            .position(|line| line.contains(&format!("pub {field}:")))
            .unwrap()
            + 1;
        assert_eq!(d["location"]["line"], expected_line);
    }
    assert_eq!(
        report(&fixture.cli(&args), false),
        first,
        "deterministic repeated failed checks"
    );
    assert_eq!(fixture.read(LEDGER), ledger);
    assert_eq!(fixture.read("src/lib.rs"), changed);
    fixture.write(
        "src/lib.rs",
        &(NESTED.to_owned() + "\npub fn broken() { let _: u32 = false; }\npub fn broken_again() { let _: bool = 3_u8; }\n"),
    );
    let failed = report(&fixture.cli(&args), false);
    assert!(failed["packages"][0]["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .any(|d| d["code"] == "compiler_failed"
            && d["location"]["file"] == fixture.root().join("src/lib.rs").to_str().unwrap()));
    let compiler_errors: Vec<_> = failed["packages"][0]["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|d| {
            d["code"] == "compiler_failed"
                && d["location"]["file"] == fixture.root().join("src/lib.rs").to_str().unwrap()
        })
        .collect();
    assert_eq!(
        compiler_errors.len(),
        2,
        "independent compiler errors survive aggregation: {failed:#}"
    );
    assert_ne!(
        compiler_errors[0]["location"]["line"],
        compiler_errors[1]["location"]["line"]
    );
    assert_eq!(fixture.read(LEDGER), ledger);
    fixture.write("src/lib.rs", &(NESTED.to_owned() + "\npub fn broken( {\n"));
    let syntax = report(&fixture.cli(&args), false);
    assert!(syntax["packages"][0]["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .any(|d| d["code"] == "compiler_failed"));
    assert!(!syntax["packages"][0]["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .any(|d| d["code"] == "shape_kind_changed"));
    assert_eq!(fixture.read(LEDGER), ledger);
    fixture.write("src/lib.rs", &changed);
    success(&fixture.cli(&[
        "reset",
        "--package",
        "standalone-history-consumer",
        "--type",
        "billing.diagnostic",
        "--version",
        "1",
    ]));
    success(&fixture.cli(&[
        "freeze",
        "--package",
        "standalone-history-consumer",
        "--type",
        "billing.diagnostic",
        "--version",
        "1",
    ]));
    success(&fixture.cargo(&["check", "--locked", "--offline"]));
    let joint_ledger = fixture.read(LEDGER);
    let released_args = [
        "check",
        "--package",
        "standalone-history-consumer",
        "--released-baseline",
        "released.json",
        "--format",
        "json",
    ];
    let joint = report(&fixture.cli(&released_args), false);
    let findings = joint["packages"][0]["diagnostics"].as_array().unwrap();
    assert_eq!(findings.len(), 6, "{joint:#}");
    assert!(findings.iter().all(|d| d["origin"] == "released_baseline"));
    let nested = findings.iter().find(|d| d["path"] == nested_path).unwrap();
    assert_eq!(nested["expected"], json!({"kind":"u32"}));
    assert_eq!(nested["actual"], json!({"kind":"u64"}));
    assert_eq!(report(&fixture.cli(&released_args), false), joint);
    assert_eq!(fixture.read("src/lib.rs"), changed);
    assert_eq!(fixture.read(LEDGER), joint_ledger);
    assert_eq!(fixture.read("released.json"), ledger);
}

#[test]
fn check_json_covers_invalid_invocations_and_cargo_failures() {
    let fixture = Fixture::empty();
    for args in [
        vec!["check", "--format", "json", "--released-baseline"],
        vec!["check", "--format", "json", "--released-baseline", "a.json"],
        vec![
            "check",
            "--format",
            "json",
            "--released-baseline",
            "a.json",
            "--released-baseline",
            "b.json",
            "--package",
            "standalone-history-consumer",
        ],
        vec!["check", "--format", "json", "--format", "json"],
        vec!["check", "--format", "json", "--format", "xml"],
        vec![
            "freeze",
            "--format",
            "json",
            "--package",
            "standalone-history-consumer",
        ],
        vec!["check", "--format", "json", "--package", "*"],
        vec!["check", "--format", "json", "--unknown", "x"],
    ] {
        let json = report(&fixture.cli(&args), false);
        assert_eq!(json["errors"][0]["code"], "input_invalid", "{json:#}");
        assert_eq!(json["packages"], json!([]));
    }
    fixture.write(LEDGER, "invalid current ledger");
    fixture.write(
        "other/Cargo.toml",
        "[package]\nname = 'zz-second'\nversion = '0.0.0'\nedition = '2024'\n",
    );
    fixture.write("other/src/lib.rs", "pub struct Plain;");
    fixture.write("other/type-history/schemas.json", "{}");
    fixture.write(
        "Cargo.toml",
        &fixture
            .read("Cargo.toml")
            .replace("[workspace]", "[workspace]\nmembers = ['other']"),
    );
    success(&fixture.cargo(&["generate-lockfile", "--offline"]));
    let aggregate = report(&fixture.cli(&["check", "--format", "json"]), false);
    assert_eq!(aggregate["packages"].as_array().unwrap().len(), 2);
    assert_eq!(
        aggregate["packages"][0]["package"],
        "standalone-history-consumer"
    );
    assert_eq!(aggregate["packages"][1]["package"], "zz-second");
    assert_eq!(aggregate["packages"][1]["diagnostics"], json!([]));
    fixture.write("Cargo.toml", "invalid = [");
    let json = report(&fixture.cli(&["check", "--format", "json"]), false);
    assert_eq!(json["errors"][0]["code"], "compiler_failed");
}
