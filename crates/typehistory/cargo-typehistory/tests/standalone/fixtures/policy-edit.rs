#[test]
fn policy_edit_changes_historical_projection_without_changing_payload() {
    use history_api::{PayloadVersion, redaction::HasRedactedHistory};
    use history_api::redactable::ToRedacted;
    let bytes = br#"{"legacy":"LEGACY_POLICY_SECRET_726","count":41,"nested":{"secret":"NESTED_SECRET_726"},"wrapped":"WRAPPED_SECRET_726","public":"visible"}"#;
    let historical: SecureV1 = serde_json::from_slice(bytes).unwrap();
    assert_eq!(historical.to_redacted().json()["legacy"], "LEGACY_POLICY_SECRET_726");
    assert_eq!(serde_json::to_vec(&historical).unwrap(), bytes);
    assert_eq!(format!("{historical:?}"), "SecureV1(<redacted>)");
    let current = Secure::history().decode(PayloadVersion::INITIAL, bytes).unwrap();
    assert_eq!(current.label, "LEGACY_POLICY_SECRET_726");
    assert_eq!(current.to_redacted().json()["label"], "[REDACTED]");
}
