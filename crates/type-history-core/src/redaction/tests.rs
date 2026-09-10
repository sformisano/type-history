use super::{RedactedDecodeError, RedactedReadError};
use crate::{DecodeFailureKind, PayloadVersion, TypeIdentity};
use redactable::{BypassTextRedaction, RedactedValue, ToRedacted};
use std::{
    cell::Cell,
    error::Error,
    fmt::{Debug, Display, Formatter, Result as FmtResult},
    rc::Rc,
};

struct SecretError {
    projections: Rc<Cell<usize>>,
    drops: Rc<Cell<usize>>,
}
impl Debug for SecretError {
    fn fmt(&self, _: &mut Formatter<'_>) -> FmtResult {
        panic!("RAW_DEBUG_SECRET_726")
    }
}
impl Display for SecretError {
    fn fmt(&self, _: &mut Formatter<'_>) -> FmtResult {
        panic!("RAW_DISPLAY_SECRET_726")
    }
}
impl Error for SecretError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        panic!("RAW_SOURCE_SECRET_726")
    }
}
impl ToRedacted for SecretError {
    fn to_redacted(&self) -> RedactedValue {
        self.projections.set(self.projections.get() + 1);
        BypassTextRedaction("safe cause".to_owned()).to_redacted()
    }
}
impl Drop for SecretError {
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + 1);
    }
}

#[test]
fn callback_is_projected_once_and_dropped_before_safe_observation() {
    let projections = Rc::new(Cell::new(0));
    let drops = Rc::new(Cell::new(0));
    let error = RedactedDecodeError::upcast(
        TypeIdentity::new("test.secret.record"),
        PayloadVersion::INITIAL,
        PayloadVersion::try_from(2).unwrap(),
        PayloadVersion::try_from(3).unwrap(),
        SecretError {
            projections: projections.clone(),
            drops: drops.clone(),
        },
    );
    assert_eq!(projections.get(), 1);
    assert_eq!(drops.get(), 1);
    assert_eq!(error.kind(), DecodeFailureKind::Upcast);
    assert_eq!(error.redacted_cause().unwrap().text(), "safe cause");
    assert!(error.source().is_none());
    let read = RedactedReadError::Decode(error.clone());
    for output in [
        format!("{error}"),
        format!("{error:?}"),
        serde_json::to_string(&error).unwrap(),
        error.to_redacted().text(),
        error.to_redacted().json().to_string(),
        format!("{read}"),
        format!("{read:?}"),
        serde_json::to_string(&read).unwrap(),
        read.to_redacted().text(),
        read.to_redacted().json().to_string(),
    ] {
        assert!(output.contains("safe cause"));
        assert!(!output.contains("SECRET_726"));
    }
    assert!(read.source().unwrap().source().is_none());
    assert_eq!(projections.get(), 1);
    assert_eq!(drops.get(), 1);
}

#[test]
fn borrowed_callback_errors_need_no_static_or_error_contract() {
    struct Borrowed<'a>(&'a str);
    impl ToRedacted for Borrowed<'_> {
        fn to_redacted(&self) -> RedactedValue {
            let _ = self.0;
            BypassTextRedaction("borrowed safe cause".to_owned()).to_redacted()
        }
    }
    let secret = "BORROWED_SECRET_726".to_owned();
    let error = RedactedDecodeError::upcast(
        TypeIdentity::new("test.secret.record"),
        PayloadVersion::INITIAL,
        PayloadVersion::INITIAL,
        PayloadVersion::try_from(2).unwrap(),
        Borrowed(&secret),
    );
    drop(secret);
    assert_eq!(
        error.redacted_cause().unwrap().text(),
        "borrowed safe cause"
    );
}
