//! Strict helpers for persisted storage profile metadata.

use serde_json::{Map, Value};
use type_history_core::resolved::StorageProfile;

use super::{JsonSchemaError, JsonSchemaErrorReason};

pub(super) const PROFILE_KEY: &str = "x-type-history-profile";

pub(super) fn read_profile(
    object: &Map<String, Value>,
    path: &str,
) -> Result<Option<StorageProfile>, JsonSchemaError> {
    let Some(id) = object.get(PROFILE_KEY) else {
        return Ok(None);
    };
    let id = id
        .as_str()
        .ok_or_else(|| JsonSchemaError::at(path, JsonSchemaErrorReason::Profile))?;
    let profile = StorageProfile::from_id(id)
        .map_err(|_| JsonSchemaError::at(path, JsonSchemaErrorReason::Profile))?;
    if object.get("type").and_then(Value::as_str) != Some(profile.json_type()) {
        return Err(JsonSchemaError::at(path, JsonSchemaErrorReason::Profile));
    }
    Ok(Some(profile))
}
