use super::support::{failure, success, Fixture, LEDGER};

const ENUMS: &str = include_str!("fixtures/enums.rs");

fn fixture() -> Fixture {
    let fixture = Fixture::empty();
    let manifest = fixture.read("Cargo.toml");
    fixture.write(
        "Cargo.toml",
        &manifest.replace(
            "serde_json = \"1.0.149\"",
            "serde_json = \"1.0.149\"\nrmp-serde = \"1.3.1\"",
        ),
    );
    success(&fixture.cargo(&["generate-lockfile", "--offline"]));
    success(&fixture.cli(&["init", "--package", "standalone-history-consumer"]));
    fixture
}

#[test]
fn supported_variants_containers_and_binary_payloads_use_public_versioned_codecs() {
    let fixture = fixture();
    fixture.write("src/lib.rs", ENUMS);
    success(&fixture.cargo(&["test", "--lib", "--locked", "--offline"]));
    success(&fixture.cli(&["freeze", "--package", "standalone-history-consumer"]));
    success(&fixture.cargo(&["test", "--lib", "--locked", "--offline"]));
}

#[test]
fn frozen_enum_edits_fail_with_a_warm_offline_consumer() {
    let fixture = fixture();
    fixture.write("src/lib.rs", ENUMS);
    success(&fixture.cli(&["freeze", "--package", "standalone-history-consumer"]));
    success(&fixture.cargo(&["build", "--locked", "--offline"]));
    let ledger = fixture.read(LEDGER);
    for (before, after) in [
        ("pub amount: u32", "pub amount: u64"),
        ("    Idle,", "    Idle,\n    Added,"),
        ("    Count(u32),", ""),
        ("Pair(u32, String)", "Pair(u32, String, u32)"),
        ("pub fixed: [Event; 1]", "pub fixed: [Event; 2]"),
        (
            "amount: u32, note: Option<String>",
            "amount: u32, note: String",
        ),
    ] {
        assert!(ENUMS.contains(before));
        fixture.write("src/lib.rs", &ENUMS.replace(before, after));
        failure(
            &fixture.cargo(&["build", "--locked", "--offline"]),
            "changed its frozen wire shape",
        );
        assert_eq!(fixture.read(LEDGER), ledger);
    }
    fixture.write(
        "src/lib.rs",
        &ENUMS.replace("Record(Details)", "Record { amount: u32 }"),
    );
    success(&fixture.cargo(&["build", "--locked", "--offline"]));
    assert_eq!(fixture.read(LEDGER), ledger);

    // Configuration selects a concrete supporting definition before trait resolution.
    // Inactive cfg members are removed by Rust before a derive can inspect them.
    let manifest = fixture.read("Cargo.toml");
    fixture.write(
        "Cargo.toml",
        &manifest.replace("[workspace]", "[features]\nalternate = []\n[workspace]"),
    );
    let configured = format!(
        r#"{}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Schema)]
pub enum ChoiceAlternate {{ First, Third }}
#[cfg(not(feature = "alternate"))]
pub use ChoiceDefault as Choice;
#[cfg(feature = "alternate")]
pub use ChoiceAlternate as Choice;
"#,
        ENUMS.replace("pub enum Choice {", "pub enum ChoiceDefault {")
    );
    fixture.write("src/lib.rs", &configured);
    success(&fixture.cargo(&["build", "--locked", "--offline"]));
    failure(
        &fixture.cargo(&["build", "--locked", "--offline", "--features", "alternate"]),
        "changed its frozen wire shape",
    );
    assert_eq!(fixture.read(LEDGER), ledger);
}

#[test]
fn historical_enum_updates_borrow_the_complete_previous_record() {
    let fixture = fixture();
    fixture.write("src/lib.rs", include_str!("fixtures/enums_v1.rs"));
    success(&fixture.cli(&["freeze", "--package", "standalone-history-consumer"]));
    let source = include_str!("fixtures/enums_v2.rs");
    fixture.write("src/lib.rs", source);
    success(&fixture.cargo(&["test", "--lib", "--locked", "--offline"]));
    success(&fixture.cli(&["freeze", "--package", "standalone-history-consumer"]));
    for (before, after) in [
        ("u64::from(amount)", "0"),
        ("old.currency.clone()", "String::from(\"EUR\")"),
    ] {
        assert!(source.contains(before));
        fixture.write("src/lib.rs", &source.replace(before, after));
        failure(
            &fixture.cargo(&["test", "--lib", "--locked", "--offline"]),
            "assertion `left == right` failed",
        );
    }
    fixture.write("src/lib.rs", source);
    success(&fixture.cargo(&["test", "--lib", "--locked", "--offline"]));
}

#[test]
fn declarations_reject_unsupported_enum_and_record_authoring() {
    let fixture = fixture();
    for (source, expected) in [
        (
            "#[history_api::versioned(stable_name = \"bad\")] enum Bad { One }",
            "history requires a concrete named record",
        ),
        (
            "#[derive(history_api::Schema)] enum Bad<T> { One(T) }",
            "generic",
        ),
        (
            "#[derive(history_api::Schema)] struct Bad();",
            "Schema tuple structs require at least one field",
        ),
        (
            "#[derive(history_api::Schema)] enum Bad {}",
            "Schema requires at least one enum variant",
        ),
        (
            "#[derive(history_api::Schema)] enum Bad { One() }",
            "Schema tuple variants require at least one field",
        ),
        (
            "#[derive(history_api::Schema)] enum Bad { #[history(added_in = v2)] One }",
            "Schema does not support history attributes",
        ),
        (
            "#[derive(history_api::Schema, serde::Serialize)] #[serde(tag = \"kind\")] enum Bad { One }",
            "Schema supports only",
        ),
        (
            "#[derive(history_api::Schema, serde::Serialize)] #[serde(untagged)] enum Bad { One(u32) }",
            "Schema supports only",
        ),
        (
            "#[derive(history_api::Schema, serde::Serialize)] enum Bad { #[serde(rename = \"other\")] One }",
            "Schema supports only",
        ),
        (
            "#[derive(history_api::Schema, serde::Serialize)] enum Bad { One { #[serde(default)] amount: u32 } }",
            "Schema supports only",
        ),
    ] {
        fixture.write("src/lib.rs", source);
        failure(
            &fixture.cargo(&["build", "--locked", "--offline"]),
            expected,
        );
    }
}
