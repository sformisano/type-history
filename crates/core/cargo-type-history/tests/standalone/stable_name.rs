use super::support::{failure, success, Fixture, LEDGER, STABLE_NAME, V1};
use serde_json::Value;

const CHANGED_NAME: &str = "billing.invoice.reissued";

#[test]
fn simple_and_two_segment_names_freeze_evolve_and_keep_their_name() {
    const SOURCE: &str = include_str!("fixtures/short-names.rs");
    const ADDED_FIELD: &str =
        "    #[history(added_in = v2, backfill_value = 7_u32)]\n    pub revision: u32,\n";
    const PACKAGE: &str = "standalone-history-consumer";

    let fixture = Fixture::empty();
    success(&fixture.cli(&["init", "--package", PACKAGE]));
    let declarations = SOURCE.split_once("#[cfg(test)]").unwrap().0;
    assert_eq!(declarations.matches(ADDED_FIELD).count(), 2);
    let v1 = declarations.replace(ADDED_FIELD, "");
    fixture.write("src/lib.rs", &v1);
    success(&fixture.cli(&["freeze", "--package", PACKAGE]));
    let frozen_v1 = fixture.ledger();
    for id in ["receipt_created", "shop.receipt"] {
        assert!(frozen_v1[id]["1"].is_object());
    }

    let frozen = fixture.read(LEDGER);
    fixture.write(
        "src/lib.rs",
        &v1.replace("receipt_created", "receipt_reissued"),
    );
    failure(
        &fixture.cargo(&["build", "--locked", "--offline"]),
        "committed history exists",
    );
    failure(
        &fixture.cli(&["freeze", "--package", PACKAGE]),
        "committed history exists",
    );
    assert_eq!(fixture.read(LEDGER), frozen);

    fixture.write("src/lib.rs", SOURCE);
    success(&fixture.cli(&["freeze", "--package", PACKAGE]));
    let frozen_v2 = fixture.ledger();
    for id in ["receipt_created", "shop.receipt"] {
        assert_eq!(frozen_v2[id]["1"], frozen_v1[id]["1"]);
        assert!(frozen_v2[id]["2"].is_object());
    }
    success(&fixture.cargo(&["test", "--lib", "--locked", "--offline"]));
    success(&fixture.cargo(&["build", "--release", "--locked", "--offline"]));
    success(&fixture.cli(&["check", "--package", PACKAGE]));
}

#[test]
fn frozen_stable_name_edits_are_rejected_by_every_lifecycle_path() {
    let fixture = Fixture::frozen();
    let frozen = fixture.read(LEDGER);

    let changed_source = V1.replace(STABLE_NAME, CHANGED_NAME);
    fixture.write("src/lib.rs", &changed_source);
    failure(
        &fixture.cargo(&["build", "--locked", "--offline"]),
        "committed history exists",
    );
    failure(
        &fixture.cli(&["check", "--package", "standalone-history-consumer"]),
        "committed history exists",
    );
    failure(
        &fixture.cli(&["freeze", "--package", "standalone-history-consumer"]),
        "committed history exists",
    );
    failure(
        &fixture.cli(&[
            "reset",
            "--package",
            "standalone-history-consumer",
            "--type",
            STABLE_NAME,
            "--version",
            "1",
        ]),
        "committed history exists",
    );

    let mut renamed_import = fixture.ledger();
    let versions = renamed_import
        .as_object_mut()
        .unwrap()
        .remove(STABLE_NAME)
        .unwrap();
    let mut versions = versions.as_object().unwrap().clone();
    for snapshot in versions.values_mut() {
        snapshot["schema"]["$id"] = format!("urn:typehistory:schema:{CHANGED_NAME}").into();
    }
    renamed_import
        .as_object_mut()
        .unwrap()
        .insert(CHANGED_NAME.to_owned(), Value::Object(versions));
    fixture.write(
        "renamed-import.json",
        &serde_json::to_string_pretty(&renamed_import).unwrap(),
    );
    failure(
        &fixture.cli(&[
            "import",
            "--package",
            "standalone-history-consumer",
            "--from",
            "renamed-import.json",
        ]),
        "import changes or deletes existing authority",
    );
    assert_eq!(fixture.read(LEDGER), frozen);

    fixture.write("src/lib.rs", V1);
    success(&fixture.cli(&[
        "reset",
        "--package",
        "standalone-history-consumer",
        "--type",
        STABLE_NAME,
        "--version",
        "1",
    ]));
    let reserved = fixture.read(LEDGER);
    fixture.write("src/lib.rs", &changed_source);
    failure(
        &fixture.cargo(&["build", "--locked", "--offline"]),
        "committed history exists",
    );
    failure(
        &fixture.cli(&[
            "freeze",
            "--package",
            "standalone-history-consumer",
            "--type",
            STABLE_NAME,
            "--version",
            "1",
        ]),
        "committed history exists",
    );
    failure(
        &fixture.cli(&[
            "reset",
            "--undo",
            "--package",
            "standalone-history-consumer",
            "--type",
            STABLE_NAME,
            "--version",
            "1",
        ]),
        "committed history exists",
    );
    assert_eq!(fixture.read(LEDGER), reserved);
}

#[test]
fn rust_record_rename_keeps_the_stable_name_and_exact_ledger() {
    let fixture = Fixture::frozen();
    let frozen = fixture.read(LEDGER);
    let renamed = V1.replace("pub struct Invoice", "pub struct Receipt");
    fixture.write(
        "src/lib.rs",
        &format!(
            r#"{renamed}
#[cfg(test)]
mod renamed_record_tests {{
    use super::{{Receipt, ReceiptV1}};
    use history_api::HasHistory;

    #[test]
    fn stable_name_is_independent_of_the_rust_name() {{
        assert_eq!(Receipt::STABLE_NAME.as_str(), "{STABLE_NAME}");
        let value: Receipt = ReceiptV1 {{ legacy: "legacy".to_owned(), count: 7 }};
        assert_eq!(value.count, 7);
    }}
}}
"#
        ),
    );

    success(&fixture.cargo(&["test", "--lib", "--locked", "--offline"]));
    success(&fixture.cargo(&["build", "--release", "--locked", "--offline"]));
    success(&fixture.cli(&["check", "--package", "standalone-history-consumer"]));
    success(&fixture.cli(&["freeze", "--package", "standalone-history-consumer"]));
    assert_eq!(fixture.read(LEDGER), frozen);
}
