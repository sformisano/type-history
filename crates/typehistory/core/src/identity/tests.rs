use std::borrow::Cow;

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
fn identities_reject_short_and_invalid_segments() {
    assert_eq!(
        TypeIdentity::try_from("account.created".to_owned()),
        Err(InvalidTypeIdentity::InvalidSegmentCount)
    );
    assert_eq!(
        TypeIdentity::try_from("Reference.account.created".to_owned()),
        Err(InvalidTypeIdentity::InvalidSegment { segment_number: 1 })
    );
    assert!(serde_json::from_str::<TypeIdentity>("\"billing.invoice\"").is_err());
}
