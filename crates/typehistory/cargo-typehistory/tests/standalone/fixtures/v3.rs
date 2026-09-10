use history_api::history;
use redactable::{BypassTextRedaction, RedactedValue, ToRedacted};
use std::cell::RefCell;
use std::fmt::{Debug, Display, Formatter, Result as FmtResult};

thread_local! { static TRACE: RefCell<Vec<&'static str>> = const { RefCell::new(Vec::new()) }; }
pub struct ConvertError;
impl Debug for ConvertError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult { formatter.write_str("RAW_AUTHOR_ERROR_SECRET") }
}
impl Display for ConvertError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult { formatter.write_str("RAW_AUTHOR_ERROR_SECRET") }
}
impl ToRedacted for ConvertError {
    fn to_redacted(&self) -> RedactedValue { BypassTextRedaction("safe conversion failure".to_owned()).to_redacted() }
}

#[history(id = "billing.invoice.issued")]
pub struct Invoice {
    #[sensitive(redactable::Secret)]
    #[history(removed_at = v2)]
    pub legacy: String,
    #[not_sensitive]
    #[history(updated_at = v3, previous_type = u64, cast_from_previous_value = stringify)]
    #[history(updated_at = v2, previous_type = u32, cast_from_previous_value = widen)]
    pub count: String,
    #[sensitive(redactable::Secret)]
    #[history(added_at = v2, cast_from_previous_payload = label)]
    pub label: String,
    #[not_sensitive]
    #[history(added_at = v2, backfill_value = 7_u32)]
    pub revision: u32,
}

fn widen(value: u32) -> Result<u64, ConvertError> {
    TRACE.with(|trace| trace.borrow_mut().push("widen"));
    Ok(u64::from(value))
}
fn label(previous: &InvoiceV1) -> Result<String, ConvertError> {
    TRACE.with(|trace| trace.borrow_mut().push("label"));
    Ok(previous.legacy.clone())
}
fn stringify(value: u64) -> Result<String, ConvertError> {
    TRACE.with(|trace| trace.borrow_mut().push("stringify"));
    if value == 13 { Err(ConvertError) } else { Ok(value.to_string()) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use history_api::{decode, DecodeFailureKind, HasHistory, PayloadVersion, ReadError, TypeIdentity};
    use std::error::Error;

    #[test]
    fn exact_decoding_runs_adjacent_steps_and_borrows_before_moves() {
        TRACE.with(|trace| trace.borrow_mut().clear());
        let value = decode::<Invoice>(&Invoice::TYPE, PayloadVersion::INITIAL, br#"{"legacy":"PAYLOAD_SENTINEL_SECRET","count":42}"#).unwrap();
        assert_eq!((&*value.count, &*value.label, value.revision), ("42", "PAYLOAD_SENTINEL_SECRET", 7));
        TRACE.with(|trace| assert_eq!(&*trace.borrow(), &["label", "widen", "stringify"]));
        let _: Invoice = InvoiceV3 { count: "4".into(), label: "current".into(), revision: 7 };
        assert_eq!(Invoice::VERSION.get(), 3);
        assert!(!format!("{value:?}").contains("PAYLOAD_SENTINEL_SECRET"));
        assert!(!value.to_redacted().text().contains("PAYLOAD_SENTINEL_SECRET"));
    }

    #[test]
    fn safe_failure_preserves_original_and_adjacent_versions() {
        let failure = decode::<Invoice>(&Invoice::TYPE, PayloadVersion::INITIAL, br#"{"legacy":"PAYLOAD_SENTINEL_SECRET","count":13}"#).unwrap_err();
        let ReadError::Decode(error) = &failure else { panic!("expected adjacent failure") };
        assert_eq!(error.source_version().get(), 1);
        assert_eq!(error.adjacent_from().unwrap().get(), 2);
        assert_eq!(error.adjacent_to().unwrap().get(), 3);
        assert_eq!(error.kind(), DecodeFailureKind::Upcast);
        let renderings = [format!("{failure:?}"), failure.to_string(), serde_json::to_string(&failure).unwrap(), failure.to_redacted().text(), format!("{error:?}"), error.to_string(), serde_json::to_string(error).unwrap(), error.to_redacted().text(), failure.source().unwrap().to_string()];
        for rendered in renderings {
            assert!(rendered.contains("safe conversion failure"), "{rendered}");
            assert!(!rendered.contains("RAW_AUTHOR_ERROR_SECRET"), "{rendered}");
            assert!(!rendered.contains("PAYLOAD_SENTINEL_SECRET"), "{rendered}");
        }
        assert!(error.source().is_none());
    }

    #[test]
    fn strict_json_exact_version_and_identity_precede_conversion() {
        TRACE.with(|trace| trace.borrow_mut().clear());
        for bytes in [b"PAYLOAD_SENTINEL_SECRET".as_slice(), br#"{"count":"42","label":"ok","revision":7}"#] {
            let failure = Invoice::history().decode(PayloadVersion::try_from_raw(99).unwrap(), bytes).unwrap_err();
            assert_eq!(failure.kind(), DecodeFailureKind::UnsupportedVersion);
        }
        for bytes in [br#"["secret",42]"#.as_slice(), br#"{"legacy":"secret","count":42,"count":43}"#, br#"{"legacy":"secret","count":42,"extra":1}"#, br#"{"legacy":"PAYLOAD_SENTINEL_SECRET","count":"wrong"}"#, b"not JSON"] {
            let failure = Invoice::history().decode(PayloadVersion::INITIAL, bytes).unwrap_err();
            assert_eq!(failure.kind(), DecodeFailureKind::Decode);
            for rendered in [format!("{failure:?}"), failure.to_string(), serde_json::to_string(&failure).unwrap(), failure.to_redacted().text()] { assert!(!rendered.contains("PAYLOAD_SENTINEL_SECRET")); }
        }
        let wrong = TypeIdentity::new("billing.other.record");
        assert!(matches!(decode::<Invoice>(&wrong, PayloadVersion::INITIAL, b"invalid"), Err(ReadError::TypeMismatch { .. })));
        TRACE.with(|trace| assert!(trace.borrow().is_empty()));
    }
}
