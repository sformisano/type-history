use super::support::{failure, success, Fixture, LEDGER};

const V2: &str = r#"
use history_api::versioned;
use std::convert::Infallible;
#[versioned(stable_name = "billing.invoice.issued")]
pub struct Invoice {
    #[history(removed_in = v2)] pub legacy: String,
    #[history(updated_in = v2, previous_type = u32, backfill_value = { return 99_u64; })]
    pub count: u64,
    #[history(added_in = v2, backfill_fn = merged)] pub merged: String,
    #[history(added_in = v2, backfill_fn = left)] pub left: String,
    #[history(added_in = v2, backfill_fn = right)] pub right: String,
    #[history(added_in = v2, backfill_value = 7_u16)] pub transient: u16,
}
fn merged(old: &InvoiceV1) -> Result<String, Infallible> { Ok(format!("{}:{}", old.legacy, old.count)) }
fn left(old: &InvoiceV1) -> Result<String, Infallible> { Ok(old.legacy[..1].to_owned()) }
fn right(old: &InvoiceV1) -> Result<String, Infallible> { Ok(old.legacy[1..].to_owned()) }
"#;

#[test]
fn backfills_cover_constant_updates_shared_reads_and_complete_field_lifetimes() {
    let fixture = Fixture::frozen();
    let frozen = fixture.read(LEDGER);
    for (before, after, diagnostic) in [
        ("return 99_u64", "return String::new()", "mismatched types"),
        ("return 99_u64", "Ok::<u64, Infallible>(99)?", "`?`"),
        ("old: &InvoiceV1", "old: InvoiceV1", "mismatched types"),
        ("old: &InvoiceV1", "old: &InvoiceV2", "no field"),
        (
            "Result<String, Infallible>",
            "Result<&'static str, Infallible>",
            "mismatched types",
        ),
    ] {
        assert!(V2.contains(before));
        fixture.write("src/lib.rs", &V2.replace(before, after));
        failure(
            &fixture.cargo(&["build", "--locked", "--offline"]),
            diagnostic,
        );
        assert_eq!(fixture.read(LEDGER), frozen);
    }
    fixture.write("src/lib.rs", V2);
    success(&fixture.cli(&["freeze", "--package", "standalone-history-consumer"]));
    let v3 = V2.replace(
        "#[history(added_in = v2, backfill_value = 7_u16)] pub transient: u16",
        "#[history(updated_in = v3, previous_type = u16, backfill_fn = grow)] #[history(added_in = v2, backfill_value = 7_u16)] pub transient: u64",
    );
    assert_ne!(v3, V2);
    let v3 = format!("{v3}\nfn grow(old: &InvoiceV2) -> Result<u64, Infallible> {{ Ok(u64::from(old.transient) + old.count) }}");
    fixture.write("src/lib.rs", &format!(r##"{v3}
#[test] fn old_data_reaches_v3() {{
    use history_api::{{decode, HasHistory, PayloadVersion}};
    let value = decode::<Invoice>(&Invoice::STABLE_NAME, PayloadVersion::INITIAL, br#"{{"legacy":"abc","count":4}}"#).unwrap();
    assert_eq!(value.count, 99);
    assert_eq!(value.merged, "abc:4");
    assert_eq!(value.left, "a");
    assert_eq!(value.right, "bc");
    assert_eq!(value.transient, 106);
}}
"##));
    success(&fixture.cargo(&["test", "--lib", "--locked", "--offline"]));
    success(&fixture.cli(&["freeze", "--package", "standalone-history-consumer"]));
    let v4 = v3.replace(
        "#[history(updated_in = v3",
        "#[history(removed_in = v4)] #[history(updated_in = v3",
    );
    assert_ne!(v4, v3);
    fixture.write("src/lib.rs", &format!(r##"{v4}
#[test] fn old_data_reaches_v4() {{
    use history_api::{{decode, HasHistory, PayloadVersion}};
    let value = decode::<Invoice>(&Invoice::STABLE_NAME, PayloadVersion::INITIAL, br#"{{"legacy":"abc","count":4}}"#).unwrap();
    let fields = serde_json::to_value(value).unwrap();
    assert!(fields.get("transient").is_none());
    assert_eq!(fields["merged"], "abc:4");
}}
"##));
    success(&fixture.cargo(&["test", "--lib", "--locked", "--offline"]));
}

#[test]
fn early_conversion_failure_stops_remaining_callbacks_and_steps() {
    use super::support::{V2 as ADJACENT_V2, V3};

    let fixture = Fixture::frozen();
    fixture.write("src/lib.rs", ADJACENT_V2);
    success(&fixture.cli(&["freeze", "--package", "standalone-history-consumer"]));

    let original = "Ok(u64::from(previous.count))";
    assert_eq!(V3.matches(original).count(), 1);
    let source = V3.replace(
        original,
        r#"if previous.count == 12 {
        Err(ConvertError(u64::from(previous.count)))
    } else {
        Ok(u64::from(previous.count))
    }"#,
    );
    let tests = include_str!("fixtures/backfill-failures.rs");
    fixture.write("src/lib.rs", &format!("{source}\n{tests}"));
    success(&fixture.cli(&["freeze", "--package", "standalone-history-consumer"]));
    success(&fixture.cargo(&["test", "--lib", "--locked", "--offline"]));
}
