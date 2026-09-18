//! Stable storage profiles shared by schema producers, readers, and adapters.

use super::{membership::rules, ConstantMembership};
use schemars::Schema;
use serde::de::Error as DeserializeError;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::error::Error;
use std::fmt::{Display, Formatter, Result as FmtResult};

/// A fixed encoding and admitted value domain, independent of dependency versions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageProfile {
    Finite32,
    Finite64,
    UuidText,
    DecimalText,
    Date,
    LocalTime,
    LocalDateTime,
    UtcInstant,
    OffsetDateTime,
}

macro_rules! profiles {
    ($($variant:ident => $id:literal),+ $(,)?) => {
        impl StorageProfile {
            /// Immutable identity of this encoding and domain.
            pub const fn id(self) -> &'static str {
                match self { $(Self::$variant => $id,)+ }
            }

            /// Resolve a known profile; unknown domains are never treated as strings.
            pub fn from_id(id: &str) -> Result<Self, ProfileError> {
                match id {
                    $($id => Ok(Self::$variant),)+
                    _ => Err(ProfileError::new(id, "unknown storage profile")),
                }
            }
        }

        $(
            /// Marker for the corresponding immutable storage profile.
            #[doc(hidden)]
            pub struct $variant;
            impl ProfileMarker for $variant {
                const PROFILE: StorageProfile = StorageProfile::$variant;
            }
        )+
    };
}

profiles! {
    Finite32 => "type-history:finite32:v1",
    Finite64 => "type-history:finite64:v1",
    UuidText => "type-history:uuid-text:v1",
    DecimalText => "type-history:decimal96-text:v1",
    Date => "type-history:date:v1",
    LocalTime => "type-history:local-time:v1",
    LocalDateTime => "type-history:local-datetime:v1",
    UtcInstant => "type-history:utc-instant:v1",
    OffsetDateTime => "type-history:offset-datetime:v1",
}

impl StorageProfile {
    /// JSON Schema primitive representation of this profile.
    pub const fn json_type(self) -> &'static str {
        match self {
            Self::Finite32 | Self::Finite64 => "number",
            _ => "string",
        }
    }

    /// Equality used by supported profile values when stored as set members.
    /// Decimal scale and offset spelling remain stored even when equality ignores them.
    pub const fn membership(self) -> ConstantMembership {
        match self {
            Self::Finite32 => rules::FINITE32,
            Self::Finite64 => rules::FINITE64,
            Self::UuidText => rules::UUID_BITS,
            Self::DecimalText => rules::DECIMAL_NUMERIC,
            Self::Date => rules::DATE,
            Self::LocalTime => rules::LOCAL_TIME,
            Self::LocalDateTime => rules::LOCAL_DATETIME,
            Self::UtcInstant | Self::OffsetDateTime => rules::UTC_INSTANT,
        }
    }

    /// Exact comparison usable by generated constant assertions.
    pub const fn same(self, other: Self) -> bool {
        self as u8 == other as u8
    }
}

impl Serialize for StorageProfile {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.id())
    }
}

impl<'de> Deserialize<'de> for StorageProfile {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let id = String::deserialize(deserializer)?;
        Self::from_id(&id).map_err(D::Error::custom)
    }
}

/// Declares a profile for a type-level schema node.
#[doc(hidden)]
pub trait ProfileMarker {
    const PROFILE: StorageProfile;
}

/// A value or profile identifier lies outside its declared storage contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileError {
    profile: String,
    reason: String,
}

impl ProfileError {
    /// Describe the rejected profile and the failed check.
    pub fn new(profile: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            profile: profile.into(),
            reason: reason.into(),
        }
    }

    /// Stable profile identifier associated with the failure.
    pub fn profile(&self) -> &str {
        &self.profile
    }

    /// The failed check.
    pub fn reason(&self) -> &str {
        &self.reason
    }
}

impl Display for ProfileError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        write!(formatter, "{}: {}", self.profile, self.reason)
    }
}

impl Error for ProfileError {}

/// Produce the shared JSON Schema representation of a storage profile.
#[doc(hidden)]
pub fn profile_schema(profile: StorageProfile) -> Schema {
    schemars::json_schema!({
        "type": profile.json_type(),
        "x-type-history-profile": profile.id(),
    })
}

#[cfg(test)]
mod tests {
    use super::{profile_schema, StorageProfile};

    #[test]
    fn every_profile_round_trips_its_exact_id_and_primitive_representation() {
        for profile in [
            StorageProfile::Finite32,
            StorageProfile::Finite64,
            StorageProfile::UuidText,
            StorageProfile::DecimalText,
            StorageProfile::Date,
            StorageProfile::LocalTime,
            StorageProfile::LocalDateTime,
            StorageProfile::UtcInstant,
            StorageProfile::OffsetDateTime,
        ] {
            assert_eq!(StorageProfile::from_id(profile.id()).unwrap(), profile);
            let value = serde_json::to_value(profile).unwrap();
            assert_eq!(value, profile.id());
            assert_eq!(
                serde_json::from_value::<StorageProfile>(value).unwrap(),
                profile
            );
            let schema = profile_schema(profile).to_value();
            assert_eq!(schema["x-type-history-profile"], profile.id());
            assert_eq!(schema["type"], profile.json_type());
            assert!(StorageProfile::from_id(&format!("{}:different-range", profile.id())).is_err());
        }
        assert!(!StorageProfile::Finite32.same(StorageProfile::Finite64));
        assert!(StorageProfile::from_id("type-history:utc-seconds:v1").is_err());
        assert!(serde_json::from_str::<StorageProfile>("\"unknown\"").is_err());
        assert!(StorageProfile::UtcInstant
            .membership()
            .same(&StorageProfile::OffsetDateTime.membership()));
    }
}
