//! Compile the README's exact Rust blocks in ordinary external consumers.

#[path = "tutorial/snippets.rs"]
mod snippets;

use super::support::{failure, success, Fixture, LEDGER};
use toml::Value;

const README: &str = include_str!("../../../../README.md");
const PACKAGE: &str = "standalone-history-consumer";

fn documented(marker: &str) -> &str {
    let marker = format!("<!-- {marker} -->\n```rust\n");
    assert_eq!(README.matches(&marker).count(), 1, "unique source marker");
    README
        .split_once(&marker)
        .expect("documented source")
        .1
        .split_once("```\n")
        .expect("closed Rust block")
        .0
}

fn fixture() -> Fixture {
    configured_fixture(false)
}

fn configured_fixture(redacted: bool) -> Fixture {
    let fixture = Fixture::configured(redacted);
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
    let v2 = documented("quick-example:v2.rs");
    let currency_history = "#[history(added_at = v2, backfill_value = \"USD\".to_owned())]";
    assert_eq!(v2.matches(currency_history).count(), 1);
    fixture.write("src/lib.rs", &v2.replacen(currency_history, "", 1));
    failure(
        &fixture.cargo(&["build", "--locked", "--offline"]),
        "field `currency` did not exist in frozen history",
    );
    assert_eq!(fixture.read(LEDGER), frozen_v1);
    fixture.write(
        "src/lib.rs",
        &format!(
            "{}{}{}",
            documented("quick-example:v2.rs"),
            documented("quick-example:runtime-tests.rs"),
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
    success(&fixture.cargo(&["build", "--release", "--locked", "--offline"]));
    success(&fixture.cli(&["check", "--package", PACKAGE]));
    fixture.write("src/lib.rs", &snippets::receipt_v3());
    success(&fixture.cargo(&["test", "--lib", "--locked", "--offline"]));
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
        (", backfill_value = 7_u32", "", "an added field requires"),
        ("previous_type = u32", "from = u32", "unknown history key"),
        (
            ", cast_from_previous_value = widen",
            "",
            "an updated field requires",
        ),
        (
            "backfill_value = 7_u32",
            "backfill_value = 7_u32, cast_from_previous_payload = label",
            "exactly one assignment rule",
        ),
        (
            "fn widen(value: u32)",
            "fn widen(value: u16)",
            "mismatched types",
        ),
        (
            "previous: &InvoiceV1",
            "previous: &InvoiceV2",
            "mismatched types",
        ),
        (
            "updated_at = v2",
            "updated_at = v0",
            "history versions are positive",
        ),
        (
            "updated_at = v2",
            "updated_at = v4294967295",
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
        .replace("Ok(u64::from(value))", "Ok(value.to_string())");
    assert!(v2.contains("Result<u64, ConvertError>") && v2.contains("Ok(u64::from(value))"));
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
        &format!("{current}{}", documented("example:nested-example.rs")),
    );
    success(&fixture.cargo(&["test", "--lib", "--locked", "--offline"]));
    success(&fixture.cli(&["freeze", "--package", PACKAGE]));

    fixture.write("import.json", &fixture.read(LEDGER));
    for file in [
        "Cargo.toml",
        "Cargo.lock",
        "type-history.toml",
        "build.rs",
        "src/lib.rs",
    ] {
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

#[test]
#[cfg(feature = "redactable")]
fn redacted_tutorial_runs_the_documented_safe_output() {
    let fixture = configured_fixture(true);
    fixture.write(
        "src/lib.rs",
        &format!(
            "{}{}",
            documented("reference:redacted-history.rs"),
            snippets::test_module(&[(
                "redacted_output",
                documented("reference:redacted-output.rs").to_owned(),
            )])
        ),
    );
    success(&fixture.cli(&["freeze", "--package", PACKAGE]));
    success(&fixture.cargo(&["test", "--lib", "--locked", "--offline"]));
}
