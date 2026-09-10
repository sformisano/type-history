#[cfg(test)]
mod tests {
    use super::{ConvertError, Invoice, InvoiceV3};
    use std::error::Error;
    use typehistory::{decode, DecodeFailureKind, HasHistory, PayloadVersion, ReadError};

    #[test]
    fn old_bytes_reach_the_current_type() {
        let value = decode::<Invoice>(
            &Invoice::TYPE,
            PayloadVersion::INITIAL,
            br#"{"legacy":"INV-42","count":42}"#,
        )
        .unwrap();
        assert_eq!(value.count, "42");
        assert_eq!(value.label, "INV-42");
        assert_eq!(value.revision, 7);
        assert_eq!(Invoice::VERSION.get(), 3);
        let _: Invoice = InvoiceV3 {
            count: "42".to_owned(),
            label: "current".to_owned(),
            revision: 7,
        };
    }

    #[test]
    fn failure_keeps_the_original_version_and_failing_step() {
        let failure = decode::<Invoice>(
            &Invoice::TYPE,
            PayloadVersion::INITIAL,
            br#"{"legacy":"INV-13","count":13}"#,
        )
        .unwrap_err();
        let ReadError::Decode(error) = &failure else {
            panic!("expected a conversion failure");
        };
        assert_eq!(error.source_version().get(), 1);
        assert_eq!(error.adjacent_from().unwrap().get(), 2);
        assert_eq!(error.adjacent_to().unwrap().get(), 3);
        assert_eq!(error.kind(), DecodeFailureKind::Upcast);
        let cause = error
            .source()
            .unwrap()
            .downcast_ref::<ConvertError>()
            .unwrap();
        assert_eq!(cause.0, 13);
    }
}
