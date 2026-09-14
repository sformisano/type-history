#[history_api::versioned(stable_name = "example.wide.record")]
pub struct Wide {
    pub unsigned: u128,
    pub signed: i128,
}

#[cfg(test)]
mod stored_tests {
    use super::{ConvertError, Invoice, TRACE, Wide};
    use history_api::{DecodeFailureKind, Versioned};
    use std::error::Error;

    const V1_RECORD: &str = r#"{"stable_name":"billing.invoice.issued","version":1,"payload":{"legacy":"INV-42","count":42}}"#;

    fn trace() -> Vec<&'static str> {
        TRACE.with(|trace| trace.borrow().clone())
    }

    #[test]
    fn historical_roundtrip_does_not_run_callbacks_until_consumed() {
        TRACE.with(|trace| trace.borrow_mut().clear());
        let stored: Versioned<Invoice> = serde_json::from_str(V1_RECORD).unwrap();
        assert_eq!(stored.stable_name().as_str(), "billing.invoice.issued");
        assert_eq!(stored.source_version().get(), 1);
        assert_eq!(serde_json::to_string(&stored).unwrap(), V1_RECORD);
        assert!(!format!("{stored:?}").contains("INV-42"));
        assert!(trace().is_empty());
        let current = Invoice::from_versioned(stored).unwrap();
        assert_eq!(trace(), ["widen", "label", "stringify"]);
        assert_eq!(
            (&*current.count, &*current.label, current.revision),
            ("42", "INV-42", 7)
        );
        let current_record = current.into_versioned();
        assert_eq!(current_record.source_version().get(), 3);
        let restored = Invoice::from_versioned(current_record).unwrap();
        assert_eq!(restored.count, "42");
        assert_eq!(trace(), ["widen", "label", "stringify"]);
    }

    #[test]
    fn invalid_envelopes_never_start_conversions_even_after_valid_payloads() {
        TRACE.with(|trace| trace.borrow_mut().clear());
        for wire in [
            r#"{"stable_name":"billing.invoice.issued","version":1,"payload":{"legacy":"x","count":42},"version":2,"payload":{}}"#,
            r#"{"stable_name":"billing.invoice.issued","version":1,"payload":{"legacy":"x","count":42},"stable_name":"other"}"#,
            r#"{"stable_name":"billing.invoice.issued","version":1,"payload":{"legacy":"x","count":42},"version":1,"payload":{"legacy":"x","count":42}}"#,
            r#"{"stable_name":"billing.invoice.issued","version":1,"payload":{"legacy":"x","count":42},"stable_name":"billing.invoice.issued","version":1,"payload":{"legacy":"x","count":42}}"#,
            r#"{"stable_name":"billing.invoice.issued","version":1,"payload":{"legacy":"x","count":42,"extra":true}}"#,
            r#"{"stable_name":"billing.invoice.issued","version":1,"payload":{"legacy":"x","count":42}"#,
            r#"{"payload":{"legacy":"x","count":42,"count":43},"version":1,"stable_name":"billing.invoice.issued"}"#,
            r#"{"payload":{"legacy":"x","count":42},"version":1,"stable_name":"billing.invoice.issued","extra":true}"#,
            r#"{"stable_name":"billing.invoice.issued","version":1,"payload":{"legacy":"x","count":42}} trailing"#,
        ] {
            assert!(
                serde_json::from_str::<Versioned<Invoice>>(wire).is_err(),
                "accepted {wire}"
            );
            assert!(trace().is_empty(), "converted {wire}");
        }
    }

    #[test]
    fn delayed_failure_keeps_original_source_and_adjacent_context() {
        TRACE.with(|trace| trace.borrow_mut().clear());
        let stored: Versioned<Invoice> =
            serde_json::from_str(&V1_RECORD.replace("42", "13")).unwrap();
        assert!(trace().is_empty());
        let error = Invoice::from_versioned(stored).unwrap_err();
        assert_eq!(error.kind(), DecodeFailureKind::Upcast);
        assert_eq!(error.source_version().get(), 1);
        assert_eq!(error.adjacent_from().unwrap().get(), 2);
        assert_eq!(error.adjacent_to().unwrap().get(), 3);
        assert_eq!(
            error
                .source()
                .unwrap()
                .downcast_ref::<ConvertError>()
                .unwrap()
                .0,
            13
        );
        assert_eq!(trace(), ["widen", "label", "stringify"]);
    }

    #[test]
    fn single_version_full_width_integers_keep_their_exact_values() {
        let wire = format!(
            r#"{{"stable_name":"example.wide.record","version":1,"payload":{{"unsigned":{},"signed":{}}}}}"#,
            u128::MAX,
            i128::MIN
        );
        let stored: Versioned<Wide> = serde_json::from_str(&wire).unwrap();
        assert_eq!(stored.source_version().get(), 1);
        assert_eq!(serde_json::to_string(&stored).unwrap(), wire);
        let current = Wide::from_versioned(stored).unwrap();
        assert_eq!(current.unsigned, u128::MAX);
        assert_eq!(current.signed, i128::MIN);
        assert_eq!(
            serde_json::to_string(&current.into_versioned()).unwrap(),
            wire
        );
    }
}
