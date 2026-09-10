use redaction_api::{BypassRedaction, BypassDebugRedaction, BypassDisplayRedaction};

#[derive(Clone, PartialEq, serde::Serialize, serde::Deserialize, history_api::Schema, redaction_api::Sensitive)]
#[serde(deny_unknown_fields)]
pub struct Nested {
    #[sensitive(redaction_api::Secret)]
    pub secret: String,
}
#[history_api::history(stable_id = "example.nested.child", redacted)]
pub struct Child { pub nested: Nested }
#[history_api::history(stable_id = "example.nested.parent", redacted)]
pub struct Parent { pub child: Option<Child> }
#[history_api::history(stable_id = "example.wrapper.record", redacted)]
pub struct Wrappers {
    pub declared: BypassRedaction<String>,
    #[not_sensitive]
    pub display: BypassDisplayRedaction<String>,
    #[not_sensitive]
    pub debug: BypassDebugRedaction<String>,
}
#[test]
fn nested_histories_and_renamed_derive() {
    use history_api::redactable::ToRedacted;
    use serde_json::Value;
    let parent = Parent { child: Some(Child { nested: Nested { secret: "RENAMED_SECRET_726".to_owned() } }) };
    assert!(!parent.to_redacted().text().contains("RENAMED_SECRET_726"));
    assert!(!format!("{parent:?}").contains("RENAMED_SECRET_726"));
    assert!(serde_json::to_string(&parent).unwrap().contains("RENAMED_SECRET_726"));
    let bytes = br#"{"declared":"public","display":"public","debug":"public"}"#;
    let wrappers: Wrappers = serde_json::from_slice(bytes).unwrap();
    assert_eq!(serde_json::to_vec(&wrappers).unwrap(), bytes);
    assert_eq!(wrappers.to_redacted().json(), serde_json::from_slice::<Value>(bytes).unwrap());
}
