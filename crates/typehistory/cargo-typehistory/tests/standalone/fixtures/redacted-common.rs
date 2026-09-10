use history_api::redactable::{BypassTextRedaction, Redactable, RedactableMapper, RedactableWithMapper, RedactedValue, Secret, RedactionPolicy, SecretPolicyKind, TextRedactionPolicy, SensitiveValue, ToRedacted};
use serde::{Deserialize, Serialize};
use std::fmt::{Debug, Formatter, Result as FmtResult};
use history_api::redactable::__private::PolicyField;

// Generated formatting must not invoke unrelated consumer macros.
#[allow(unused_macros)]
macro_rules! concat { ($($tokens:tt)*) => { compile_error!("consumer concat invoked") }; }
#[allow(unused_macros)]
macro_rules! stringify { ($($tokens:tt)*) => { "CONSUMER_STRINGIFY_SECRET_726" }; }

#[derive(Clone, PartialEq, Serialize, Deserialize, history_api::Schema)]
#[serde(deny_unknown_fields)]
pub struct Details { pub secret: String }
impl Debug for Details {
    fn fmt(&self, _: &mut Formatter<'_>) -> FmtResult { panic!("RAW_NESTED_DEBUG_SECRET") }
}
impl RedactableWithMapper for Details {
    fn redact_with<M: RedactableMapper>(self, mapper: &M) -> Self {
        Self { secret: <String as PolicyField<Secret>>::apply_field(self.secret, mapper) }
    }
}
impl Redactable for Details {}
impl ToRedacted for Details {
    fn to_redacted(&self) -> RedactedValue { history_api::redactable::__private::generated_redacted_json(self) }
}

// Deliberately has no Error, Debug, or Clone implementation.
pub struct CallbackFailure;
impl ToRedacted for CallbackFailure {
    fn to_redacted(&self) -> RedactedValue { BypassTextRedaction("safe conversion cause".to_owned()).to_redacted() }
}
fn widen(value: u32) -> Result<u64, CallbackFailure> { Ok(u64::from(value)) }
fn text(value: u64) -> Result<String, CallbackFailure> {
    if value == 999 { Err(CallbackFailure) } else { Ok(value.to_string()) }
}
fn label(previous: &SecureV1) -> Result<String, CallbackFailure> { Ok(previous.legacy.clone()) }

#[derive(Clone, PartialEq)]
pub struct WrapperSecret;
impl RedactionPolicy for WrapperSecret {
    type Kind = SecretPolicyKind;
    fn policy() -> TextRedactionPolicy { Secret::policy() }
}
