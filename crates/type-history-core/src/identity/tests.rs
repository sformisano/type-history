use std::{borrow::Cow, panic::catch_unwind};

use super::{InvalidTypeIdentity, PayloadVersion, TypeIdentity};

#[test]
fn payload_versions_reject_zero_during_parsing_and_deserialization() {
    assert_eq!(PayloadVersion::INITIAL.get(), 1);
    assert_eq!(PayloadVersion::try_from_raw(7).unwrap().get(), 7);
    assert_eq!(PayloadVersion::try_from_raw(0).unwrap_err().value, 0);
    assert!(serde_json::from_str::<PayloadVersion>("0").is_err());
    assert_eq!(
        serde_json::from_str::<PayloadVersion>("2").unwrap().get(),
        2
    );
}

#[test]
fn identities_preserve_static_and_owned_canonical_values() {
    const STATIC_TYPE: TypeIdentity = TypeIdentity::new("billing.invoice.paid");
    let owned = TypeIdentity::try_from("billing.invoice.paid".to_owned()).unwrap();
    assert_eq!(STATIC_TYPE, owned);
    assert!(matches!(STATIC_TYPE.0, Cow::Borrowed(_)));
    assert!(matches!(owned.0, Cow::Owned(_)));
    assert_eq!(
        serde_json::to_string(&owned).unwrap(),
        "\"billing.invoice.paid\""
    );
    assert_eq!(
        serde_json::from_str::<TypeIdentity>("\"billing.invoice.paid\"").unwrap(),
        STATIC_TYPE
    );
}

#[test]
fn identities_accept_simple_and_namespaced_values() {
    for value in [
        "a",
        "receipt",
        "receipt_created",
        "receipt_2",
        "shop.receipt",
        "shop.receipt.created",
        "shop_2.receipt_created",
    ] {
        let owned = TypeIdentity::try_from(value.to_owned()).expect(value);
        let declared = TypeIdentity::new(value);
        assert_eq!(owned, declared);
        assert_eq!(owned.as_str(), value);
        let json = serde_json::to_string(value).unwrap();
        assert_eq!(serde_json::to_string(&owned).unwrap(), json);
        assert_eq!(serde_json::from_str::<TypeIdentity>(&json).unwrap(), owned);
    }
}

#[test]
fn identities_reject_empty_segments_and_invalid_characters() {
    for (value, segment_number) in [
        ("", 1),
        (".receipt", 1),
        ("receipt.", 2),
        ("shop..receipt", 2),
        ("Receipt", 1),
        ("shop.Receipt", 2),
        ("receipt-created", 1),
        ("receipt created", 1),
        ("receipt/created", 1),
        ("réceipt", 1),
        ("receipt\n", 1),
        ("_receipt", 1),
        ("1receipt", 1),
    ] {
        assert_eq!(
            TypeIdentity::try_from(value.to_owned()),
            Err(InvalidTypeIdentity::InvalidSegment { segment_number }),
            "{value:?}"
        );
        let json = serde_json::to_string(value).unwrap();
        assert!(serde_json::from_str::<TypeIdentity>(&json).is_err());
        assert!(catch_unwind(|| TypeIdentity::new(value)).is_err());
    }
}

#[test]
fn identities_enforce_the_length_limit_with_or_without_dots() {
    for value in ["a".repeat(TypeIdentity::MAX_LEN), "a.".repeat(127) + "a"] {
        assert_eq!(value.len(), TypeIdentity::MAX_LEN);
        assert!(TypeIdentity::try_from(value.clone()).is_ok());
        let json = serde_json::to_string(&value).unwrap();
        assert!(serde_json::from_str::<TypeIdentity>(&json).is_ok());
        let too_long = value + "a";
        assert_eq!(
            TypeIdentity::try_from(too_long.clone()),
            Err(InvalidTypeIdentity::TooLong)
        );
        let json = serde_json::to_string(&too_long).unwrap();
        assert!(serde_json::from_str::<TypeIdentity>(&json).is_err());
    }
}
