//! Validated logical identities and positive payload versions.

use std::{
    borrow::Cow,
    fmt::{Display, Formatter, Result as FmtResult},
    num::NonZeroU32,
};

use serde::{de::Error as _, Deserialize, Deserializer, Serialize};

/// Invalid durable identity for one consumer-owned record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum InvalidTypeIdentity {
    /// Record types cannot exceed 255 ASCII bytes.
    #[error("event payload type exceeds 255 bytes")]
    TooLong,
    /// Record types contain fewer than three canonical segments.
    #[error("event payload type must contain at least three segments")]
    InvalidSegmentCount,
    /// One dot-separated segment violates the record type grammar.
    #[error("event payload type segment {segment_number} is invalid")]
    InvalidSegment {
        /// One-based position of the invalid segment.
        segment_number: u8,
    },
}

/// Validated durable identity of one consumer-owned record.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct TypeIdentity(Cow<'static, str>);

impl TypeIdentity {
    /// Maximum length of one canonical record type, in ASCII bytes.
    pub const MAX_LEN: usize = 255;

    /// Declare a validated static record type.
    ///
    /// # Panics
    ///
    /// Panics during constant evaluation when `value` is not canonical.
    pub const fn new(value: &'static str) -> Self {
        match validate_type_identity(value) {
            Ok(()) => Self(Cow::Borrowed(value)),
            Err(InvalidTypeIdentity::TooLong) => {
                panic!("event payload type exceeds 255 bytes")
            }
            Err(InvalidTypeIdentity::InvalidSegmentCount) => {
                panic!("event payload type must contain at least three segments")
            }
            Err(InvalidTypeIdentity::InvalidSegment { .. }) => {
                panic!("event payload type contains an invalid segment")
            }
        }
    }

    /// Borrow the canonical string representation.
    pub fn as_str(&self) -> &str {
        self.0.as_ref()
    }

    /// Borrow the static declaration, when this value was declared statically.
    pub const fn as_static_str(&self) -> Option<&'static str> {
        match &self.0 {
            Cow::Borrowed(value) => Some(*value),
            Cow::Owned(_) => None,
        }
    }
}

impl TryFrom<String> for TypeIdentity {
    type Error = InvalidTypeIdentity;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        validate_type_identity(&value)?;
        Ok(Self(Cow::Owned(value)))
    }
}

impl<'de> Deserialize<'de> for TypeIdentity {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::try_from(String::deserialize(deserializer)?).map_err(D::Error::custom)
    }
}

impl Display for TypeIdentity {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        formatter.write_str(self.as_str())
    }
}

impl AsRef<str> for TypeIdentity {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

const fn validate_type_identity(value: &str) -> Result<(), InvalidTypeIdentity> {
    let bytes = value.as_bytes();
    if bytes.len() > TypeIdentity::MAX_LEN {
        return Err(InvalidTypeIdentity::TooLong);
    }

    let mut dot_count = 0;
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'.' {
            dot_count += 1;
        }
        index += 1;
    }
    if dot_count < 2 {
        return Err(InvalidTypeIdentity::InvalidSegmentCount);
    }

    let mut segment_number = 1_u8;
    let mut segment_start = 0;
    index = 0;
    while index <= bytes.len() {
        if index == bytes.len() || bytes[index] == b'.' {
            if index == segment_start || !is_ascii_lowercase(bytes[segment_start]) {
                return Err(InvalidTypeIdentity::InvalidSegment { segment_number });
            }
            let mut segment_index = segment_start + 1;
            while segment_index < index {
                let byte = bytes[segment_index];
                if !is_ascii_lowercase(byte) && !byte.is_ascii_digit() && byte != b'_' {
                    return Err(InvalidTypeIdentity::InvalidSegment { segment_number });
                }
                segment_index += 1;
            }
            segment_number += 1;
            segment_start = index + 1;
        }
        index += 1;
    }
    Ok(())
}

const fn is_ascii_lowercase(byte: u8) -> bool {
    byte >= b'a' && byte <= b'z'
}

/// Positive schema version of one durable record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PayloadVersion(NonZeroU32);

impl PayloadVersion {
    /// Smallest valid payload schema version.
    pub const INITIAL: Self = Self(NonZeroU32::MIN);

    /// Construct a schema version from an already-positive value.
    pub const fn new(value: NonZeroU32) -> Self {
        Self(value)
    }

    /// Parse a raw schema version from storage or transport.
    pub const fn try_from_raw(value: u32) -> Result<Self, InvalidPayloadVersion> {
        match NonZeroU32::new(value) {
            Some(value) => Ok(Self(value)),
            None => Err(InvalidPayloadVersion { value }),
        }
    }

    /// Return the positive raw version.
    pub const fn get(self) -> u32 {
        self.0.get()
    }
}

impl TryFrom<u32> for PayloadVersion {
    type Error = InvalidPayloadVersion;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        Self::try_from_raw(value)
    }
}

impl From<NonZeroU32> for PayloadVersion {
    fn from(value: NonZeroU32) -> Self {
        Self::new(value)
    }
}

impl From<PayloadVersion> for u32 {
    fn from(value: PayloadVersion) -> Self {
        value.get()
    }
}

impl Display for PayloadVersion {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        self.0.fmt(formatter)
    }
}

/// Error returned when a raw payload schema version is zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("event payload schema version must be positive, got {value}")]
pub struct InvalidPayloadVersion {
    /// Rejected raw version.
    pub value: u32,
}

#[cfg(test)]
mod tests;
