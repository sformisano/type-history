use super::support::{failure, success, Fixture, V1, V2, V3};
use toml::Value;

const PACKAGE: &str = "standalone-history-consumer";
const TESTS: &str = include_str!("fixtures/versioned-tests.rs");
const VALUES: &str = include_str!("fixtures/versioned-values.rs");
const VALIDATION: &str = include_str!("fixtures/versioned-validation.rs");

#[test]
fn versioned_values_delay_conversion_and_support_ordinary_consumers() {
    let fixture = Fixture::empty();
    let mut manifest: Value = toml::from_str(&fixture.read("Cargo.toml")).unwrap();
    manifest["dependencies"]
        .as_table_mut()
        .unwrap()
        .insert("rmp-serde".into(), "1.3.1".into());
    fixture.write("Cargo.toml", &toml::to_string(&manifest).unwrap());
    success(&fixture.cargo(&["generate-lockfile", "--offline"]));
    success(&fixture.cli(&["init", "--package", PACKAGE]));
    for source in [V1, V2] {
        fixture.write("src/lib.rs", source);
        success(&fixture.cli(&["freeze", "--package", PACKAGE]));
    }
    fixture.write(
        "src/lib.rs",
        &format!("{V3}\n{TESTS}\n{VALUES}\n{VALIDATION}"),
    );
    success(&fixture.cli(&["freeze", "--package", PACKAGE]));
    success(&fixture.cargo(&["test", "--lib", "--locked", "--offline"]));
    // An application imports only the current alias and the public container.
    fixture.write(
        "src/main.rs",
        r###"
use history_api::Versioned;
use standalone_history_consumer::Invoice;
fn main() {
    let stored: Versioned<Invoice> = serde_json::from_str(
        r##"{"stable_name":"billing.invoice.issued","version":1,"payload":{"legacy":"app","count":42}}"##,
    ).unwrap();
    let current = Invoice::from_versioned(stored).unwrap();
    assert_eq!(current.count, "42");
    assert_eq!(current.into_versioned().source_version().get(), 3);
}
"###,
    );
    success(&fixture.cargo(&["run", "--locked", "--offline"]));
    for (source, diagnostic) in [
        (
            r#"
use history_api::Versioned;
use standalone_history_consumer::{Invoice, InvoiceV1};
fn main() {
    let old = InvoiceV1 { legacy: String::new(), count: 1 };
    let _: Versioned<Invoice> = Versioned::new(old);
}
"#,
            "mismatched types",
        ),
        (
            r#"
use history_api::Versioned;
use standalone_history_consumer::InvoiceV1;
fn main() {
    let old = InvoiceV1 { legacy: String::new(), count: 1 };
    let _ = Versioned::new(old);
}
"#,
            "VersionedHistory",
        ),
        (
            r#"
use standalone_history_consumer::Invoice;
fn main() {
    let mut record = Invoice { count: "1".into(), label: "a".into(), revision: 7 }.into_versioned();
    record.historical = record.historical;
}
"#,
            "private",
        ),
    ] {
        fixture.write("src/main.rs", source);
        failure(
            &fixture.cargo(&["build", "--bin", PACKAGE, "--locked", "--offline"]),
            diagnostic,
        );
    }
}
