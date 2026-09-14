use super::diagnostics::report;
use super::support::{success, Fixture, LEDGER, STABLE_NAME, V1, V2};
use serde_json::{json, Value};

fn check(fixture: &Fixture, file: &str, ok: bool) -> Value {
    let source = fixture.read("src/lib.rs");
    let lock_existed = fixture.root().join("type-history/.schemas.lock").exists();
    let ledger = fixture.read(LEDGER);
    let released = std::fs::read(fixture.root().join(file)).ok();
    let result = report(
        &fixture.cli(&[
            "check",
            "--package",
            "standalone-history-consumer",
            "--released-baseline",
            file,
            "--format",
            "json",
        ]),
        ok,
    );
    if !ok {
        let repeated = report(
            &fixture.cli(&[
                "check",
                "--package",
                "standalone-history-consumer",
                "--released-baseline",
                file,
                "--format",
                "json",
            ]),
            false,
        );
        assert_eq!(
            repeated, result,
            "repeated failed checks retain the same report"
        );
    }
    assert_eq!(
        fixture.root().join("type-history/.schemas.lock").exists(),
        lock_existed
    );
    assert_eq!(fixture.read("src/lib.rs"), source);
    assert_eq!(fixture.read(LEDGER), ledger);
    assert_eq!(std::fs::read(fixture.root().join(file)).ok(), released);
    result
}
fn contains(report: &Value, code: &str) -> bool {
    report["packages"][0]["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .any(|d| d["code"] == code)
}

#[test]
fn released_baseline_detects_joint_edits_reset_and_missing_history_read_only() {
    let fixture = Fixture::frozen();
    fixture.remove("type-history/.schemas.lock");
    let released = fixture.read(LEDGER);
    fixture.write("../released.json", &released);
    check(&fixture, "../released.json", true);
    fixture.write("src/lib.rs", V2);
    success(&fixture.cli(&["freeze", "--package", "standalone-history-consumer"]));
    check(&fixture, "../released.json", true); // Later frozen versions are allowed.
    fixture.write("src/lib.rs", &(V2.to_owned() + "\n#[versioned(stable_name = \"billing.additional\")]\npub struct Additional { pub value: u32 }\n"));
    success(&fixture.cli(&["freeze", "--package", "standalone-history-consumer"]));
    check(&fixture, "../released.json", true); // Independent new histories are allowed too.
    let complete = fixture.read(LEDGER);
    fixture.write("../released.json", &complete);
    fixture.write(LEDGER, &released);
    fixture.write("src/lib.rs", V1);
    assert!(contains(
        &check(&fixture, "../released.json", false),
        "version_missing"
    ));
    fixture.write("../released.json", &released);
    fixture.write(LEDGER, "{}");
    fixture.write("src/lib.rs", "");
    assert!(contains(
        &check(&fixture, "../released.json", false),
        "history_missing"
    ));
    fixture.write(LEDGER, &released);
    fixture.write("src/lib.rs", V1);
    let mut current = fixture.ledger();
    current[STABLE_NAME]["1"]["reset_draft"] = json!(true);
    fixture.set_ledger(&current);
    assert!(contains(
        &check(&fixture, "../released.json", false),
        "released_reset"
    ));
    fixture.write("../released.json", &fixture.read(LEDGER));
    let simultaneous = check(&fixture, "../released.json", false);
    let resets: Vec<_> = simultaneous["packages"][0]["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|d| d["code"] == "released_reset")
        .collect();
    assert_eq!(
        resets.len(),
        2,
        "both authority resets remain visible: {simultaneous:#}"
    );
    let files: Vec<_> = resets
        .iter()
        .map(|d| d["location"]["file"].as_str().unwrap())
        .collect();
    assert!(files.contains(&fixture.root().join(LEDGER).to_str().unwrap()));
    assert!(files.contains(
        &fixture
            .root()
            .parent()
            .unwrap()
            .join("released.json")
            .to_str()
            .unwrap()
    ));
    fixture.write(LEDGER, &released);
    assert!(contains(
        &check(&fixture, "../released.json", false),
        "released_reset"
    ));
    fixture.write("../released.json", &released);
    let mut current: Value = serde_json::from_str(&released).unwrap();
    current[STABLE_NAME]["1"]["schema"]["properties"]["count"] =
        json!({"type":"integer", "format":"uint64", "minimum":0});
    fixture.set_ledger(&current);
    fixture.write("src/lib.rs", &V1.replace("count: u32", "count: u64"));
    success(&fixture.cargo(&["check", "--locked", "--offline"]));
    let changed = check(&fixture, "../released.json", false);
    assert!(contains(&changed, "shape_kind_changed"), "{changed:#}");
    let d = changed["packages"][0]["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["code"] == "shape_kind_changed")
        .unwrap();
    assert_eq!(d["origin"], "released_baseline");
    assert_eq!(d["path"], json!([{"kind":"field","name":"count"}]));
    assert_eq!(d["expected"], json!({"kind":"u32"}));
    assert_eq!(d["actual"], json!({"kind":"u64"}));
}

#[test]
fn released_baseline_rejects_bad_authority_and_current_path_aliases() {
    let fixture = Fixture::empty();
    success(&fixture.cli(&["init", "--package", "standalone-history-consumer"]));
    fixture.write("src/lib.rs", V1);
    success(&fixture.cargo(&["build", "--locked", "--offline"]));
    success(&fixture.cli(&["freeze", "--package", "standalone-history-consumer"]));
    let original = fixture.read(LEDGER);
    fixture.write("released.json", &original);
    check(&fixture, "released.json", true);
    for bytes in ["{", "[]", "{\"a\":{}}"] {
        fixture.write("released.json", bytes);
        assert!(contains(
            &check(&fixture, "released.json", false),
            "input_invalid"
        ));
    }
    let mut wrong_id = fixture.ledger();
    wrong_id[STABLE_NAME]["1"]["schema"]["$id"] = json!("wrong:id");
    fixture.write("released.json", &serde_json::to_string(&wrong_id).unwrap());
    assert!(contains(
        &check(&fixture, "released.json", false),
        "input_invalid"
    ));
    assert!(contains(
        &check(&fixture, "missing.json", false),
        "input_invalid"
    ));
    assert!(contains(&check(&fixture, LEDGER, false), "input_invalid"));
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(
            fixture.root().join(LEDGER),
            fixture.root().join("alias.json"),
        )
        .unwrap();
        assert!(contains(
            &check(&fixture, "alias.json", false),
            "input_invalid"
        ));

        use std::fs;
        use std::os::unix::fs::MetadataExt;

        let current = fixture.root().join(LEDGER);
        let hard_link = fixture.root().join("hard-link.json");
        fs::hard_link(&current, &hard_link).unwrap();
        assert_ne!(
            current.canonicalize().unwrap(),
            hard_link.canonicalize().unwrap()
        );
        let current_metadata = fs::metadata(&current).unwrap();
        let released_metadata = fs::metadata(&hard_link).unwrap();
        assert_eq!(
            (current_metadata.dev(), current_metadata.ino()),
            (released_metadata.dev(), released_metadata.ino())
        );
        let rejected = check(&fixture, "hard-link.json", false);
        let findings = rejected["packages"][0]["diagnostics"].as_array().unwrap();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0]["code"], "input_invalid");
        assert_eq!(findings[0]["origin"], "released_baseline");
        assert_eq!(findings[0]["location"]["file"], hard_link.to_str().unwrap());
        assert_eq!(
            findings[0]["hint"],
            "--released-baseline must be independent of the current ledger"
        );
    }
    assert_eq!(fixture.read(LEDGER), original);
}

#[test]
fn released_baseline_freshness_is_checked_after_compiler_failure() {
    let fixture = Fixture::frozen();
    let ledger = fixture.read(LEDGER);
    let source = fixture.read("src/lib.rs");
    fixture.write("../released.json", &ledger);
    // Model an external edit during compilation, then fail before export can finish.
    fixture.write(
        "build.rs",
        &format!(
            r#"
use std::fs;
fn main() {{
    fs::write({:?}, "external edit").unwrap();
    panic!("injected compiler failure after external edit");
}}
"#,
            fixture.root().join("../released.json")
        ),
    );
    let result = report(
        &fixture.cli(&[
            "check",
            "--package",
            "standalone-history-consumer",
            "--released-baseline",
            "../released.json",
            "--format",
            "json",
        ]),
        false,
    );
    assert!(contains(&result, "compiler_failed"), "{result:#}");
    assert!(contains(&result, "snapshot_stale"), "{result:#}");
    assert_eq!(fixture.read(LEDGER), ledger);
    assert_eq!(fixture.read("src/lib.rs"), source);
    assert_eq!(fixture.read("../released.json"), "external edit");
    assert!(!fixture
        .root()
        .join("type-history/.schemas.lock.stage")
        .exists());
}
