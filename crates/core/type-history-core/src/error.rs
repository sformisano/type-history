//! Decoding errors with original sources and dispatch by stable name.

use std::{
    error::Error,
    fmt::{Display, Formatter, Result as FmtResult},
};

use serde_json::Error as JsonError;

use crate::{DecodeContext, DecodeFailureKind, History, PayloadVersion, StableName};

/// Decoding failure with original and adjacent version context.
///
/// If a V1 record reaches V2 but its V2-to-V3 conversion fails, this error keeps
/// V1 as the stored version and V2-to-V3 as the failing step.
/// [`Error::source`] exposes the original decoder or conversion error.
#[derive(Debug)]
pub struct DecodeError {
    context: DecodeContext,
    source: Option<Box<dyn Error + Send + Sync + 'static>>,
}

impl DecodeError {
    /// Reject an unsupported stored version without inspecting its bytes.
    pub fn unsupported(stable_name: StableName, source_version: PayloadVersion) -> Self {
        Self {
            context: DecodeContext::unsupported(stable_name, source_version),
            source: None,
        }
    }

    /// Retain the original JSON decoder error and stored version.
    pub fn decode(
        stable_name: StableName,
        source_version: PayloadVersion,
        error: JsonError,
    ) -> Self {
        Self {
            context: DecodeContext::decode(stable_name, source_version),
            source: Some(Box::new(error)),
        }
    }

    /// Retain an owned conversion error and the failing adjacent versions.
    pub fn upcast<E>(
        stable_name: StableName,
        source_version: PayloadVersion,
        adjacent_from: PayloadVersion,
        adjacent_to: PayloadVersion,
        error: E,
    ) -> Self
    where
        E: Error + Send + Sync + 'static,
    {
        Self {
            context: DecodeContext::upcast(stable_name, source_version, adjacent_from, adjacent_to),
            source: Some(Box::new(error)),
        }
    }

    /// Immutable stable name and version context.
    pub const fn context(&self) -> &DecodeContext {
        &self.context
    }

    /// Stable name shared by every version of this type.
    pub fn stable_name(&self) -> &StableName {
        self.context().stable_name()
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
            self.stable_name(),
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
    /// Stable name shared by every version of this type.
    const STABLE_NAME: StableName;
    /// Positive current payload version.
    const VERSION: PayloadVersion;
    /// Generated retained-version decoder and adjacent conversions.
    fn history() -> History<Self>;
}

/// Read a JSON payload whose stable name and version are stored separately.
///
/// The stable name must match `T`. The generated decoder selects the historical
/// type for `version`, then applies adjacent conversions to return the current
/// type. Unsupported versions, malformed payloads, and failed conversions return
/// an error. For an envelope that includes metadata, use [`crate::Versioned`].
pub fn decode<T: HasHistory>(
    stable_name: &StableName,
    version: PayloadVersion,
    bytes: &[u8],
) -> Result<T, ReadError> {
    if stable_name != &T::STABLE_NAME {
        return Err(ReadError::TypeMismatch {
            expected: T::STABLE_NAME,
            found: stable_name.clone(),
        });
    }
    T::history()
        .decode(version, bytes)
        .map_err(ReadError::Decode)
}

/// A stored stable name mismatch or an exact-version decoding failure.
#[derive(Debug)]
pub enum ReadError {
    /// Stored stable name differs from the requested current record.
    TypeMismatch {
        /// Stable name declared by the requested current record.
        expected: StableName,
        /// Stable name read from storage.
        found: StableName,
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
                error.stable_name(),
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
