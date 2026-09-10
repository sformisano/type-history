use super::support::{failure, success, Fixture, V1, V2, V3};

const PACKAGE: &str = "standalone-history-consumer";
const TESTS: &str = include_str!("fixtures/versioned-tests.rs");
#[cfg(feature = "redactable")]
const REDACTED_TESTS: &str = include_str!("fixtures/versioned-redacted-tests.rs");

#[test]
fn versioned_values_delay_conversion_and_support_ordinary_consumers() {
    let fixture = Fixture::empty();
    success(&fixture.cli(&["init", "--package", PACKAGE]));
    for source in [V1, V2] {
        fixture.write("src/lib.rs", source);
        success(&fixture.cli(&["freeze", "--package", PACKAGE]));
    }
    fixture.write("src/lib.rs", &format!("{V3}\n{TESTS}"));
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
        r##"{"billing.invoice.issued":{"1":{"legacy":"app","count":42}}}"##,
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

#[test]
#[cfg(feature = "redactable")]
fn versioned_redacted_values_keep_safe_observations_and_raw_serialization() {
    let fixture = Fixture::configured(true);
    success(&fixture.cli(&["init", "--package", PACKAGE]));
    fixture.write(
        "src/lib.rs",
        r#"
#[history_api::history(stable_id = "example.stored.secret", redacted)]
pub struct Secure { #[sensitive] pub value: String }
"#,
    );
    success(&fixture.cli(&["freeze", "--package", PACKAGE]));
    fixture.write("src/lib.rs", REDACTED_TESTS);
    success(&fixture.cli(&["freeze", "--package", PACKAGE]));
    success(&fixture.cargo(&["test", "--lib", "--locked", "--offline"]));
    success(&fixture.cargo(&["test", "--lib", "--release", "--locked", "--offline"]));
}
