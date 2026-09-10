//! Dependency-neutral canonical JSON byte encoding shared by history internals.
//!
//! This module deliberately knows nothing about schema manifests, field
//! inventories, digest domains, or expected-versus-observed selection. Its
//! single responsibility is the mechanical JSON value-to-byte mapping.

use serde_json::{Error as JsonError, Value};

/// A JSON value is outside the canonical integer-only artifact subset.
#[derive(Debug, thiserror::Error)]
pub enum CanonicalJsonValueError {
    /// JSON string encoding failed.
    #[error("canonical JSON string encoding failed")]
    StringEncoding(#[from] JsonError),
    /// Non-integer JSON numbers are not part of the canonical artifact language.
    #[error("canonical artifacts require finite integer JSON numbers")]
    UnsupportedNumber,
}

/// Encode one JSON value into canonical UTF-8 bytes.
///
/// Object keys use RFC 8785 UTF-16 ordering. History artifacts admit only integer
/// JSON numbers, so number spelling is deterministic without a floating-point
/// normalization policy.
pub fn value_to_vec(value: &Value) -> Result<Vec<u8>, CanonicalJsonValueError> {
    let mut output = Vec::new();
    write_value(value, &mut output)?;
    Ok(output)
}

fn write_value(value: &Value, output: &mut Vec<u8>) -> Result<(), CanonicalJsonValueError> {
    match value {
        Value::Null => output.extend_from_slice(b"null"),
        Value::Bool(true) => output.extend_from_slice(b"true"),
        Value::Bool(false) => output.extend_from_slice(b"false"),
        Value::Number(number) => {
            if !(number.is_i64() || number.is_u64()) {
                return Err(CanonicalJsonValueError::UnsupportedNumber);
            }
            output.extend_from_slice(number.to_string().as_bytes());
        }
        Value::String(value) => output.extend_from_slice(serde_json::to_string(value)?.as_bytes()),
        Value::Array(values) => {
            output.push(b'[');
            for (index, value) in values.iter().enumerate() {
                if index != 0 {
                    output.push(b',');
                }
                write_value(value, output)?;
            }
            output.push(b']');
        }
        Value::Object(values) => {
            let mut entries = values.iter().collect::<Vec<_>>();
            entries.sort_by(|(left, _), (right, _)| left.encode_utf16().cmp(right.encode_utf16()));
            output.push(b'{');
            for (index, (key, value)) in entries.into_iter().enumerate() {
                if index != 0 {
                    output.push(b',');
                }
                output.extend_from_slice(serde_json::to_string(key)?.as_bytes());
                output.push(b':');
                write_value(value, output)?;
            }
            output.push(b'}');
        }
    }
    Ok(())
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
            Err(CanonicalJsonValueError::UnsupportedNumber)
        ));
    }
}
