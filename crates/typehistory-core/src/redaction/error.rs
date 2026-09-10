//! Owned safe projections; raw errors are discarded at construction.

use crate::{DecodeContext, DecodeFailureKind, PayloadVersion, TypeIdentity};
use redactable::{BypassJsonRedaction, BypassTextRedaction, RedactedValue, ToRedacted};
use serde::{ser::SerializeStruct, Serialize, Serializer};
use serde_json::Error as JsonError;
use std::{
    error::Error,
    fmt::{Debug, Display, Formatter, Result as FmtResult},
};

/// Historical failure retaining context and an already projected safe cause.
///
/// `Debug`, `Display`, serialization, and `ToRedacted` observe only owned safe
/// values. `Error::source()` is `None`: neither raw JSON nor callback errors
/// remain reachable. Use [`Self::redacted_cause`] for the safe projection.
#[derive(Clone, PartialEq, Eq)]
pub struct RedactedDecodeError {
    context: DecodeContext,
    cause: Option<RedactedValue>,
}

impl RedactedDecodeError {
    /// Reject an unsupported stored version before reading payload bytes.
    pub fn unsupported(identity: TypeIdentity, version: PayloadVersion) -> Self {
        Self {
            context: DecodeContext::unsupported(identity, version),
            cause: None,
        }
    }

    /// Discard the decoder error without formatting or storing it.
    pub fn decode(identity: TypeIdentity, version: PayloadVersion, _: JsonError) -> Self {
        Self {
            context: DecodeContext::decode(identity, version),
            cause: Some(
                BypassTextRedaction("history wire decoding failed".to_owned()).to_redacted(),
            ),
        }
    }

    /// Project a concrete callback error once, before dropping its raw value.
    ///
    /// Only the declared `ToRedacted` contract is required. Its implementation
    /// and any custom policy are the author's responsibility. Raw formatting,
    /// `Error`, `Clone`, `Send`, `Sync`, and `'static` bounds are unnecessary.
    pub fn upcast<E: ToRedacted>(
        identity: TypeIdentity,
        version: PayloadVersion,
        from: PayloadVersion,
        to: PayloadVersion,
        error: E,
    ) -> Self {
        Self {
            context: DecodeContext::upcast(identity, version, from, to),
            cause: Some(error.to_redacted()),
        }
    }

    /// Immutable original and failing adjacent version context.
    pub const fn context(&self) -> &DecodeContext {
        &self.context
    }
    /// Logical record identity.
    pub fn identity(&self) -> &TypeIdentity {
        self.context.identity()
    }
    /// Original stored payload version.
    pub const fn source_version(&self) -> PayloadVersion {
        self.context.source_version()
    }
    /// Failing adjacent predecessor, if conversion began.
    pub const fn adjacent_from(&self) -> Option<PayloadVersion> {
        self.context.adjacent_from()
    }
    /// Failing adjacent destination, if conversion began.
    pub const fn adjacent_to(&self) -> Option<PayloadVersion> {
        self.context.adjacent_to()
    }
    /// Stable failure classification.
    pub const fn kind(&self) -> DecodeFailureKind {
        self.context.kind()
    }
    /// Captured safe callback projection or fixed safe JSON failure.
    pub fn redacted_cause(&self) -> Option<&RedactedValue> {
        self.cause.as_ref()
    }
}

impl Serialize for RedactedDecodeError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut record = serializer.serialize_struct("RedactedDecodeError", 6)?;
        record.serialize_field("identity", self.identity())?;
        record.serialize_field("source_version", &self.source_version())?;
        record.serialize_field("adjacent_from", &self.adjacent_from())?;
        record.serialize_field("adjacent_to", &self.adjacent_to())?;
        record.serialize_field("kind", &self.kind())?;
        record.serialize_field(
            "redacted_cause",
            &self.cause.as_ref().map(RedactedValue::json),
        )?;
        record.end()
    }
}

impl ToRedacted for RedactedDecodeError {
    fn to_redacted(&self) -> RedactedValue {
        // This serializer sees only validated metadata and captured safe JSON.
        BypassJsonRedaction(self).to_redacted()
    }
}
impl Debug for RedactedDecodeError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        Debug::fmt(&self.to_redacted(), formatter)
    }
}
impl Display for RedactedDecodeError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        formatter.write_str(&self.to_redacted().text())
    }
}
impl Error for RedactedDecodeError {}

/// Identity mismatch or safe historical failure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum RedactedReadError {
    /// The stored record belongs to another logical type.
    TypeMismatch {
        /// Requested type identity.
        expected: TypeIdentity,
        /// Stored type identity.
        found: TypeIdentity,
    },
    /// Safe exact-version decoding or conversion failure.
    Decode(RedactedDecodeError),
}
impl ToRedacted for RedactedReadError {
    fn to_redacted(&self) -> RedactedValue {
        BypassJsonRedaction(self).to_redacted()
    }
}
impl Display for RedactedReadError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        formatter.write_str(&self.to_redacted().text())
    }
}
impl Error for RedactedReadError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::TypeMismatch { .. } => None,
            Self::Decode(error) => Some(error),
        }
    }
}
