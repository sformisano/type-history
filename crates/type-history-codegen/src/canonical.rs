//! RFC 8785 canonical JSON serialization for type-history schema artifacts.

use serde::Serialize;
use serde_json::{Error as JsonError, Value};
use type_history_core::canonical_json::CanonicalJsonValueError;

/// A value could not be represented as canonical JSON.
#[derive(Debug, thiserror::Error)]
pub enum CanonicalJsonError {
    /// Serde could not convert the value into JSON.
    #[error("canonical JSON serialization failed")]
    Serialize(#[from] JsonError),
    /// Non-integer JSON numbers are not part of the canonical artifact language.
    #[error("Canonical artifacts require finite integer JSON numbers")]
    UnsupportedNumber,
}

impl From<CanonicalJsonValueError> for CanonicalJsonError {
    fn from(error: CanonicalJsonValueError) -> Self {
        match error {
            CanonicalJsonValueError::StringEncoding(error) => Self::Serialize(error),
            CanonicalJsonValueError::UnsupportedNumber => Self::UnsupportedNumber,
        }
    }
}

/// Serialize a history artifact into deterministic RFC 8785-compatible UTF-8.
///
/// History artifact schemas deliberately use only strings, booleans, null,
/// arrays, objects, and integer numbers. This keeps the number-serialization
/// subset unambiguous while object keys are ordered by UTF-16 code units as
/// required by RFC 8785.
pub fn to_vec<T: Serialize>(value: &T) -> Result<Vec<u8>, CanonicalJsonError> {
    let value = serde_json::to_value(value)?;
    value_to_vec(&value)
}

/// Serialize one JSON value into deterministic canonical UTF-8.
pub fn value_to_vec(value: &Value) -> Result<Vec<u8>, CanonicalJsonError> {
    type_history_core::canonical_json::value_to_vec(value).map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn object_keys_use_utf16_order_and_no_whitespace() {
        let value = json!({"z": 1, "\u{1f600}": 2, "\u{fffd}": 3, "a": [true, null]});
        assert_eq!(
            String::from_utf8(value_to_vec(&value).expect("canonical JSON")).expect("UTF-8"),
            "{\"a\":[true,null],\"z\":1,\"😀\":2,\"�\":3}"
        );
    }

    #[test]
    fn rejects_floating_point_numbers() {
        assert!(matches!(
            value_to_vec(&json!({"number": 1.5})),
            Err(CanonicalJsonError::UnsupportedNumber)
        ));
    }
}
