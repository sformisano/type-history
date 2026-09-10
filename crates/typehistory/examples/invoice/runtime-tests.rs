#[cfg(test)]
mod tests {
    use super::{Invoice, InvoiceV3};
    use redactable::ToRedacted;
    use typehistory::{decode, DecodeFailureKind, HasHistory, PayloadVersion, ReadError};

    #[test]
    fn old_bytes_reach_the_current_type() {
        let value = decode::<Invoice>(
            &Invoice::TYPE,
            PayloadVersion::INITIAL,
            br#"{"legacy":"PAYLOAD_SECRET","count":42}"#,
        )
        .unwrap();
        assert_eq!(value.count, "42");
        assert_eq!(value.label, "PAYLOAD_SECRET");
        assert_eq!(value.revision, 7);
        assert_eq!(Invoice::VERSION.get(), 3);
        let _: Invoice = InvoiceV3 {
            count: "42".to_owned(),
            label: "current".to_owned(),
            revision: 7,
        };
        assert!(!format!("{value:?}").contains("PAYLOAD_SECRET"));
        assert!(!value.to_redacted().text().contains("PAYLOAD_SECRET"));
    }

    #[test]
    fn failure_keeps_the_original_version_and_failing_step() {
        let failure = decode::<Invoice>(
            &Invoice::TYPE,
            PayloadVersion::INITIAL,
            br#"{"legacy":"PAYLOAD_SECRET","count":13}"#,
        )
        .unwrap_err();
        let ReadError::Decode(error) = &failure else {
            panic!("expected a conversion failure");
        };
        assert_eq!(error.source_version().get(), 1);
        assert_eq!(error.adjacent_from().unwrap().get(), 2);
        assert_eq!(error.adjacent_to().unwrap().get(), 3);
        assert_eq!(error.kind(), DecodeFailureKind::Upcast);
        assert!(failure.to_string().contains("safe conversion failure"));
        assert!(!serde_json::to_string(&failure).unwrap().contains("PAYLOAD_SECRET"));
    }
}
