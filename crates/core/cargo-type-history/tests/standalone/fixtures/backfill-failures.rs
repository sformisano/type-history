#[cfg(test)]
mod early_failure_tests {
    use super::{ConvertError, Invoice, TRACE};
    use history_api::{
        decode, DecodeError, DecodeFailureKind, HasHistory, PayloadVersion, ReadError, Versioned,
    };
    use std::error::Error;

    fn assert_early_failure(error: &DecodeError) {
        assert_eq!(error.stable_name().as_str(), "billing.invoice.issued");
        assert_eq!(error.kind(), DecodeFailureKind::Upcast);
        assert_eq!(error.source_version().get(), 1);
        assert_eq!(error.adjacent_from().unwrap().get(), 1);
        assert_eq!(error.adjacent_to().unwrap().get(), 2);
        let cause = error
            .source()
            .unwrap()
            .downcast_ref::<ConvertError>()
            .unwrap();
        assert_eq!(cause.0, 12);
        assert_eq!(cause.to_string(), "count 12 cannot be converted");
        TRACE.with(|trace| assert_eq!(&*trace.borrow(), &["widen"]));
    }

    #[test]
    fn raw_decode_stops_before_later_fields_and_versions() {
        TRACE.with(|trace| trace.borrow_mut().clear());
        let failure = decode::<Invoice>(
            &Invoice::STABLE_NAME,
            PayloadVersion::INITIAL,
            br#"{"legacy":"INV-12","count":12}"#,
        )
        .unwrap_err();
        let ReadError::Decode(error) = &failure else {
            panic!("expected an adjacent conversion failure");
        };
        assert_early_failure(error);
        let outer_source = failure
            .source()
            .unwrap()
            .downcast_ref::<DecodeError>()
            .unwrap();
        assert_eq!(outer_source.context(), error.context());
    }

    #[test]
    fn versioned_conversion_stops_before_later_fields_and_versions() {
        TRACE.with(|trace| trace.borrow_mut().clear());
        let stored: Versioned<Invoice> = serde_json::from_str(
            r#"{"stable_name":"billing.invoice.issued","version":1,"payload":{"legacy":"INV-12","count":12}}"#,
        )
        .unwrap();
        assert_eq!(stored.source_version().get(), 1);
        TRACE.with(|trace| assert!(trace.borrow().is_empty()));

        let error = Invoice::from_versioned(stored).unwrap_err();
        assert_early_failure(&error);
    }
}
