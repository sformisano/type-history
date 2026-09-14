use serde::{Deserialize, Serialize};
use serde_json::{Deserializer, Error as JsonError};
use thiserror::Error;
use type_history_core::resolved::SchemaShape;

/// One complete observation emitted by a failed unconditional frozen assertion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrozenSchemaObservation {
    /// Durable containing-history identity.
    pub stable_name: String,
    /// Positive frozen payload version.
    pub version: u32,
    /// Previously frozen descriptor.
    pub expected: SchemaShape,
    /// Descriptor resolved by the failing compiler invocation.
    pub actual: SchemaShape,
}

/// A present compiler marker could not be interpreted as a schema observation.
#[derive(Debug, Error)]
pub enum DiagnosticDecodeError {
    /// JSON or a nested schema descriptor is malformed.
    #[error("invalid frozen schema diagnostic: {0}")]
    InvalidJson(#[from] JsonError),
    /// A marker used the reserved zero version.
    #[error("invalid frozen schema diagnostic: version must be positive")]
    InvalidVersion,
}

/// Read a marker from compiler prose, allowing compiler text after the JSON object.
pub fn parse_compiler_marker(
    message: &str,
) -> Result<Option<FrozenSchemaObservation>, DiagnosticDecodeError> {
    let Some((_, payload)) = message.split_once("TYPE_HISTORY_SCHEMA_DIAGNOSTIC_V1:") else {
        return Ok(None);
    };
    let mut deserializer = Deserializer::from_str(payload);
    let mut observation = FrozenSchemaObservation::deserialize(&mut deserializer)?;
    if observation.version == 0 {
        return Err(DiagnosticDecodeError::InvalidVersion);
    }
    observation.expected = observation.expected.normalized();
    observation.actual = observation.actual.normalized();
    Ok(Some(observation))
}
