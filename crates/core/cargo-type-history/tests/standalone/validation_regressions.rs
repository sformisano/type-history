use super::support::{failure, success, Fixture, LEDGER, V1};

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
