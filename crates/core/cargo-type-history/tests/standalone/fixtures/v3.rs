use history_api::versioned;
use std::cell::RefCell;
use std::error::Error;
use std::fmt::{Display, Formatter, Result as FmtResult};

thread_local! { static TRACE: RefCell<Vec<&'static str>> = const { RefCell::new(Vec::new()) }; }

#[derive(Debug)]
pub struct ConvertError(pub u64);

impl Display for ConvertError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        write!(formatter, "count {} cannot be converted", self.0)
    }
}

impl Error for ConvertError {}

#[versioned(stable_name = "billing.invoice.issued")]
pub struct Invoice {
    #[history(removed_in = v2)]
    pub legacy: String,
    #[history(updated_in = v3, previous_type = u64, backfill_fn = stringify)]
    #[history(updated_in = v2, previous_type = u32, backfill_fn = widen)]
    pub count: String,
    #[history(added_in = v2, backfill_fn = label)]
    pub label: String,
    #[history(added_in = v2, backfill_value = 7_u32)]
    pub revision: u32,
}

fn widen(previous: &InvoiceV1) -> Result<u64, ConvertError> {
    TRACE.with(|trace| trace.borrow_mut().push("widen"));
    Ok(u64::from(previous.count))
}
fn label(previous: &InvoiceV1) -> Result<String, ConvertError> {
    TRACE.with(|trace| trace.borrow_mut().push("label"));
    Ok(previous.legacy.clone())
}
fn stringify(previous: &InvoiceV2) -> Result<String, ConvertError> {
    let value = previous.count;
    TRACE.with(|trace| trace.borrow_mut().push("stringify"));
    if value == 13 { Err(ConvertError(value)) } else { Ok(value.to_string()) }
}

#[cfg(test)]
mod tests {
    use super::{ConvertError, Invoice, InvoiceV3, TRACE};
    use history_api::{decode, DecodeError, DecodeFailureKind, HasHistory, PayloadVersion, ReadError, StableName};
    use serde_json::Error as JsonError;
    use std::error::Error;

    #[test]
    fn exact_decoding_runs_adjacent_steps_and_borrows_before_moves() {
        TRACE.with(|trace| trace.borrow_mut().clear());
        let value = decode::<Invoice>(&Invoice::STABLE_NAME, PayloadVersion::INITIAL, br#"{"legacy":"INV-42","count":42}"#).unwrap();
        assert_eq!((&*value.count, &*value.label, value.revision), ("42", "INV-42", 7));
        TRACE.with(|trace| assert_eq!(&*trace.borrow(), &["widen", "label", "stringify"]));
        let _: Invoice = InvoiceV3 { count: "4".into(), label: "current".into(), revision: 7 };
        assert_eq!(Invoice::VERSION.get(), 3);
        assert!(format!("{value:?}").contains("INV-42"));
    }

    #[test]
    fn failure_preserves_original_and_adjacent_versions_and_typed_cause() {
        let failure = decode::<Invoice>(&Invoice::STABLE_NAME, PayloadVersion::INITIAL, br#"{"legacy":"INV-13","count":13}"#).unwrap_err();
        let ReadError::Decode(error) = &failure else { panic!("expected adjacent failure") };
        assert_eq!(error.source_version().get(), 1);
        assert_eq!(error.adjacent_from().unwrap().get(), 2);
        assert_eq!(error.adjacent_to().unwrap().get(), 3);
        assert_eq!(error.kind(), DecodeFailureKind::Upcast);
        let cause = error.source().unwrap().downcast_ref::<ConvertError>().unwrap();
        assert_eq!(cause.0, 13);
        assert_eq!(cause.to_string(), "count 13 cannot be converted");
        let outer_source = failure.source().unwrap().downcast_ref::<DecodeError>().unwrap();
        assert_eq!(outer_source.context(), error.context());
    }

    #[test]
    fn strict_json_exact_version_and_stable_name_precede_conversion() {
        TRACE.with(|trace| trace.borrow_mut().clear());
        for bytes in [b"not JSON".as_slice(), br#"{"count":"42","label":"ok","revision":7}"#] {
            let failure = Invoice::history().decode(PayloadVersion::try_from_raw(99).unwrap(), bytes).unwrap_err();
            assert_eq!(failure.kind(), DecodeFailureKind::UnsupportedVersion);
            assert!(failure.source().is_none());
        }
        for bytes in [br#"["INV-42",42]"#.as_slice(), br#"{"legacy":"INV-42","count":42,"count":43}"#, br#"{"legacy":"INV-42","count":42,"extra":1}"#, br#"{"legacy":"INV-42","count":"wrong"}"#, b"not JSON"] {
            let failure = Invoice::history().decode(PayloadVersion::INITIAL, bytes).unwrap_err();
            assert_eq!(failure.kind(), DecodeFailureKind::Decode);
            assert!(failure.source().unwrap().is::<JsonError>());
        }
        let wrong = StableName::new("billing.other.record");
        assert!(matches!(decode::<Invoice>(&wrong, PayloadVersion::INITIAL, b"invalid"), Err(ReadError::TypeMismatch { .. })));
        TRACE.with(|trace| assert!(trace.borrow().is_empty()));
    }
}
