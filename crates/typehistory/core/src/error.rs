//! Decoding errors with original sources and typed identity dispatch.

use std::{
    error::Error,
    fmt::{Display, Formatter, Result as FmtResult},
};

use serde_json::Error as JsonError;

use crate::{DecodeContext, DecodeFailureKind, History, PayloadVersion, TypeIdentity};

/// Decoding failure with original and adjacent version context.
///
/// [`Error::source`] exposes the original decoder or conversion error.
#[derive(Debug)]
pub struct DecodeError {
    context: DecodeContext,
    source: Option<Box<dyn Error + Send + Sync + 'static>>,
}

impl DecodeError {
    /// Reject an unsupported stored version without inspecting its bytes.
    pub fn unsupported(identity: TypeIdentity, source_version: PayloadVersion) -> Self {
        Self {
            context: DecodeContext::unsupported(identity, source_version),
            source: None,
        }
    }

    /// Retain the original JSON decoder error and stored version.
    pub fn decode(
        identity: TypeIdentity,
        source_version: PayloadVersion,
        error: JsonError,
    ) -> Self {
        Self {
            context: DecodeContext::decode(identity, source_version),
            source: Some(Box::new(error)),
        }
    }

    /// Retain an owned conversion error and the failing adjacent versions.
    pub fn upcast<E>(
        identity: TypeIdentity,
        source_version: PayloadVersion,
        adjacent_from: PayloadVersion,
        adjacent_to: PayloadVersion,
        error: E,
    ) -> Self
    where
        E: Error + Send + Sync + 'static,
    {
        Self {
            context: DecodeContext::upcast(identity, source_version, adjacent_from, adjacent_to),
            source: Some(Box::new(error)),
        }
    }

    /// Immutable identity and version context.
    pub const fn context(&self) -> &DecodeContext {
        &self.context
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
}

impl Display for DecodeError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        let failure = match self.kind() {
            DecodeFailureKind::UnsupportedVersion => "unsupported stored version",
            DecodeFailureKind::Decode => "wire decoding failed",
            DecodeFailureKind::Upcast => "conversion failed",
        };
        write!(
            formatter,
            "{failure} for {} at stored version {}",
            self.identity(),
            self.source_version(),
        )?;
        if let (Some(from), Some(to)) = (self.adjacent_from(), self.adjacent_to()) {
            write!(formatter, " during version {from} to {to}")?;
        }
        Ok(())
    }
}

impl Error for DecodeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source
            .as_deref()
            .map(|source| source as &(dyn Error + 'static))
    }
}

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
#[derive(Debug)]
pub enum ReadError {
    /// Stored identity differs from the requested current record.
    TypeMismatch {
        /// Identity declared by the requested current record.
        expected: TypeIdentity,
        /// Identity read from storage.
        found: TypeIdentity,
    },
    /// Historical decoding or one adjacent conversion failed.
    Decode(DecodeError),
}

impl Display for ReadError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        match self {
            Self::TypeMismatch { expected, found } => write!(
                formatter,
                "stored type {found} does not match expected type {expected}",
            ),
            Self::Decode(error) => write!(
                formatter,
                "cannot read {} at stored version {}",
                error.identity(),
                error.source_version(),
            ),
        }
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
