use invoice_history::{ConvertError, Invoice};
use rmp_serde::{from_slice as from_msgpack, to_vec_named};
use serde_json::{from_slice, from_str, json, to_value, to_vec, Value};
use std::{error::Error, str::from_utf8};
use type_history::{DecodeFailureKind, Versioned};

const OLD: &[u8] = br#"{"stable_name":"billing.invoice.issued","version":1,"payload":{"legacy":"INV-42","count":42}}"#;
const V1_MSGPACK_HEX: &[u8] = include_bytes!("fixtures/invoice-v1.msgpack.hex");
const V3_MSGPACK_HEX: &[u8] = include_bytes!("fixtures/invoice-v3.msgpack.hex");

fn fixture_bytes(fixture: &[u8]) -> Vec<u8> {
    from_utf8(fixture)
        .unwrap()
        .split_ascii_whitespace()
        .map(|octet| u8::from_str_radix(octet, 16).unwrap())
        .collect()
}

fn expected_invoice() -> Invoice {
    Invoice {
        count: "42".to_owned(),
        label: "INV-42".to_owned(),
        revision: 7,
    }
}

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
        json!({"stable_name":"billing.invoice.issued","version":3,"payload":{"count":"42","label":"INV-42","revision":7}})
    );
}

#[test]
fn untouched_record_retains_its_original_version_and_data() {
    let stored: Versioned<Invoice> = from_slice(OLD).unwrap();
    assert_eq!(stored.stable_name().as_str(), "billing.invoice.issued");
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
fn fixed_messagepack_fixtures_decode_expected_versions_and_values() {
    let stored: Versioned<Invoice> = from_msgpack(&fixture_bytes(V1_MSGPACK_HEX)).unwrap();
    assert_eq!(stored.stable_name().as_str(), "billing.invoice.issued");
    assert_eq!(stored.source_version().get(), 1);
    assert_eq!(Invoice::from_versioned(stored).unwrap(), expected_invoice());

    let stored: Versioned<Invoice> = from_msgpack(&fixture_bytes(V3_MSGPACK_HEX)).unwrap();
    assert_eq!(stored.stable_name().as_str(), "billing.invoice.issued");
    assert_eq!(stored.source_version().get(), 3);
    assert_eq!(Invoice::from_versioned(stored).unwrap(), expected_invoice());
}

#[test]
fn messagepack_encoding_matches_exact_historical_and_current_fixtures() {
    let historical: Versioned<Invoice> = from_slice(OLD).unwrap();
    assert_eq!(
        to_vec_named(&historical).unwrap(),
        fixture_bytes(V1_MSGPACK_HEX)
    );
    assert_eq!(
        to_vec_named(&expected_invoice().into_versioned()).unwrap(),
        fixture_bytes(V3_MSGPACK_HEX)
    );
}

#[test]
fn conversion_failure_keeps_the_concrete_source_and_original_version() {
    let bytes = br#"{"stable_name":"billing.invoice.issued","version":1,"payload":{"legacy":"INV-13","count":13}}"#;
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
        r#"{"stable_name":"billing.invoice.issued"}"#,
        r#"{"stable_name":"another.invoice.issued","version":1,"payload":{"legacy":"INV-42","count":42}}"#,
        r#"{"stable_name":"billing.invoice.issued","version":0,"payload":{}}"#,
        r#"{"stable_name":"billing.invoice.issued","version":"1","payload":{}}"#,
        r#"{"stable_name":"billing.invoice.issued","version":-1,"payload":{}}"#,
        r#"{"stable_name":"billing.invoice.issued","version":1.5,"payload":{}}"#,
        r#"{"stable_name":"billing.invoice.issued","version":null,"payload":{}}"#,
        r#"{"stable_name":"billing.invoice.issued","version":true,"payload":{}}"#,
        r#"{"version":1,"payload":{"legacy":"INV-42","count":42}}"#,
        r#"{"stable_name":"billing.invoice.issued","payload":{"legacy":"INV-42","count":42}}"#,
        r#"{"stable_name":"billing.invoice.issued","version":1}"#,
        r#"{"stable_name":"billing.invoice.issued","version":4,"payload":{}}"#,
        r#"{"stable_name":"billing.invoice.issued","version":4294967296,"payload":{}}"#,
        r#"{"stable_name":"billing.invoice.issued","version":1,"payload":{"legacy":"INV-42","count":42},"version":1,"payload":{}}"#,
        r#"{"stable_name":"billing.invoice.issued","version":1,"payload":{"legacy":"INV-42","count":42},"stable_name":"billing.invoice.issued"}"#,
        r#"{"stable_name":"billing.invoice.issued","version":1,"payload":{"legacy":"INV-42","count":42},"payload":{}}"#,
        r#"{"stable_name":"billing.invoice.issued","version":1,"payload":{"legacy":"INV-42","count":42},"extra":true}"#,
        r#"{"stable_name":"billing.invoice.issued","version":1,"payload":{"legacy":"INV-42","count":42,"count":43}}"#,
        r#"{"stable_name":"billing.invoice.issued","version":1,"payload":{"legacy":"INV-42","count":42,"extra":0}}"#,
        r#"{"stable_name":"billing.invoice.issued","version":1,"payload":{"legacy":"INV-42"}}"#,
        r#"{"stable_name":"billing.invoice.issued","version":1,"payload":["INV-42",42]}"#,
        r#"{"stable_name":"billing.invoice.issued","version":1,"payload":{"legacy":"INV-42","count":42}} trailing"#,
    ] {
        assert!(
            from_str::<Versioned<Invoice>>(bytes).is_err(),
            "accepted {bytes}"
        );
    }
}

#[test]
fn stable_name_or_version_mismatch_rejects_the_value() {
    let error = from_str::<Versioned<Invoice>>(
        r#"{"payload":{},"version":1,"stable_name":"another.invoice.issued"}"#,
    )
    .unwrap_err();
    assert!(error.to_string().contains("stable name"));
    let error = from_str::<Versioned<Invoice>>(
        r#"{"payload":{},"version":9,"stable_name":"billing.invoice.issued"}"#,
    )
    .unwrap_err();
    assert!(error.to_string().contains("unsupported"));
}

#[test]
fn wrapper_fields_can_appear_in_any_order() {
    let fields = [
        r#""stable_name":"billing.invoice.issued""#,
        r#""version":1"#,
        r#""payload":{"legacy":"INV-42","count":42}"#,
    ];
    for order in [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ] {
        let wire = format!("{{{}}}", order.map(|index| fields[index]).join(","));
        let stored: Versioned<Invoice> = from_str(&wire).unwrap();
        assert_eq!(stored.source_version().get(), 1);
        let invoice = Invoice::from_versioned(stored).unwrap();
        assert_eq!(invoice.label, "INV-42");
        assert_eq!(invoice.count, "42");
    }
}
