use invoice_history::payment::{Currency, Payment, PaymentCurrencyError, PaymentStatus};
use rmp_serde::{from_slice as from_msgpack, to_vec_named};
use serde_json::{from_slice, from_str, to_value, Value};
use std::{error::Error, str::from_utf8};
use type_history::{decode, DecodeFailureKind, HasHistory, PayloadVersion, Versioned};

const V1_JSON: &[u8] = include_bytes!("fixtures/payment-v1.json");
const V2_JSON: &[u8] = include_bytes!("fixtures/payment-v2.json");
const V1_MSGPACK: &[u8] = include_bytes!("fixtures/payment-v1.msgpack.hex");
const V2_MSGPACK: &[u8] = include_bytes!("fixtures/payment-v2.msgpack.hex");

fn fixture_bytes(hex: &[u8]) -> Vec<u8> {
    from_utf8(hex)
        .unwrap()
        .split_ascii_whitespace()
        .map(|byte| u8::from_str_radix(byte, 16).unwrap())
        .collect()
}

fn expected_payment() -> Payment {
    Payment {
        reference: "PAY-42".to_owned(),
        status: PaymentStatus::Paid {
            amount_minor: 4200,
            currency: Currency::Usd,
        },
    }
}

#[test]
fn historical_payment_preserves_amount_and_currency() {
    let json: Versioned<Payment> = from_slice(V1_JSON).unwrap();
    let binary: Versioned<Payment> = from_msgpack(&fixture_bytes(V1_MSGPACK)).unwrap();
    for stored in [json, binary] {
        assert_eq!(stored.source_version().get(), 1);
        assert_eq!(Payment::from_versioned(stored).unwrap(), expected_payment());
    }
}

#[test]
fn historical_enum_keeps_its_original_wire_value_until_conversion() {
    let stored: Versioned<Payment> = from_slice(V1_JSON).unwrap();
    assert_eq!(to_vec_named(&stored).unwrap(), fixture_bytes(V1_MSGPACK));
    assert_eq!(
        to_value(stored).unwrap(),
        from_slice::<Value>(V1_JSON).unwrap()
    );
}

#[test]
fn current_payment_matches_fixed_json_and_messagepack() {
    let current = expected_payment().into_versioned();
    assert_eq!(
        to_value(&current).unwrap(),
        from_slice::<Value>(V2_JSON).unwrap()
    );
    assert_eq!(to_vec_named(&current).unwrap(), fixture_bytes(V2_MSGPACK));
    for stored in [
        from_slice::<Versioned<Payment>>(V2_JSON).unwrap(),
        from_msgpack::<Versioned<Payment>>(&fixture_bytes(V2_MSGPACK)).unwrap(),
    ] {
        assert_eq!(stored.source_version().get(), 2);
        assert_eq!(Payment::from_versioned(stored).unwrap(), expected_payment());
    }
}

#[test]
fn unit_variant_survives_the_field_update() {
    let stored = from_str(
        r#"{"stable_name":"billing.payment.recorded","version":1,"payload":{"reference":"PAY-43","currency_code":"USD","status":"Pending"}}"#,
    ).unwrap();
    let payment = Payment::from_versioned(stored).unwrap();
    assert_eq!(payment.reference, "PAY-43");
    assert_eq!(payment.status, PaymentStatus::Pending);
}

#[test]
fn raw_json_api_applies_the_same_enum_conversion() {
    let payment = decode::<Payment>(
        &Payment::STABLE_NAME,
        PayloadVersion::INITIAL,
        br#"{"reference":"PAY-42","currency_code":"USD","status":{"Paid":4200}}"#,
    )
    .unwrap();
    assert_eq!(payment, expected_payment());
}

#[test]
fn unknown_stored_currency_returns_the_original_conversion_error() {
    let stored = from_str(
        r#"{"stable_name":"billing.payment.recorded","version":1,"payload":{"reference":"PAY-42","currency_code":"GBP","status":{"Paid":4200}}}"#,
    ).unwrap();
    let error = Payment::from_versioned(stored).unwrap_err();
    assert_eq!(error.kind(), DecodeFailureKind::Upcast);
    assert_eq!(error.source_version().get(), 1);
    assert_eq!(error.adjacent_from().unwrap().get(), 1);
    assert_eq!(error.adjacent_to().unwrap().get(), 2);
    assert_eq!(
        error
            .source()
            .unwrap()
            .downcast_ref::<PaymentCurrencyError>()
            .unwrap()
            .0,
        "GBP"
    );
}
