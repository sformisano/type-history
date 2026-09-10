//! A value with checked identity, version, and historical data.

mod serde_impl;

use std::fmt::{Debug, Formatter, Result as FmtResult};

use serde::{Deserializer, Serialize};

use crate::{PayloadVersion, TypeIdentity};

/// Generated bridge between a current record and its retained historical types.
#[doc(hidden)]
pub trait VersionedHistory: Sized {
    /// Generated enum containing one retained version's data.
    type Historical: Serialize;
    /// Conversion failure for this history's ordinary or redacted error contract.
    type Error;

    /// Frozen identity shared by every version of this history.
    const STABLE_ID: TypeIdentity;
    /// Version generated for the current alias.
    const CURRENT_VERSION: PayloadVersion;
    /// Whether deserialization failures must discard untrusted error messages.
    const REDACTED: bool;

    /// Wrap the current value in the generated historical enum.
    fn into_historical(self) -> Self::Historical;
    /// Obtain the version corresponding to the contained historical value.
    fn source_version(value: &Self::Historical) -> PayloadVersion;
    /// Deserialize exactly the requested retained type, without converting it.
    fn deserialize_historical<'de, D: Deserializer<'de>>(
        version: PayloadVersion,
        deserializer: D,
    ) -> Result<Self::Historical, D::Error>;
    /// Apply adjacent conversions until the current type is reached.
    fn upgrade(value: Self::Historical) -> Result<Self, Self::Error>;
}

/// A serializable value that keeps its identity, version, and data together.
///
/// The current alias's `into_versioned` method wraps a value with its metadata.
/// After deserializing, use the alias's `from_versioned` method to convert to the
/// current type. Typehistory selects the historical type from that metadata.
/// No conversion runs until [`into_current`](Self::into_current) is called.
///
/// The wire representation consists of two single-entry maps. The outer key is
/// the stable identity; the inner key is the version as a decimal string. The
/// inner value is the original historical payload. For example:
/// `{"shop.receipt.created":{"1":{"amount_cents":1200}}}`.
/// This requires a Serde format that supports string-keyed maps and represents
/// payload structs as maps, such as JSON or MessagePack with named fields.
///
/// Deserializing and reserializing this value preserves its historical version.
/// To serialize the latest version, convert to the current type and wrap it again.
/// Metadata has no setters: changing it independently would mislabel the data.
///
/// For an explicitly redacted history, errors returned from this type's
/// `Deserialize` implementation discard the original parser message. Errors
/// raised outside that implementation, such as a format's trailing-input check,
/// remain the format library's responsibility. Serialization contains the real
/// data; redaction does not make stored bytes safe to log.
pub struct Versioned<T: VersionedHistory> {
    historical: T::Historical,
}

impl<T: VersionedHistory> Versioned<T> {
    /// Wrap a current value with its generated identity and version.
    pub fn new(current: T) -> Self {
        Self {
            historical: current.into_historical(),
        }
    }

    /// Return the generated stable identity, checked when deserializing.
    pub fn stable_id(&self) -> TypeIdentity {
        T::STABLE_ID
    }

    /// Return the version of the contained data, before any conversions.
    pub fn source_version(&self) -> PayloadVersion {
        T::source_version(&self.historical)
    }

    /// Apply the declared conversions and return the current type.
    ///
    /// Conversion failures retain the stored version and the failing adjacent
    /// step. Ordinary histories preserve the callback's concrete error source;
    /// redacted histories retain only its redacted projection.
    pub fn into_current(self) -> Result<T, T::Error> {
        T::upgrade(self.historical)
    }
}

impl<T: VersionedHistory> Clone for Versioned<T>
where
    T::Historical: Clone,
{
    fn clone(&self) -> Self {
        Self {
            historical: self.historical.clone(),
        }
    }
}

impl<T: VersionedHistory> Debug for Versioned<T> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        formatter
            .debug_struct("Versioned")
            .field("stable_id", &self.stable_id())
            .field("source_version", &self.source_version())
            .finish_non_exhaustive()
    }
}
