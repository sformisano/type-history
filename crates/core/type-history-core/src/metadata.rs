//! Validated stable names and positive payload versions.

use std::{
    borrow::Cow,
    fmt::{Display, Formatter, Result as FmtResult},
    num::NonZeroU32,
};

use serde::{de::Error as _, Deserialize, Deserializer, Serialize};

/// Invalid stable name for a type and its history.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum InvalidStableName {
    /// Stable names cannot exceed 255 ASCII bytes.
    #[error("stable name exceeds 255 bytes")]
    TooLong,
    /// One dot-separated segment violates the stable name grammar.
    #[error("stable name segment {segment_number} must start with a lowercase ASCII letter and contain only lowercase ASCII letters, digits, or underscores")]
    InvalidSegment {
        /// One-based position of the invalid segment.
        segment_number: u8,
    },
}

/// Validated stable name shared by every version of a type.
///
/// Stable names contain 1–255 ASCII bytes. Each starts with a lowercase letter,
/// followed by lowercase letters, digits, or underscores. Optional dots separate
/// segments that follow the same rule: `receipt_created` and `shop.receipt` are
/// both valid. Once a history is frozen, the build hook rejects changes to its
/// stable name.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct StableName(Cow<'static, str>);

impl StableName {
    /// Maximum length of one stable name, in ASCII bytes.
    pub const MAX_LEN: usize = 255;

    /// Declare a validated static stable name.
    ///
    /// # Panics
    ///
    /// Panics when `value` violates the [`StableName`] syntax. In a constant
    /// declaration, invalid syntax fails compilation.
    pub const fn new(value: &'static str) -> Self {
        match validate_stable_name(value) {
            Ok(()) => Self(Cow::Borrowed(value)),
            Err(InvalidStableName::TooLong) => {
                panic!("stable name exceeds 255 bytes")
            }
            Err(InvalidStableName::InvalidSegment { .. }) => {
                panic!("stable name contains an invalid segment")
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

impl TryFrom<String> for StableName {
    type Error = InvalidStableName;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        validate_stable_name(&value)?;
        Ok(Self(Cow::Owned(value)))
    }
}

impl<'de> Deserialize<'de> for StableName {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::try_from(String::deserialize(deserializer)?).map_err(D::Error::custom)
    }
}

impl Display for StableName {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        formatter.write_str(self.as_str())
    }
}

impl AsRef<str> for StableName {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

const fn validate_stable_name(value: &str) -> Result<(), InvalidStableName> {
    let bytes = value.as_bytes();
    if bytes.len() > StableName::MAX_LEN {
        return Err(InvalidStableName::TooLong);
    }

    let mut segment_number = 1_u8;
    let mut segment_start = 0;
    let mut index = 0;
    while index <= bytes.len() {
        if index == bytes.len() || bytes[index] == b'.' {
            if index == segment_start || !is_ascii_lowercase(bytes[segment_start]) {
                return Err(InvalidStableName::InvalidSegment { segment_number });
            }
            let mut segment_index = segment_start + 1;
            while segment_index < index {
                let byte = bytes[segment_index];
                if !is_ascii_lowercase(byte) && !byte.is_ascii_digit() && byte != b'_' {
                    return Err(InvalidStableName::InvalidSegment { segment_number });
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
