//! Stream metadata before decoding a typed payload; never buffer generic values.

use std::{
    fmt::{Formatter, Result as FmtResult},
    marker::PhantomData,
};

use serde::{
    de::{DeserializeSeed, Error, IgnoredAny, MapAccess, Visitor},
    ser::SerializeMap,
    Deserialize, Deserializer, Serialize, Serializer,
};

use super::{Versioned, VersionedHistory};
use crate::PayloadVersion;

impl<T: VersionedHistory> Serialize for Versioned<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(1))?;
        map.serialize_entry(T::STABLE_ID.as_str(), &PayloadWithVersion(self))?;
        map.end()
    }
}

struct PayloadWithVersion<'a, T: VersionedHistory>(&'a Versioned<T>);

impl<T: VersionedHistory> Serialize for PayloadWithVersion<'_, T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(1))?;
        map.serialize_entry(
            &self.0.source_version().get().to_string(),
            &self.0.historical,
        )?;
        map.end()
    }
}

impl<'de, T: VersionedHistory> Deserialize<'de> for Versioned<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer
            .deserialize_map(RecordVisitor(PhantomData))
            .map_err(|error| {
                if T::REDACTED {
                    // Do not format the discarded error: it may contain payload data.
                    D::Error::custom("invalid redacted versioned value")
                } else {
                    error
                }
            })
    }
}

struct RecordVisitor<T>(PhantomData<T>);

impl<'de, T: VersionedHistory> Visitor<'de> for RecordVisitor<T> {
    type Value = Versioned<T>;

    fn expecting(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        formatter.write_str("a map containing one stable identity and its versioned payload")
    }

    fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
        let identity = map
            .next_key::<String>()?
            .ok_or_else(|| M::Error::custom("missing stable identity"))?;
        if identity != T::STABLE_ID.as_str() {
            return Err(M::Error::custom(
                "versioned value stable identity does not match the requested type",
            ));
        }
        let historical = map.next_value_seed(VersionSeed::<T>(PhantomData))?;
        require_end(&mut map)?;
        Ok(Versioned { historical })
    }
}

struct VersionSeed<T>(PhantomData<T>);

impl<'de, T: VersionedHistory> DeserializeSeed<'de> for VersionSeed<T> {
    type Value = T::Historical;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_map(self)
    }
}

impl<'de, T: VersionedHistory> Visitor<'de> for VersionSeed<T> {
    type Value = T::Historical;

    fn expecting(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        formatter.write_str("a map containing one version and its payload")
    }

    fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
        let raw = map
            .next_key::<String>()?
            .ok_or_else(|| M::Error::custom("missing payload version"))?;
        let number = raw
            .parse::<u32>()
            .map_err(|_| M::Error::custom("invalid payload version"))?;
        if number.to_string() != raw {
            return Err(M::Error::custom(
                "payload version must be a canonical decimal string",
            ));
        }
        let version = PayloadVersion::try_from(number).map_err(M::Error::custom)?;
        let historical = map.next_value_seed(PayloadSeed::<T> {
            version,
            marker: PhantomData,
        })?;
        require_end(&mut map)?;
        Ok(historical)
    }
}

struct PayloadSeed<T> {
    version: PayloadVersion,
    marker: PhantomData<T>,
}

impl<'de, T: VersionedHistory> DeserializeSeed<'de> for PayloadSeed<T> {
    type Value = T::Historical;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        T::deserialize_historical(self.version, deserializer)
    }
}

fn require_end<'de, M: MapAccess<'de>>(map: &mut M) -> Result<(), M::Error> {
    if map.next_key::<IgnoredAny>()?.is_some() {
        return Err(M::Error::custom(
            "versioned value maps must contain exactly one entry",
        ));
    }
    Ok(())
}
