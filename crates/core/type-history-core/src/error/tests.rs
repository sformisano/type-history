use std::{
    convert::Infallible,
    error::Error,
    fmt::{Display, Formatter, Result as FmtResult},
    io::{Error as IoError, ErrorKind},
    ptr,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use serde_json::{to_value, Error as JsonError};

use crate::{
    decode, DecodeError, DecodeFailureKind, HasHistory, History, PayloadVersion, ReadError,
    StableName,
};

#[derive(Debug)]
struct ConversionError {
    rejected_value: u32,
    source: IoError,
    drops: Arc<AtomicUsize>,
}

impl Display for ConversionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        write!(formatter, "cannot convert value {}", self.rejected_value)
    }
}

impl Error for ConversionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.source)
    }
}

impl Drop for ConversionError {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::SeqCst);
    }
}

fn version(raw: u32) -> PayloadVersion {
    PayloadVersion::try_from_raw(raw).expect("positive version")
}

#[test]
fn upcast_owns_the_original_error_and_preserves_each_source() {
    let drops = Arc::new(AtomicUsize::new(0));
    let error = DecodeError::upcast(
        StableName::new("billing.invoice.issued"),
        version(1),
        version(2),
        version(3),
        ConversionError {
            rejected_value: 13,
            source: IoError::new(ErrorKind::InvalidData, "invalid conversion input"),
            drops: Arc::clone(&drops),
        },
    );
    assert_eq!(error.kind(), DecodeFailureKind::Upcast);
    assert_eq!(error.source_version(), version(1));
    assert_eq!(error.adjacent_from(), Some(version(2)));
    assert_eq!(error.adjacent_to(), Some(version(3)));
    assert_eq!(error.stable_name().as_str(), "billing.invoice.issued");
    assert_eq!(error.context().stable_name(), error.stable_name());
    assert_eq!(
        to_value(error.context()).unwrap()["stable_name"],
        "billing.invoice.issued"
    );
    assert_eq!(drops.load(Ordering::SeqCst), 0);

    let failure = ReadError::Decode(error);
    let wrapped = failure
        .source()
        .and_then(|source| source.downcast_ref::<DecodeError>())
        .expect("read failure retains its decode error");
    let original = wrapped
        .source()
        .and_then(|source| source.downcast_ref::<ConversionError>())
        .expect("conversion error retains its concrete type");
    assert_eq!(original.rejected_value, 13);
    assert_eq!(original.to_string(), "cannot convert value 13");
    let repeated = wrapped
        .source()
        .and_then(|source| source.downcast_ref::<ConversionError>())
        .expect("repeated source access");
    assert!(ptr::eq(original, repeated));
    let nested = original
        .source()
        .and_then(|source| source.downcast_ref::<IoError>())
        .expect("application error retains its own source");
    assert_eq!(nested.kind(), ErrorKind::InvalidData);
    assert_eq!(nested.to_string(), "invalid conversion input");

    drop(failure);
    assert_eq!(drops.load(Ordering::SeqCst), 1);
}

#[test]
fn stable_name_mismatch_returns_before_invoking_the_decoder() {
    struct Current;

    impl HasHistory for Current {
        const STABLE_NAME: StableName = StableName::new("billing.invoice.issued");
        const VERSION: PayloadVersion = PayloadVersion::INITIAL;

        fn history() -> History<Self> {
            panic!("stable name mismatch must precede descriptor and byte access")
        }
    }

    let wrong = StableName::new("billing.invoice.paid");
    let failure = match decode::<Current>(&wrong, version(1), b"not JSON") {
        Err(failure) => failure,
        Ok(_) => panic!("wrong stable name was accepted"),
    };
    let ReadError::TypeMismatch { expected, found } = &failure else {
        panic!("expected stable name mismatch");
    };
    assert_eq!(expected, &Current::STABLE_NAME);
    assert_eq!(found, &wrong);
    assert!(failure.source().is_none());
    assert!(failure.to_string().contains(expected.as_str()));
    assert!(failure.to_string().contains(found.as_str()));
}

#[test]
fn wire_failure_preserves_the_decoder_error_and_location() {
    let original = serde_json::from_slice::<Vec<u32>>(b"[\n  1,\n  invalid\n]")
        .expect_err("invalid JSON value");
    let expected = (
        original.line(),
        original.column(),
        original.classify(),
        original.to_string(),
    );
    let failure = DecodeError::decode(
        StableName::new("billing.invoice.issued"),
        version(4),
        original,
    );
    assert_eq!(failure.kind(), DecodeFailureKind::Decode);
    assert_eq!(failure.source_version(), version(4));
    assert_eq!(failure.stable_name().as_str(), "billing.invoice.issued");
    assert_eq!(failure.adjacent_from(), None);
    assert_eq!(failure.adjacent_to(), None);
    let source = failure
        .source()
        .and_then(|source| source.downcast_ref::<JsonError>())
        .expect("original JSON error");
    assert_eq!(
        (
            source.line(),
            source.column(),
            source.classify(),
            source.to_string(),
        ),
        expected,
    );
}

#[test]
fn unsupported_version_has_context_without_a_source() {
    let failure = DecodeError::unsupported(StableName::new("billing.invoice.issued"), version(5));
    assert_eq!(failure.kind(), DecodeFailureKind::UnsupportedVersion);
    assert_eq!(failure.source_version(), version(5));
    assert_eq!(failure.stable_name().as_str(), "billing.invoice.issued");
    assert_eq!(failure.adjacent_from(), None);
    assert_eq!(failure.adjacent_to(), None);
    assert!(failure.source().is_none());
}

#[test]
fn error_wrappers_support_thread_transfer_and_infallible_callbacks() {
    fn assert_error<T: Error + Send + Sync + 'static>() {}

    assert_error::<DecodeError>();
    assert_error::<ReadError>();
    let _: fn(
        StableName,
        PayloadVersion,
        PayloadVersion,
        PayloadVersion,
        Infallible,
    ) -> DecodeError = DecodeError::upcast::<Infallible>;
}
