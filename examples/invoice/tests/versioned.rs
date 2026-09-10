use invoice_history::{ConvertError, Invoice};
use rmp_serde::{from_slice as from_msgpack, to_vec_named};
use serde_json::{from_slice, from_str, json, to_value, to_vec, Value};
use std::error::Error;
use typehistory::{DecodeFailureKind, Versioned};

const OLD: &[u8] = br#"{"billing.invoice.issued":{"1":{"legacy":"INV-42","count":42}}}"#;

#[test]
fn application_uses_only_the_current_alias_for_reading_and_writing() {
    let invoice = Invoice::from_versioned(from_slice(OLD).unwrap()).unwrap();
    assert_eq!(invoice.count, "42");
    assert_eq!(invoice.label, "INV-42");
    assert_eq!(invoice.revision, 7);

    let bytes = to_vec(&invoice.clone().into_versioned()).unwrap();
    let restored = Invoice::from_versioned(from_slice(&bytes).unwrap()).unwrap();
    assert_eq!(restored, invoice);
    assert_eq!(
        from_slice::<Value>(&bytes).unwrap(),
        json!({
            "billing.invoice.issued": {"3": {"count": "42", "label": "INV-42", "revision": 7}}
        })
    );
}

#[test]
fn untouched_record_retains_its_original_version_and_data() {
    let stored: Versioned<Invoice> = from_slice(OLD).unwrap();
    assert_eq!(stored.stable_id().as_str(), "billing.invoice.issued");
    assert_eq!(stored.source_version().get(), 1);
    assert_eq!(
        to_value(stored.clone()).unwrap(),
        from_slice::<Value>(OLD).unwrap()
    );
    assert!(!format!("{stored:?}").contains("INV-42"));
}

#[test]
fn binary_format_reads_old_records_and_round_trips_current_values() {
    let stored: Versioned<Invoice> = from_slice(OLD).unwrap();
    let bytes = to_vec_named(&stored).unwrap();
    let invoice = Invoice::from_versioned(from_msgpack(&bytes).unwrap()).unwrap();
    assert_eq!(invoice.label, "INV-42");
    let bytes = to_vec_named(&invoice.clone().into_versioned()).unwrap();
    assert_eq!(
        Invoice::from_versioned(from_msgpack(&bytes).unwrap()).unwrap(),
        invoice
    );
}

#[test]
fn conversion_failure_keeps_the_concrete_source_and_original_version() {
    let bytes = br#"{"billing.invoice.issued":{"1":{"legacy":"INV-13","count":13}}}"#;
    let stored = from_slice(bytes).unwrap();
    let error = Invoice::from_versioned(stored).unwrap_err();
    assert_eq!(error.kind(), DecodeFailureKind::Upcast);
    assert_eq!(error.source_version().get(), 1);
    assert_eq!(error.adjacent_from().unwrap().get(), 2);
    assert_eq!(error.adjacent_to().unwrap().get(), 3);
    assert_eq!(
        error
            .source()
            .unwrap()
            .downcast_ref::<ConvertError>()
            .unwrap()
            .0,
        13
    );
}

#[test]
fn malformed_envelopes_and_payloads_are_rejected() {
    for bytes in [
        "{}",
        r#"{"billing.invoice.issued":{}}"#,
        r#"{"another.invoice.issued":{"1":{"legacy":"INV-42","count":42}}}"#,
        r#"{"billing.invoice.issued":{"0":{}}}"#,
        r#"{"billing.invoice.issued":{"01":{}}}"#,
        r#"{"billing.invoice.issued":{"+1":{}}}"#,
        r#"{"billing.invoice.issued":{"4":{}}}"#,
        r#"{"billing.invoice.issued":{"4294967296":{}}}"#,
        r#"{"billing.invoice.issued":{"1":{"legacy":"INV-42","count":42},"1":{}}}"#,
        r#"{"billing.invoice.issued":{"1":{"legacy":"INV-42","count":42}},"billing.invoice.issued":{}}"#,
        r#"{"billing.invoice.issued":{"1":{"legacy":"INV-42","count":42,"count":43}}}"#,
        r#"{"billing.invoice.issued":{"1":{"legacy":"INV-42","count":42,"extra":0}}}"#,
        r#"{"billing.invoice.issued":{"1":{"legacy":"INV-42"}}}"#,
        r#"{"billing.invoice.issued":{"1":["INV-42",42]}}"#,
        r#"{"billing.invoice.issued":{"1":{"legacy":"INV-42","count":42}}} trailing"#,
    ] {
        assert!(
            from_str::<Versioned<Invoice>>(bytes).is_err(),
            "accepted {bytes}"
        );
    }
}

#[test]
fn identity_or_version_mismatch_precedes_payload_parsing() {
    let error =
        from_str::<Versioned<Invoice>>(r#"{"another.invoice.issued": INVALID"#).unwrap_err();
    assert!(error.to_string().contains("stable identity"));
    let error =
        from_str::<Versioned<Invoice>>(r#"{"billing.invoice.issued":{"9": INVALID"#).unwrap_err();
    assert!(error.to_string().contains("unsupported"));
}
