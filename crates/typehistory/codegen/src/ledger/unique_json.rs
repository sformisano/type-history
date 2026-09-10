//! Reject duplicate object keys before schema decoding can discard them.

use serde::{
    de::{Error, MapAccess, SeqAccess, Visitor},
    Deserialize, Deserializer,
};
use serde_json::Error as JsonError;
use std::{
    collections::BTreeSet,
    fmt::{Formatter, Result as FmtResult},
};

pub(super) fn check(bytes: &[u8]) -> Result<(), JsonError> {
    serde_json::from_slice::<UniqueJson>(bytes).map(|_| ())
}

struct UniqueJson;

impl<'de> Deserialize<'de> for UniqueJson {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(UniqueVisitor)
    }
}

struct UniqueVisitor;

impl<'de> Visitor<'de> for UniqueVisitor {
    type Value = UniqueJson;

    fn expecting(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        formatter.write_str("JSON without duplicate object keys")
    }
    fn visit_bool<E: Error>(self, _: bool) -> Result<UniqueJson, E> {
        Ok(UniqueJson)
    }
    fn visit_i64<E: Error>(self, _: i64) -> Result<UniqueJson, E> {
        Ok(UniqueJson)
    }
    fn visit_u64<E: Error>(self, _: u64) -> Result<UniqueJson, E> {
        Ok(UniqueJson)
    }
    fn visit_f64<E: Error>(self, _: f64) -> Result<UniqueJson, E> {
        Ok(UniqueJson)
    }
    fn visit_str<E: Error>(self, _: &str) -> Result<UniqueJson, E> {
        Ok(UniqueJson)
    }
    fn visit_unit<E: Error>(self) -> Result<UniqueJson, E> {
        Ok(UniqueJson)
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut access: A) -> Result<UniqueJson, A::Error> {
        while access.next_element::<UniqueJson>()?.is_some() {}
        Ok(UniqueJson)
    }
    fn visit_map<A: MapAccess<'de>>(self, mut access: A) -> Result<UniqueJson, A::Error> {
        let mut names = BTreeSet::new();
        while let Some(name) = access.next_key::<String>()? {
            if !names.insert(name.clone()) {
                return Err(A::Error::custom(format!("duplicate JSON key `{name}`")));
            }
            access.next_value::<UniqueJson>()?;
        }
        Ok(UniqueJson)
    }
}
