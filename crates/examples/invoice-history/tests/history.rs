use invoice_history::{ConvertError, Invoice};
use serde_json::{from_slice, json, to_vec, Value};
use std::error::Error;
use type_history::{decode, DecodeFailureKind, HasHistory, PayloadVersion, ReadError};

#[test]
fn old_bytes_reach_the_current_type() {
    let invoice = decode::<Invoice>(
        &Invoice::STABLE_NAME,
        PayloadVersion::INITIAL,
        br#"{"legacy":"INV-42","count":42}"#,
    )
    .unwrap();
    assert_eq!(invoice.count, "42");
    assert_eq!(invoice.label, "INV-42");
    assert_eq!(invoice.revision, 7);
    assert_eq!(Invoice::VERSION.get(), 3);
}

#[test]
fn current_payload_round_trips_without_historical_backfills() {
    let invoice = Invoice {
        count: "84".to_owned(),
        label: "INV-84".to_owned(),
        revision: 8,
    };
    let bytes = to_vec(&invoice).unwrap();
    let restored = decode::<Invoice>(&Invoice::STABLE_NAME, Invoice::VERSION, &bytes).unwrap();
    assert_eq!(restored, invoice);
    assert_eq!(
        from_slice::<Value>(&bytes).unwrap(),
        json!({"count": "84", "label": "INV-84", "revision": 8})
    );
}

#[test]
fn callback_failure_keeps_its_source_and_adjacent_versions() {
    let failure = decode::<Invoice>(
        &Invoice::STABLE_NAME,
        PayloadVersion::INITIAL,
        br#"{"legacy":"INV-13","count":13}"#,
    )
    .unwrap_err();
    let ReadError::Decode(error) = failure else {
        panic!("expected a conversion failure");
    };
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
fn unknown_historical_fields_are_rejected() {
    let failure = decode::<Invoice>(
        &Invoice::STABLE_NAME,
        PayloadVersion::INITIAL,
        br#"{"legacy":"INV-42","count":42,"unknown":true}"#,
    )
    .unwrap_err();
    let ReadError::Decode(error) = failure else {
        panic!("expected a wire decoding failure");
    };
    assert_eq!(error.kind(), DecodeFailureKind::Decode);
}
