//! Compile the README's exact Rust blocks in ordinary external consumers.

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
    let fixture = Fixture::empty();
    let mut manifest: Value = toml::from_str(&fixture.read("Cargo.toml")).unwrap();
    let dependencies = manifest["dependencies"].as_table_mut().unwrap();
    let runtime = dependencies.remove("history_api").unwrap();
    dependencies.insert("typehistory".into(), runtime);
    fixture.write("Cargo.toml", &toml::to_string(&manifest).unwrap());
    success(&fixture.cli(&["init", "--package", PACKAGE]));
    fixture
}

#[test]
fn receipt_tutorial_reads_old_bytes_and_writes_current_payloads() {
    let fixture = fixture();
    fixture.write("src/lib.rs", documented("quick-example:v1.rs"));
    success(&fixture.cargo(&["build", "--locked", "--offline"]));
    success(&fixture.cli(&["freeze", "--package", PACKAGE]));
    fixture.write(
        "src/lib.rs",
        &format!(
            "{}{}",
            documented("quick-example:v2.rs"),
            documented("quick-example:runtime-tests.rs")
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
            &[("TYPEHISTORY_REQUIRE_FROZEN", Some("1"))],
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
            &[("TYPEHISTORY_REQUIRE_FROZEN", Some("0"))],
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
    fixture.write("src/lib.rs", v3);
    success(&fixture.cargo(&["build", "--locked", "--offline"]));
    fixture.write("src/lib.rs", v2);
    success(&fixture.cargo(&["build", "--release", "--locked", "--offline"]));
    assert_eq!(fixture.read(LEDGER), frozen_v2);

    let current = format!("{v3}{}", documented("example:runtime-tests.rs"));
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
        "typehistory.toml",
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
        fixture.read("imported/typehistory/schemas.json"),
        fixture.read(LEDGER)
    );
}
