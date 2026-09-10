//! Standalone safe decoding errors and typed identity dispatch.

use std::{
    error::Error,
    fmt::{Debug, Display, Formatter, Result as FmtResult},
};

use redactable::{BypassJsonRedaction, BypassTextRedaction, RedactedValue, ToRedacted};
use serde::{ser::SerializeStruct, Serialize, Serializer};

use crate::{
    DecodeContext, DecodeFailureKind, History, HistoryFailure, PayloadVersion, TypeIdentity,
};

/// Observation-safe standalone failure with original and adjacent version context.
#[derive(Clone, PartialEq, Eq)]
pub struct DecodeError(HistoryFailure<RedactedValue>);

impl DecodeError {
    /// Reject an unsupported stored version without inspecting its bytes.
    pub fn unsupported(identity: TypeIdentity, source_version: PayloadVersion) -> Self {
        Self(HistoryFailure::unsupported(identity, source_version))
    }

    /// Discard raw decoder details and retain a fixed safe wire failure.
    pub fn decode(identity: TypeIdentity, source_version: PayloadVersion) -> Self {
        Self(HistoryFailure::decode(
            identity,
            source_version,
            BypassTextRedaction("history wire decoding failed".to_owned()).to_redacted(),
        ))
    }

    /// Capture an author's error through its declared safe output exactly once.
    pub fn upcast<E>(
        identity: TypeIdentity,
        source_version: PayloadVersion,
        adjacent_from: PayloadVersion,
        adjacent_to: PayloadVersion,
        error: &E,
    ) -> Self
    where
        E: Debug + ToRedacted + Send + Sync + 'static,
    {
        Self(HistoryFailure::upcast(
            identity,
            source_version,
            adjacent_from,
            adjacent_to,
            error.to_redacted(),
        ))
    }

    /// Immutable identity and version context.
    pub const fn context(&self) -> &DecodeContext {
        self.0.context()
    }

    /// Stable logical record identity.
    pub fn identity(&self) -> &TypeIdentity {
        self.context().identity()
    }

    /// Original stored payload version, preserved throughout the chain.
    pub const fn source_version(&self) -> PayloadVersion {
        self.context().source_version()
    }

    /// Failing adjacent predecessor, if conversion started.
    pub const fn adjacent_from(&self) -> Option<PayloadVersion> {
        self.context().adjacent_from()
    }

    /// Failing adjacent destination, if conversion started.
    pub const fn adjacent_to(&self) -> Option<PayloadVersion> {
        self.context().adjacent_to()
    }

    /// Stable decode failure class.
    pub const fn kind(&self) -> DecodeFailureKind {
        self.context().kind()
    }

    /// Author-declared safe cause, or the fixed safe wire failure.
    pub fn redacted_cause(&self) -> Option<&RedactedValue> {
        self.0.safe_cause()
    }
}

impl Serialize for DecodeError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut record = serializer.serialize_struct("DecodeError", 6)?;
        record.serialize_field("identity", self.identity())?;
        record.serialize_field("source_version", &self.source_version())?;
        record.serialize_field("adjacent_from", &self.adjacent_from())?;
        record.serialize_field("adjacent_to", &self.adjacent_to())?;
        record.serialize_field("kind", &self.kind())?;
        record.serialize_field(
            "redacted_cause",
            &self.redacted_cause().map(RedactedValue::json),
        )?;
        record.end()
    }
}

impl Debug for DecodeError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        formatter
            .debug_struct("DecodeError")
            .field("identity", self.identity())
            .field("source_version", &self.source_version())
            .field("adjacent_from", &self.adjacent_from())
            .field("adjacent_to", &self.adjacent_to())
            .field("kind", &self.kind())
            .field(
                "redacted_cause",
                &self.redacted_cause().map(RedactedValue::json),
            )
            .finish()
    }
}

impl ToRedacted for DecodeError {
    fn to_redacted(&self) -> RedactedValue {
        // Serialization contains only validated metadata and captured safe JSON.
        BypassJsonRedaction(self).to_redacted()
    }
}

impl Display for DecodeError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        formatter.write_str(&self.to_redacted().text())
    }
}

impl Error for DecodeError {}

/// Current record with a generated exact-version history.
pub trait HasHistory: Sized {
    /// Stable persisted logical identity.
    const TYPE: TypeIdentity;
    /// Positive current payload version.
    const VERSION: PayloadVersion;
    /// Generated retained-version decoder and adjacent conversions.
    fn history() -> History<Self>;
}

/// Read a stored record after checking its exact logical identity.
pub fn decode<T: HasHistory>(
    identity: &TypeIdentity,
    version: PayloadVersion,
    bytes: &[u8],
) -> Result<T, ReadError> {
    if identity != &T::TYPE {
        return Err(ReadError::TypeMismatch {
            expected: T::TYPE,
            found: identity.clone(),
        });
    }
    T::history()
        .decode(version, bytes)
        .map_err(ReadError::Decode)
}

/// A stored identity mismatch or an exact-version decoding failure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum ReadError {
    /// Stored identity differs from the requested current record.
    TypeMismatch {
        /// Identity declared by the requested current record.
        expected: TypeIdentity,
        /// Identity read from storage.
        found: TypeIdentity,
    },
    /// Historical decoding or one adjacent conversion failed safely.
    Decode(DecodeError),
}

impl ToRedacted for ReadError {
    fn to_redacted(&self) -> RedactedValue {
        // Both variants serialize only validated identities and safe failures.
        BypassJsonRedaction(self).to_redacted()
    }
}

impl Display for ReadError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        formatter.write_str(&self.to_redacted().text())
    }
}

impl Error for ReadError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::TypeMismatch { .. } => None,
            Self::Decode(error) => Some(error),
        }
    }
}

#[cfg(test)]
mod tests;
