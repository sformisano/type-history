use std::{
    fmt::{Debug, Display, Formatter, Result as FmtResult},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use redactable::{BypassTextRedaction, RedactedValue, ToRedacted};
use serde_json::json;

use crate::{
    decode, DecodeError, DecodeFailureKind, HasHistory, History, PayloadVersion, ReadError,
    TypeIdentity,
};

struct SecretError(Arc<AtomicUsize>);

impl Debug for SecretError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        formatter.write_str("SECRET_RAW_DEBUG_SENTINEL")
    }
}

impl Display for SecretError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        formatter.write_str("SECRET_RAW_DISPLAY_SENTINEL")
    }
}

impl ToRedacted for SecretError {
    fn to_redacted(&self) -> RedactedValue {
        self.0.fetch_add(1, Ordering::SeqCst);
        BypassTextRedaction("rejected conversion".to_owned()).to_redacted()
    }
}

fn version(raw: u32) -> PayloadVersion {
    PayloadVersion::try_from_raw(raw).expect("positive version")
}

#[test]
fn author_error_is_captured_once_and_all_renderers_retain_safe_context() {
    let captures = Arc::new(AtomicUsize::new(0));
    let error = DecodeError::upcast(
        TypeIdentity::new("billing.invoice.issued"),
        version(1),
        version(2),
        version(3),
        &SecretError(Arc::clone(&captures)),
    );
    assert_eq!(error.kind(), DecodeFailureKind::Upcast);
    assert_eq!(error.source_version(), version(1));
    assert_eq!(error.adjacent_from(), Some(version(2)));
    assert_eq!(error.adjacent_to(), Some(version(3)));
    assert_eq!(error.identity().as_str(), "billing.invoice.issued");
    let json = serde_json::to_value(&error).expect("serialize safe failure");
    assert_eq!(
        json,
        json!({
            "identity": "billing.invoice.issued",
            "source_version": 1,
            "adjacent_from": 2,
            "adjacent_to": 3,
            "kind": "Upcast",
            "redacted_cause": {"message": "rejected conversion"},
        })
    );
    for rendered in [
        format!("{error}"),
        format!("{error:?}"),
        json.to_string(),
        error.to_redacted().text(),
        error.to_redacted().json().to_string(),
        error.redacted_cause().expect("captured cause").text(),
    ] {
        assert!(!rendered.contains("SECRET_RAW_"));
    }
    assert_eq!(captures.load(Ordering::SeqCst), 1);
}

#[test]
fn identity_mismatch_returns_before_invoking_the_decoder() {
    struct Current;

    impl HasHistory for Current {
        const TYPE: TypeIdentity = TypeIdentity::new("billing.invoice.issued");
        const VERSION: PayloadVersion = PayloadVersion::INITIAL;

        fn history() -> History<Self> {
            panic!("identity mismatch must precede descriptor and byte access")
        }
    }

    let wrong = TypeIdentity::new("billing.invoice.paid");
    let failure = match decode::<Current>(&wrong, version(1), b"SECRET_RAW_PAYLOAD_SENTINEL") {
        Err(failure) => failure,
        Ok(_) => panic!("wrong identity was accepted"),
    };
    assert_eq!(
        failure,
        ReadError::TypeMismatch {
            expected: Current::TYPE,
            found: wrong
        }
    );
    for rendered in [
        format!("{failure}"),
        format!("{failure:?}"),
        serde_json::to_string(&failure).expect("serialize identity failure"),
        failure.to_redacted().text(),
        failure.to_redacted().json().to_string(),
    ] {
        assert!(!rendered.contains("SECRET_RAW_PAYLOAD_SENTINEL"));
    }
}

#[test]
fn decode_and_unknown_versions_have_fixed_safe_causes() {
    let failure = DecodeError::decode(TypeIdentity::new("billing.invoice.issued"), version(4));
    assert_eq!(failure.kind(), DecodeFailureKind::Decode);
    assert_eq!(failure.source_version(), version(4));
    assert_eq!(failure.adjacent_from(), None);
    assert_eq!(failure.adjacent_to(), None);
    assert_eq!(
        failure.redacted_cause().expect("safe cause").text(),
        "history wire decoding failed"
    );

    let unknown = DecodeError::unsupported(TypeIdentity::new("billing.invoice.issued"), version(5));
    assert_eq!(unknown.kind(), DecodeFailureKind::UnsupportedVersion);
    assert_eq!(unknown.source_version(), version(5));
    assert!(unknown.redacted_cause().is_none());
}
