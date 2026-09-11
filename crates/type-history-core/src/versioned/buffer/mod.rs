//! Retain a payload until its envelope has supplied and validated the version.

mod replay;
#[cfg(test)]
mod tests;

use std::{
    collections::BTreeSet,
    fmt::{Formatter, Result as FmtResult},
};

use serde::{
    de::{Error, MapAccess, SeqAccess, Visitor},
    Deserialize, Deserializer,
};
use serde_json::{Number, Value};

pub(super) struct BufferedValue {
    value: Content,
    human_readable: bool,
}

enum Content {
    Scalar(Value),
    Float(f64),
    Bytes(Vec<u8>),
    Sequence(Vec<BufferedValue>),
    Map(Vec<(String, BufferedValue)>),
}

impl<'de> Deserialize<'de> for BufferedValue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let human_readable = deserializer.is_human_readable();
        let value = deserializer.deserialize_any(ContentVisitor)?;
        Ok(Self {
            value,
            human_readable,
        })
    }
}

struct ContentVisitor;

impl<'de> Visitor<'de> for ContentVisitor {
    type Value = Content;

    fn expecting(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        formatter.write_str("a historical payload value")
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E> {
        Ok(Content::Scalar(Value::Null))
    }

    fn visit_none<E: Error>(self) -> Result<Self::Value, E> {
        self.visit_unit()
    }

    fn visit_some<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_any(self)
    }

    fn visit_newtype_struct<D: Deserializer<'de>>(
        self,
        deserializer: D,
    ) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_any(self)
    }

    fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E> {
        Ok(Content::Scalar(Value::Bool(value)))
    }

    fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E> {
        Ok(Content::Scalar(Value::Number(value.into())))
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E> {
        Ok(Content::Scalar(Value::Number(value.into())))
    }

    fn visit_i128<E: Error>(self, value: i128) -> Result<Self::Value, E> {
        Number::from_i128(value)
            .map(|number| Content::Scalar(Value::Number(number)))
            .ok_or_else(|| E::custom("integer exceeds the payload buffer's range"))
    }

    fn visit_u128<E: Error>(self, value: u128) -> Result<Self::Value, E> {
        Number::from_u128(value)
            .map(|number| Content::Scalar(Value::Number(number)))
            .ok_or_else(|| E::custom("integer exceeds the payload buffer's range"))
    }

    fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E> {
        Ok(Content::Float(value))
    }

    fn visit_str<E: Error>(self, value: &str) -> Result<Self::Value, E> {
        self.visit_string(value.to_owned())
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E> {
        Ok(Content::Scalar(Value::String(value)))
    }

    fn visit_bytes<E: Error>(self, value: &[u8]) -> Result<Self::Value, E> {
        self.visit_byte_buf(value.to_owned())
    }

    fn visit_byte_buf<E>(self, value: Vec<u8>) -> Result<Self::Value, E> {
        Ok(Content::Bytes(value))
    }

    fn visit_seq<S: SeqAccess<'de>>(self, mut sequence: S) -> Result<Self::Value, S::Error> {
        let mut values = Vec::new();
        while let Some(value) = sequence.next_element()? {
            values.push(value);
        }
        Ok(Content::Sequence(values))
    }

    fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
        let mut fields = Vec::new();
        let mut names = BTreeSet::new();
        while let Some(name) = map.next_key::<String>()? {
            if !names.insert(name.clone()) {
                return Err(M::Error::custom("duplicate field in historical payload"));
            }
            fields.push((name, map.next_value()?));
        }
        Ok(Content::Map(fields))
    }
}
