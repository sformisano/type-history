//! Normalized JSON Schema 2020-12 comparison documents for record wire snapshots.
//!
//! `schemars` generates a raw document from the current Rust types. Type History
//! normalizes that document and validates its closed structural [`SchemaShape`].
//! Development tooling compares these documents under each stable name and
//! version. Type History never validates business values against a schema document.

use std::{
    collections::BTreeMap,
    fmt::{Display, Formatter, Result as FmtResult},
};

use serde::{de::Error as _, Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use type_history_core::resolved::{FieldPresence, SchemaField, SchemaShape};

use crate::canonical::{self, CanonicalJsonError};

/// One normalized JSON Schema 2020-12 document describing a record wire shape.
///
/// Every constructor leaves the document normalized and structurally valid, so
/// [`JsonSchemaDocument::shape`] cannot fail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonSchemaDocument {
    identity: String,
    value: Value,
}

impl JsonSchemaDocument {
    /// JSON Schema dialect named by every document.
    pub const META_SCHEMA: &'static str = "https://json-schema.org/draft/2020-12/schema";

    /// Normalize one raw `schemars` export for the durable identity.
    pub fn from_export(mut raw: Value, identity: &str) -> Result<Self, JsonSchemaError> {
        let Some(root) = raw.as_object_mut() else {
            return Err(JsonSchemaError::at("", JsonSchemaErrorReason::NotAnObject));
        };
        root.insert(
            "$schema".to_owned(),
            Value::String(Self::META_SCHEMA.to_owned()),
        );
        root.insert("$id".to_owned(), Value::String(identity.to_owned()));
        normalize(&mut raw)?;
        Self::from_normalized(raw)
    }

    /// Write the normalized document for one resolved structural shape.
    pub fn from_shape(shape: &SchemaShape, identity: &str) -> Self {
        let mut value = write_node(shape);
        let root = value.as_object_mut().expect("written nodes are objects");
        root.insert(
            "$schema".to_owned(),
            Value::String(Self::META_SCHEMA.to_owned()),
        );
        root.insert("$id".to_owned(), Value::String(identity.to_owned()));
        Self::from_normalized(value).expect("written documents are normalized and valid")
    }

    /// Accept one stored document that must already be normalized.
    pub fn from_normalized(value: Value) -> Result<Self, JsonSchemaError> {
        let mut normalized = value.clone();
        normalize(&mut normalized)?;
        if normalized != value {
            return Err(JsonSchemaError::at(
                "",
                JsonSchemaErrorReason::NotNormalized,
            ));
        }
        let identity = read_identity(&value)?;
        parse(&value)?;
        Ok(Self { identity, value })
    }

    /// Durable identity carried by `$id`.
    pub fn identity(&self) -> &str {
        &self.identity
    }

    /// Borrow the normalized document.
    pub fn as_value(&self) -> &Value {
        &self.value
    }

    /// Structural shape described by the document.
    pub fn shape(&self) -> SchemaShape {
        parse(&self.value).expect("constructed documents are structurally valid")
    }

    /// Canonical RFC 8785 bytes of the complete document.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, CanonicalJsonError> {
        canonical::value_to_vec(&self.value)
    }
}

impl Serialize for JsonSchemaDocument {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.value.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for JsonSchemaDocument {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        Self::from_normalized(value).map_err(D::Error::custom)
    }
}

/// Build a record shape from named field shapes, assigning option presence.
pub fn record(fields: BTreeMap<String, SchemaShape>) -> SchemaShape {
    SchemaShape::Record {
        fields: fields
            .into_iter()
            .map(|(name, schema)| SchemaField {
                presence: presence_of(&schema),
                name,
                schema,
            })
            .collect(),
    }
}

fn presence_of(schema: &SchemaShape) -> FieldPresence {
    if matches!(schema, SchemaShape::Option { .. }) {
        FieldPresence::Optional
    } else {
        FieldPresence::Required
    }
}

/// A document is outside the normalized Type History JSON Schema subset.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub struct JsonSchemaError {
    /// JSON pointer of the offending node, empty for the root.
    pub path: String,
    /// Why the node was rejected.
    pub reason: JsonSchemaErrorReason,
}

impl JsonSchemaError {
    fn at(path: &str, reason: JsonSchemaErrorReason) -> Self {
        Self {
            path: path.to_owned(),
            reason,
        }
    }
}

impl Display for JsonSchemaError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        if self.path.is_empty() {
            write!(formatter, "JSON Schema document root: {}", self.reason)
        } else {
            write!(
                formatter,
                "JSON Schema document node `{}`: {}",
                self.path, self.reason
            )
        }
    }
}

/// Why one JSON Schema node was rejected.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum JsonSchemaErrorReason {
    /// A schema node was not a JSON object.
    #[error("schema nodes must be JSON objects")]
    NotAnObject,
    /// The stored bytes change under normalization.
    #[error("document is not normalized")]
    NotNormalized,
    /// `$schema` was absent or not the 2020-12 dialect.
    #[error("`$schema` must be `{}`", JsonSchemaDocument::META_SCHEMA)]
    MetaSchema,
    /// `$id` was absent or did not carry a durable identity.
    #[error("`$id` must be a nonempty complete schema identity")]
    Identity,
    /// `$ref`, `$defs`, or `definitions` appeared.
    #[error("references and definitions are not admitted; export with inline subschemas")]
    Reference,
    /// A keyword outside the closed subset appeared.
    #[error("keyword `{0}` is not part of the Type History JSON Schema subset")]
    UnknownKeyword(String),
    /// A keyword carried a value of the wrong JSON type.
    #[error("keyword `{0}` has an invalid value")]
    InvalidKeyword(String),
    /// A number was not an integer.
    #[error("numbers must be integers")]
    Number,
    /// `type` named something outside the subset.
    #[error("`type` must name one of boolean, string, integer, array, or object")]
    Type,
    /// An integer lacked a width format or named an unknown one.
    #[error("integers require a `format` naming one of the eleven Rust integer widths")]
    IntegerFormat,
    /// Integer bounds disagreed with the width.
    #[error("integer bounds do not match the width named by `format`")]
    IntegerBounds,
    /// An array length constraint was inconsistent.
    #[error("fixed arrays require equal `minItems` and `maxItems`")]
    ArrayLength,
    /// An object lacked `additionalProperties: false`.
    #[error("objects require `additionalProperties: false`")]
    AdditionalProperties,
    /// `required` disagreed with the nullable properties.
    #[error("`required` must list exactly the properties that do not admit `null`")]
    Required,
    /// An optional node was not exactly a value or `null` alternative.
    #[error("`anyOf` must contain exactly one value schema and one `null` schema")]
    Option,
    /// An enum node was malformed.
    #[error("enumerations require unique string names, and a single name uses `const`")]
    Enum,
    /// A `oneOf` entry did not describe one variant.
    #[error("each `oneOf` entry must be a unit `const` or one externally tagged variant")]
    Variant,
    /// A tuple variant carried fewer than two items.
    #[error("tuple variants require at least two `prefixItems`")]
    Tuple,
    /// Serialization or canonicalization failed.
    #[error("canonical JSON serialization failed")]
    Canonical,
}

impl From<CanonicalJsonError> for JsonSchemaError {
    fn from(error: CanonicalJsonError) -> Self {
        match error {
            CanonicalJsonError::UnsupportedNumber => Self::at("", JsonSchemaErrorReason::Number),
            CanonicalJsonError::Serialize(_) => Self::at("", JsonSchemaErrorReason::Canonical),
        }
    }
}

const NULL_TYPE: &str = "null";

fn read_identity(value: &Value) -> Result<String, JsonSchemaError> {
    let root = value
        .as_object()
        .ok_or_else(|| JsonSchemaError::at("", JsonSchemaErrorReason::NotAnObject))?;
    if root.get("$schema").and_then(Value::as_str) != Some(JsonSchemaDocument::META_SCHEMA) {
        return Err(JsonSchemaError::at("", JsonSchemaErrorReason::MetaSchema));
    }
    root.get("$id")
        .and_then(Value::as_str)
        .filter(|identity| !identity.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| JsonSchemaError::at("", JsonSchemaErrorReason::Identity))
}

// ---------------------------------------------------------------------------
// Normalization
// ---------------------------------------------------------------------------

mod normalize;
use normalize::normalize;

// ---------------------------------------------------------------------------
// Writer
// ---------------------------------------------------------------------------

mod write;
use write::write_node;

// ---------------------------------------------------------------------------

mod read;
pub use read::parse;

#[cfg(test)]
mod tests;
