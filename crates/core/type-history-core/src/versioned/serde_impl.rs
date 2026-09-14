//! Validate the named envelope before decoding its historical payload.

use std::{
    fmt::{Formatter, Result as FmtResult},
    marker::PhantomData,
};

use serde::{
    de::{Error, MapAccess, Visitor},
    ser::SerializeMap,
    Deserialize, Deserializer, Serialize, Serializer,
};

use super::{buffer::BufferedValue, Versioned, VersionedHistory};
use crate::PayloadVersion;

const FIELDS: &[&str] = &["stable_name", "version", "payload"];

impl<T: VersionedHistory> Serialize for Versioned<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(3))?;
        map.serialize_entry("stable_name", T::STABLE_NAME.as_str())?;
        map.serialize_entry("version", &self.source_version().get())?;
        map.serialize_entry("payload", &self.historical)?;
        map.end()
    }
}

impl<'de, T: VersionedHistory> Deserialize<'de> for Versioned<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_map(RecordVisitor(PhantomData))
    }
}

struct RecordVisitor<T>(PhantomData<T>);

impl<'de, T: VersionedHistory> Visitor<'de> for RecordVisitor<T> {
    type Value = Versioned<T>;

    fn expecting(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        formatter.write_str("a map with stable_name, version, and payload fields")
    }

    fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
        let mut stable_name = None;
        let mut version = None;
        let mut payload = None;

        while let Some(field) = map.next_key::<String>()? {
            match field.as_str() {
                "stable_name" => {
                    if stable_name.is_some() {
                        return Err(M::Error::duplicate_field("stable_name"));
                    }
                    stable_name = Some(map.next_value::<String>()?);
                }
                "version" => {
                    if version.is_some() {
                        return Err(M::Error::duplicate_field("version"));
                    }
                    let number = map.next_value::<u32>()?;
                    version = Some(PayloadVersion::try_from(number).map_err(M::Error::custom)?);
                }
                "payload" => {
                    if payload.is_some() {
                        return Err(M::Error::duplicate_field("payload"));
                    }
                    payload = Some(map.next_value::<BufferedValue>()?);
                }
                _ => return Err(M::Error::unknown_field(&field, FIELDS)),
            }
        }

        let stable_name = stable_name.ok_or_else(|| M::Error::missing_field("stable_name"))?;
        let version = version.ok_or_else(|| M::Error::missing_field("version"))?;
        let payload = payload.ok_or_else(|| M::Error::missing_field("payload"))?;
        if stable_name != T::STABLE_NAME.as_str() {
            return Err(M::Error::custom(
                "versioned value stable name does not match the requested type",
            ));
        }

        let historical = T::deserialize_historical(version, payload).map_err(M::Error::custom)?;
        Ok(Versioned { historical })
    }
}
