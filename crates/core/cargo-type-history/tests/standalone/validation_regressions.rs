use super::support::{failure, success, Fixture, LEDGER, V1};

#[test]
fn newly_staged_candidate_must_pass_ordinary_validation_before_commit() {
    let fixture = Fixture::frozen();
    let before = fixture.read(LEDGER);
    let changed = format!(
        "#[cfg(test)] type Added = u32;\n#[cfg(not(test))] type Added = String;\n{}",
        V1.replace("pub count: u32,", "pub count: u32,\n    #[history(added_in = v2, backfill_value = Added::default())]\n    pub added: Added,")
    );
    fixture.write("src/lib.rs", &changed);
    failure(
        &fixture.cli(&["freeze", "--package", "standalone-history-consumer"]),
        "frozen wire shape",
    );
    assert_eq!(fixture.read(LEDGER), before);
    assert_eq!(fixture.read("src/lib.rs"), changed);
    fixture.write(
        "src/lib.rs",
        &changed.replace("type Added = String", "type Added = u32"),
    );
    success(&fixture.cli(&["freeze", "--package", "standalone-history-consumer"]));
    assert_ne!(fixture.read(LEDGER), before);
}

#[test]
fn check_and_unchanged_freeze_validate_the_ordinary_library() {
    let fixture = Fixture::frozen();
    let before = fixture.read(LEDGER);
    let changed = format!(
        "#[cfg(test)] type Count = u32;\n#[cfg(not(test))] type Count = String;\n{}",
        V1.replace("count: u32", "count: Count")
    );
    fixture.write("src/lib.rs", &changed);
    for action in ["check", "freeze"] {
        failure(
            &fixture.cli(&[action, "--package", "standalone-history-consumer"]),
            "frozen wire shape",
        );
        assert_eq!(fixture.read(LEDGER), before);
    }
    fixture.write("src/lib.rs", V1);
    for action in ["check", "freeze"] {
        success(&fixture.cli(&[action, "--package", "standalone-history-consumer"]));
        assert_eq!(fixture.read(LEDGER), before);
    }
}
