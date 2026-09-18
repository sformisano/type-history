//! Shared implementations keep each checked wrapper's schema and encoding together.

#[cfg(any(
    feature = "uuid",
    feature = "rust-decimal",
    feature = "chrono",
    feature = "time",
    feature = "typed-floats"
))]
macro_rules! profile_traits {
    ($rust:ty, $profile:ident) => {
        impl crate::resolved::ResolvedSchema for $rust {
            type Wire = crate::resolved::Profile<crate::resolved::profile::$profile>;
        }
        impl crate::resolved::JsonSchemaField for $rust {
            fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
                crate::resolved::profile_schema(crate::resolved::StorageProfile::$profile)
            }
        }
        impl crate::resolved::SetMembership for $rust {
            const MEMBERSHIP: crate::resolved::ConstantMembership =
                crate::resolved::StorageProfile::$profile.membership();
        }
    };
}

#[cfg(any(
    feature = "uuid",
    feature = "rust-decimal",
    feature = "chrono",
    feature = "time"
))]
macro_rules! checked_wrapper {
    ($(#[$meta:meta])* $name:ident, $native:ty, $profile:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name($native);

        impl $name {
            /// Borrow the validated native value. Mutable access is intentionally absent.
            pub fn as_inner(&self) -> &$native { &self.0 }

            /// Recover the native value without changing its components.
            pub fn into_inner(self) -> $native { self.0 }
        }

        impl TryFrom<$native> for $name {
            type Error = crate::resolved::ProfileError;
            fn try_from(value: $native) -> Result<Self, Self::Error> {
                Self::validate(&value)?;
                Ok(Self(value))
            }
        }

        impl serde::Serialize for $name {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_str(&self.to_text())
            }
        }

        impl<'de> serde::Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let text = <String as serde::Deserialize>::deserialize(deserializer)?;
                Self::from_text(&text).map_err(serde::de::Error::custom)
            }
        }
        profile_traits!($name, $profile);
    };
}
