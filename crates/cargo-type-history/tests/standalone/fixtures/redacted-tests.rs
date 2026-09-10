#[cfg(test)]
mod tests {
    use super::{Secure, SecureV1, SecureV2, SecureV3};
    use history_api::{DecodeFailureKind, PayloadVersion, StableName};
    use history_api::redaction::{self, HasRedactedHistory};
    use history_api::redactable::{Redactable, ToRedacted};
    use serde_json::json;
    use std::error::Error;

    const BYTES: &[u8] = br#"{"legacy":"LEGACY_SECRET_726","count":41,"nested":{"secret":"NESTED_SECRET_726"},"wrapped":"WRAPPED_SECRET_726","public":"visible"}"#;

    #[test]
    fn every_record_surface_preserves_wire_and_redacts_output() {
        let first: SecureV1 = serde_json::from_slice(BYTES).unwrap();
        assert_eq!(serde_json::to_vec(&first).unwrap(), BYTES);
        assert_eq!(format!("{first:?}"), "SecureV1(<redacted>)");
        let safe = first.to_redacted().json();
        assert_eq!(safe["public"], "visible");
        assert_eq!(safe["count"], 0);
        assert_eq!(safe["legacy"], "[REDACTED]");
        let current = redaction::decode::<Secure>(&Secure::STABLE_NAME, PayloadVersion::INITIAL, BYTES).unwrap();
        assert_eq!(current.label, "LEGACY_SECRET_726");
        assert_eq!(current.count, "41");
        let second: SecureV2 = serde_json::from_value(json!({"count":41,"label":"LEGACY_SECRET_726","nested":{"secret":"NESTED_SECRET_726"},"wrapped":"WRAPPED_SECRET_726","public":"visible"})).unwrap();
        let third: &SecureV3 = &current;
        for output in [
            format!("{first:?}"), format!("{second:?}"), format!("{third:?}"),
            first.to_redacted().text(), second.to_redacted().text(), third.to_redacted().text(),
            safe.to_string(), first.to_redacted().json().to_string(),
            serde_json::to_string(&first.clone().redact()).unwrap(),
        ] {
            for secret in ["LEGACY_SECRET_726", "NESTED_SECRET_726", "WRAPPED_SECRET_726"] {
                assert!(!output.contains(secret), "{output}");
            }
        }
        assert_eq!(serde_json::to_vec(&first).unwrap(), BYTES);
        for invalid in [br#"[]"#.as_slice(), br#"{"count":"JSON_SECRET_726"}"#, br#"{"count":1,"count":2}"#, br#"{"unknown":"JSON_SECRET_726"}"#] {
            let error = Secure::history().decode(PayloadVersion::INITIAL, invalid).unwrap_err();
            assert_eq!(error.kind(), DecodeFailureKind::Decode);
            assert!(error.source().is_none());
            for output in [format!("{error}"), format!("{error:?}"), serde_json::to_string(&error).unwrap(), error.to_redacted().text(), error.to_redacted().json().to_string()] {
                assert!(!output.contains("JSON_SECRET_726"));
            }
        }
    }

    #[test]
    fn errors_keep_context_and_only_safe_causes() {
        let failed = String::from_utf8(BYTES.to_vec()).unwrap().replace("41", "999");
        let error = redaction::decode::<Secure>(&Secure::STABLE_NAME, PayloadVersion::INITIAL, failed.as_bytes()).unwrap_err();
        let cause = error.source().unwrap();
        assert!(cause.source().is_none());
        let context = error.to_redacted().json();
        let context = &context["Decode"];
        assert_eq!(context["stable_name"], "example.secure.record");
        assert_eq!(context["source_version"], 1);
        assert_eq!(context["adjacent_from"], 2);
        assert_eq!(context["adjacent_to"], 3);
        assert_eq!(context["kind"], "Upcast");
        assert_eq!(context["redacted_cause"]["message"], "safe conversion cause");
        let unsupported = Secure::history().decode(PayloadVersion::try_from(4).unwrap(), b"JSON_SECRET_726").unwrap_err();
        assert_eq!(unsupported.kind(), DecodeFailureKind::UnsupportedVersion);
        assert!(unsupported.redacted_cause().is_none());
        let mismatch = redaction::decode::<Secure>(&StableName::new("other.record.type"), PayloadVersion::INITIAL, BYTES).unwrap_err();
        assert!(mismatch.source().is_none());
    }
}
