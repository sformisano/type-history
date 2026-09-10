use history_api::{DecodeContext, DecodeFailureKind, PayloadVersion, TypeIdentity};
use serde::Serialize;
use serde_json::Error as JsonError;

// This frontend owns its tree and projection trait. Neither requires ToRedacted.
#[derive(Debug, Serialize)]
pub struct ErrorTree { variant: &'static str, safe_code: u32 }
pub trait ToRedactedError { fn to_redacted_error(&self) -> ErrorTree; }
pub struct CallbackError { variant: &'static str, secret: String }
impl ToRedactedError for CallbackError {
    fn to_redacted_error(&self) -> ErrorTree { ErrorTree { variant: self.variant, safe_code: self.secret.len() as u32 } }
}
#[derive(Debug, Serialize)]
pub struct FrontendError { context: DecodeContext, cause: Option<ErrorTree> }
impl FrontendError {
    pub fn unsupported(identity: TypeIdentity, version: PayloadVersion) -> Self {
        Self { context: DecodeContext::unsupported(identity, version), cause: None }
    }
    pub fn decode(identity: TypeIdentity, version: PayloadVersion, _: JsonError) -> Self {
        Self { context: DecodeContext::decode(identity, version), cause: Some(ErrorTree { variant: "Wire", safe_code: 0 }) }
    }
    pub fn upcast<E: ToRedactedError>(identity: TypeIdentity, version: PayloadVersion, from: PayloadVersion, to: PayloadVersion, error: E) -> Self {
        Self { context: DecodeContext::upcast(identity, version, from, to), cause: Some(error.to_redacted_error()) }
    }
}
include!(concat!(env!("OUT_DIR"), "/history.rs"));
fn convert(value: u32) -> Result<String, CallbackError> {
    if value == 1 { Err(CallbackError { variant: "Value", secret: "AXIS_RAW_SECRET_726".to_owned() }) }
    else { Ok(value.to_string()) }
}
fn inspect(previous: &AxisRecordV1) -> Result<bool, CallbackError> {
    if previous.value == 2 { Err(CallbackError { variant: "Payload", secret: "AXIS_RAW_SECRET_726".to_owned() }) }
    else { Ok(true) }
}
#[test]
fn concrete_frontend_errors_keep_their_structured_tree() {
    for (bytes, variant) in [(br#"{"value":1}"#.as_slice(), "Value"), (br#"{"value":2}"#, "Payload")] {
        let error = frontend_decode_history(PayloadVersion::INITIAL, bytes).unwrap_err();
        assert_eq!(error.context.kind(), DecodeFailureKind::Upcast);
        assert_eq!(error.context.source_version(), PayloadVersion::INITIAL);
        assert_eq!(error.context.adjacent_from(), Some(PayloadVersion::INITIAL));
        assert_eq!(error.context.adjacent_to().unwrap().get(), 2);
        assert_eq!(error.cause.as_ref().unwrap().variant, variant);
        assert_eq!(error.cause.as_ref().unwrap().safe_code, 19);
        assert!(!serde_json::to_string(&error).unwrap().contains("AXIS_RAW_SECRET_726"));
    }
    let valid = frontend_decode_history(PayloadVersion::INITIAL, br#"{"value":3}"#).unwrap();
    assert_eq!(valid.value, "3");
    let invalid = frontend_decode_history(PayloadVersion::INITIAL, br#"{"value":"AXIS_RAW_SECRET_726"}"#).unwrap_err();
    assert_eq!(invalid.context.kind(), DecodeFailureKind::Decode);
    assert_eq!(invalid.cause.unwrap().variant, "Wire");
}
