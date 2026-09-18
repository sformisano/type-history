//! Strict helpers for persisted collection contract metadata.

use serde_json::Value;
use type_history_core::resolved::Membership;

use super::{JsonSchemaError, JsonSchemaErrorReason};

pub(super) const MEMBERSHIP_KEY: &str = "x-type-history-membership";

pub(super) fn read_membership(value: &Value, path: &str) -> Result<Membership, JsonSchemaError> {
    serde_json::from_value(value.clone())
        .map_err(|_| JsonSchemaError::at(path, JsonSchemaErrorReason::Membership))
}
