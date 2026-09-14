//! Compile tutorial Rust blocks and compare documented artifacts with real output.

#[path = "tutorial/snippets.rs"]
mod snippets;

use super::support::{failure, success, Fixture, LEDGER};
use serde_json::{from_str, json, Value as JsonValue};
use toml::Value;

const DOCUMENTATION: &[&str] = &[
    include_str!("../../../../../README.md"),
    include_str!("../../../../../book/src/guide.md"),
    include_str!("../../../../../book/src/field-history.md"),
    include_str!("../../../../../book/src/decoding.md"),
    include_str!("../../../../../book/src/lifecycle.md"),
    include_str!("../../../../../book/src/integration.md"),
    include_str!("../../../../../book/src/setup.md"),
];
const PACKAGE: &str = "standalone-history-consumer";

fn documented(marker: &str) -> &str {
    documented_block(marker, "rust")
}

fn documented_json(marker: &str) -> JsonValue {
    from_str(documented_block(marker, "json")).expect("documented JSON")
}

pub(super) fn documented_block(marker: &str, language: &str) -> &'static str {
    let marker = format!("<!-- {marker} -->\n```{language}\n");
    assert_eq!(
        DOCUMENTATION
            .iter()
            .map(|document| document.matches(&marker).count())
            .sum::<usize>(),
        1,
        "unique source marker: {marker}"
    );
    DOCUMENTATION
        .iter()
        .find_map(|document| document.split_once(&marker))
        .expect("documented source")
        .1
        .split_once("```\n")
        .expect("closed code block")
        .0
}

fn fixture() -> Fixture {
    let fixture = Fixture::empty();
    let mut manifest: Value = toml::from_str(&fixture.read("Cargo.toml")).unwrap();
    let dependencies = manifest["dependencies"].as_table_mut().unwrap();
    let runtime = dependencies.remove("history_api").unwrap();
    dependencies.insert("type-history".into(), runtime);
    fixture.write("Cargo.toml", &toml::to_string(&manifest).unwrap());
    success(&fixture.cli(&["init", "--package", PACKAGE]));
    fixture
}

#[test]
fn receipt_tutorial_reads_old_bytes_and_writes_current_payloads() {
    let fixture = fixture();
    assert_eq!(fixture.ledger(), json!({}));
    let mut manifest: Value = toml::from_str(&fixture.read("Cargo.toml")).unwrap();
    manifest["dependencies"]
        .as_table_mut()
        .unwrap()
        .insert("rmp-serde".into(), "1.3.1".into());
    fixture.write("Cargo.toml", &toml::to_string(&manifest).unwrap());
    success(&fixture.cargo(&["generate-lockfile", "--offline"]));
    fixture.write("src/lib.rs", documented("quick-example:v1.rs"));
    success(&fixture.cargo(&["build", "--locked", "--offline"]));
    success(&fixture.cli(&["freeze", "--package", PACKAGE]));
    let frozen_v1 = fixture.read(LEDGER);
    assert_eq!(fixture.ledger(), documented_json("journey:ledger-v1.json"));
    let v2 = documented("quick-example:v2.rs");
    let currency_history = "#[history(added_in = v2, backfill_value = \"USD\".to_owned())]";
    assert_eq!(v2.matches(currency_history).count(), 1);
    fixture.write("src/lib.rs", &v2.replacen(currency_history, "", 1));
    failure(
        &fixture.cargo(&["build", "--locked", "--offline"]),
        "field `currency` did not exist in frozen history",
    );
    assert_eq!(fixture.read(LEDGER), frozen_v1);
    let currency_field = format!("{currency_history}\n    pub currency: String,");
    assert_eq!(v2.matches(&currency_field).count(), 1);
    fixture.write(
        "src/lib.rs",
        &v2.replacen(&currency_field, documented("journey:wrong-backfill.rs"), 1),
    );
    failure(
        &fixture.cargo(&["build", "--locked", "--offline"]),
        documented_block("journey:backfill-error.txt", "text").trim(),
    );
    assert_eq!(fixture.read(LEDGER), frozen_v1);
    fixture.write(
        "src/lib.rs",
        &format!(
            "{}{}",
            documented("quick-example:v2.rs"),
            snippets::receipt_tests()
        ),
    );
    success(&fixture.cargo(&["test", "--lib", "--locked", "--offline"]));
    success(&fixture.cli(&[
        "freeze",
        "--package",
        PACKAGE,
        "--type",
        "shop.receipt.created",
        "--version",
        "2",
    ]));
    let frozen_v2 = fixture.ledger();
    let history = &frozen_v2["shop.receipt.created"];
    assert_eq!(history.as_object().unwrap().len(), 2);
    assert_eq!(
        history["1"],
        documented_json("journey:ledger-v1.json")["shop.receipt.created"]["1"]
    );
    assert_eq!(frozen_v2, documented_json("journey:ledger-v2.json"));
    assert_eq!(
        history["2"]["schema"]["required"],
        json!(["amount_cents", "currency"])
    );
    success(&fixture.cargo(&["build", "--release", "--locked", "--offline"]));
    success(&fixture.cli(&["check", "--package", PACKAGE]));
    fixture.write("src/lib.rs", &snippets::receipt_v3());
    success(&fixture.cargo(&["test", "--lib", "--locked", "--offline"]));

    // The field reference and README explore different drafts after frozen V2.
    // Exercise the README's signed amount before freezing its V3 for release.
    fixture.write(
        "src/lib.rs",
        &format!(
            "{}{}",
            documented("overview:v3.rs"),
            snippets::receipt_journey_tests()
        ),
    );
    success(&fixture.cargo(&["test", "--lib", "--locked", "--offline"]));
    assert_eq!(fixture.ledger(), frozen_v2, "V3 is still a draft");
    success(&fixture.cli(&["freeze", "--package", PACKAGE]));
    let frozen_v3 = fixture.ledger();
    let history = &frozen_v3["shop.receipt.created"];
    assert_eq!(history.as_object().unwrap().len(), 3);
    for version in ["1", "2"] {
        assert_eq!(history[version], frozen_v2["shop.receipt.created"][version]);
    }
    assert_eq!(frozen_v3, documented_json("journey:ledger-v3.json"));
    success(&fixture.cargo(&["build", "--release", "--locked", "--offline"]));
    success(&fixture.cli(&["check", "--package", PACKAGE]));
}

#[test]
fn invoice_tutorial_checks_evolution_rejections_and_lifecycle() {
    let fixture = fixture();
    fixture.write("src/lib.rs", documented("example:versions/v1.rs"));
    success(&fixture.cargo(&["build", "--locked", "--offline"]));
    success(&fixture.cli(&["freeze", "--package", PACKAGE]));
    let frozen_v1 = fixture.read(LEDGER);
    let v2 = documented("example:versions/v2.rs");
    for (before, after, diagnostic) in [
        (", backfill_value = 7_u32", "", "a history record requires"),
        ("previous_type = u32", "from = u32", "unknown history key"),
        (", backfill_fn = widen", "", "a history record requires"),
        (
            "backfill_value = 7_u32",
            "backfill_value = 7_u32, backfill_fn = label",
            "exactly one of",
        ),
        (
            "fn widen(previous: &InvoiceV1)",
            "fn widen(previous: InvoiceV1)",
            "mismatched types",
        ),
        (
            "previous: &InvoiceV1",
            "previous: &InvoiceV2",
            "mismatched types",
        ),
        (
            "updated_in = v2",
            "updated_in = v0",
            "history versions are positive",
        ),
        (
            "updated_in = v2",
            "updated_in = v4294967295",
            "exceeds the one permitted successor V2",
        ),
    ] {
        let invalid = v2.replace(before, after);
        assert_ne!(invalid, v2, "the rejected edit must change the source");
        fixture.write("src/lib.rs", &invalid);
        failure(
            &fixture.cargo(&["build", "--locked", "--offline"]),
            diagnostic,
        );
        assert_eq!(fixture.read(LEDGER), frozen_v1);
    }
    let invalid = v2
        .replace("Result<u64, ConvertError>", "Result<String, ConvertError>")
        .replace(
            "Ok(u64::from(previous.count))",
            "Ok(previous.count.to_string())",
        );
    assert!(
        v2.contains("Result<u64, ConvertError>") && v2.contains("Ok(u64::from(previous.count))")
    );
    fixture.write("src/lib.rs", &invalid);
    failure(
        &fixture.cargo(&["build", "--locked", "--offline"]),
        "mismatched types",
    );
    assert_eq!(fixture.read(LEDGER), frozen_v1);

    fixture.write("src/lib.rs", v2);
    success(&fixture.cargo(&["build", "--locked", "--offline"]));
    failure(
        &fixture.cargo_env(
            &["build", "--locked", "--offline"],
            &[("TYPE_HISTORY_REQUIRE_FROZEN", Some("1"))],
        ),
        "draft",
    );
    for profile in ["release", "release_child"] {
        failure(
            &fixture.cargo(&["build", "--profile", profile, "--locked", "--offline"]),
            "draft",
        );
    }
    success(&fixture.cargo(&["build", "--profile", "dev_child", "--locked", "--offline"]));
    failure(
        &fixture.cargo_env(
            &["build", "--locked", "--offline"],
            &[("TYPE_HISTORY_REQUIRE_FROZEN", Some("0"))],
        ),
        "must be unset or exactly 1",
    );
    success(&fixture.cli(&[
        "freeze",
        "--package",
        PACKAGE,
        "--type",
        "billing.invoice.issued",
        "--version",
        "2",
    ]));
    let frozen_v2 = fixture.read(LEDGER);
    let v3 = documented("example:versions/v3.rs");
    snippets::check_invoice_fragments(v3);
    fixture.write("src/lib.rs", v3);
    success(&fixture.cargo(&["build", "--locked", "--offline"]));
    fixture.write("src/lib.rs", v2);
    success(&fixture.cargo(&["build", "--release", "--locked", "--offline"]));
    assert_eq!(fixture.read(LEDGER), frozen_v2);

    let current = format!(
        "{v3}{}{}",
        documented("example:runtime-tests.rs"),
        snippets::invoice_tests()
    );
    fixture.write("src/lib.rs", &current);
    success(&fixture.cargo(&["test", "--lib", "--locked", "--offline"]));
    success(&fixture.cli(&[
        "freeze",
        "--package",
        PACKAGE,
        "--type",
        "billing.invoice.issued",
        "--version",
        "3",
    ]));
    success(&fixture.cargo(&["build", "--release", "--locked", "--offline"]));
    let frozen_v3 = fixture.read(LEDGER);
    let exact = [
        "--package",
        PACKAGE,
        "--type",
        "billing.invoice.issued",
        "--version",
        "3",
    ];
    success(&fixture.cli(&[&["reset"][..], &exact].concat()));
    failure(
        &fixture.cargo(&["build", "--release", "--locked", "--offline"]),
        "reset draft",
    );
    failure(
        &fixture.cli(&["freeze", "--package", PACKAGE]),
        "reset reservations",
    );
    success(&fixture.cli(&[&["reset", "--undo"][..], &exact].concat()));
    assert_eq!(fixture.read(LEDGER), frozen_v3);
    success(&fixture.cli(&[&["reset"][..], &exact].concat()));
    success(&fixture.cli(&[&["freeze"][..], &exact].concat()));
    success(&fixture.cli(&["check", "--package", PACKAGE]));
    fixture.write(
        "src/lib.rs",
        &format!(
            "{current}{}{}",
            documented("example:nested-example.rs"),
            documented("example:enum-example.rs"),
        ),
    );
    success(&fixture.cargo(&["test", "--lib", "--locked", "--offline"]));
    success(&fixture.cli(&["freeze", "--package", PACKAGE]));

    fixture.write("import.json", &fixture.read(LEDGER));
    for file in ["Cargo.toml", "Cargo.lock", "build.rs", "src/lib.rs"] {
        fixture.write(&format!("imported/{file}"), &fixture.read(file));
    }
    let imported = fixture.root().join("imported");
    success(&fixture.cli_at(
        &imported,
        &["import", "--package", PACKAGE, "--from", "../import.json"],
    ));
    success(&fixture.cargo_at(&imported, &["build", "--release", "--locked", "--offline"]));
    success(&fixture.cli_at(&imported, &["check", "--package", PACKAGE]));
    assert_eq!(
        fixture.read("imported/type-history/schemas.json"),
        fixture.read(LEDGER)
    );
}

#[test]
fn renamed_dependency_tutorial_uses_the_documented_macro_import() {
    let fixture = Fixture::empty();
    success(&fixture.cli(&["init", "--package", PACKAGE]));
    fixture.write(
        "src/lib.rs",
        &format!(
            "{}{}",
            documented("reference:renamed-dependency.rs"),
            snippets::test_module(&[(
                "renamed_dependency_roundtrip",
                r#"
let delivery = Delivery { postal_code: "00100".to_owned() };
let bytes = serde_json::to_vec(&delivery.into_versioned())?;
let restored = Delivery::from_versioned(serde_json::from_slice(&bytes)?)?;
assert_eq!(restored.postal_code, "00100");
"#
                .to_owned(),
            )])
        ),
    );
    success(&fixture.cli(&["freeze", "--package", PACKAGE]));
    success(&fixture.cargo(&["test", "--lib", "--locked", "--offline"]));
}
