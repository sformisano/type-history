//! A value with a checked stable name, version, and historical data.

mod buffer;
mod serde_impl;

use std::fmt::{Debug, Formatter, Result as FmtResult};

use serde::{de::DeserializeOwned, Deserializer, Serialize};
use serde_json::Error as JsonError;

use crate::{PayloadVersion, StableName};
use buffer::BufferedValue;

/// Decode a generated JSON payload using the same named-record rules as `Versioned`.
#[doc(hidden)]
pub fn decode_json_payload<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, JsonError> {
    let buffered: BufferedValue = serde_json::from_slice(bytes)?;
    T::deserialize(buffered)
}

/// Generated bridge between a current record and its retained historical types.
#[doc(hidden)]
pub trait VersionedHistory: Sized {
    /// Generated enum containing one retained version's data.
    type Historical: Serialize;
    /// Conversion failure for this history.
    type Error;

    /// Frozen stable name shared by every version of this history.
    const STABLE_NAME: StableName;
    /// Version generated for the current alias.
    const CURRENT_VERSION: PayloadVersion;
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

    /// Consume the wrapper for the current alias's generated `from_versioned` method.
    fn from_versioned(value: Versioned<Self>) -> Result<Self, Self::Error> {
        Self::upgrade(value.historical)
    }
}

/// A serializable value that keeps its stable name, version, and data together.
///
/// Suppose `ReceiptCreated` now names V2, but a stored receipt is V1.
/// Deserializing it as `Versioned<ReceiptCreated>` selects and keeps the V1 data.
/// Calling `ReceiptCreated::from_versioned` then runs the conversion to V2.
/// Deserialization itself runs no conversions.
///
/// To write a current receipt, call its `into_versioned` method and serialize
/// the result. Deserializing and reserializing a historical wrapper preserves
/// its historical version. Convert to the current type and wrap it again to emit
/// the latest version. Metadata has no setters: changing it independently would
/// mislabel the data.
///
/// # Stored representation
///
/// The serialized map has three required fields: `stable_name`, the numeric
/// `version`, and the historical `payload`. For example:
/// `{"stable_name":"shop.receipt.created","version":1,"payload":{"amount_cents":1200}}`.
/// Fields may appear in any order. Unknown envelope fields, duplicate fields,
/// mismatched names, and unsupported versions are rejected.
/// The payload is buffered until its metadata has been checked, then decoded
/// into the matching historical type. Supported formats include JSON and
/// MessagePack with named fields; payload structs must be represented as maps.
/// This includes supporting records nested in fields and containers.
/// JSON integers use `serde_json`'s arbitrary-precision representation, which can
/// also interpret that library's reserved numeric-marker objects as integers.
pub struct Versioned<T: VersionedHistory> {
    historical: T::Historical,
}

impl<T: VersionedHistory> Versioned<T> {
    /// Wrap a current value with its generated stable name and version.
    pub fn new(current: T) -> Self {
        Self {
            historical: current.into_historical(),
        }
    }

    /// Return the generated stable name, checked when deserializing.
    pub fn stable_name(&self) -> StableName {
        T::STABLE_NAME
    }

    /// Return the version of the contained data, before any conversions.
    pub fn source_version(&self) -> PayloadVersion {
        T::source_version(&self.historical)
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
            .field("stable_name", &self.stable_name())
            .field("source_version", &self.source_version())
            .finish_non_exhaustive()
    }
}
