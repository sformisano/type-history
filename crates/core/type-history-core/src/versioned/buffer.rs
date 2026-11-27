//! Retain a payload until its envelope has supplied and validated the version.

#[cfg(all(test, feature = "typed-floats"))]
mod float_tests;
mod key;
mod replay;
#[cfg(test)]
mod tests;

use std::{
    collections::BTreeSet,
    fmt::{Formatter, Result as FmtResult},
};

use serde::{
    de::{DeserializeSeed, Error, MapAccess, SeqAccess, Visitor},
    Deserialize, Deserializer,
};
use serde_json::{value::RawValue, Number, Value};

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

impl BufferedValue {
    pub(super) fn from_json(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        let mut parser = serde_json::Deserializer::from_slice(bytes);
        let value = BufferSource::JsonText.deserialize(&mut parser)?;
        parser.end()?;
        Ok(value)
    }
}

impl<'de> Deserialize<'de> for BufferedValue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        BufferSource::Serde.deserialize(deserializer)
    }
}

// A generic Serde caller may already have converted the input to untyped
// values, erasing the distinction between a number and a marker-shaped map.
#[derive(Clone, Copy)]
enum BufferSource {
    Serde,
    JsonText,
}

impl<'de> DeserializeSeed<'de> for BufferSource {
    type Value = BufferedValue;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<BufferedValue, D::Error> {
        let human_readable = deserializer.is_human_readable();
        let value = deserializer.deserialize_any(ContentVisitor {
            source: self,
            human_readable,
        })?;
        Ok(BufferedValue {
            value,
            human_readable,
        })
    }
}

struct ContentVisitor {
    source: BufferSource,
    human_readable: bool,
}

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
        while let Some(value) = sequence.next_element_seed(self.source)? {
            values.push(value);
        }
        Ok(Content::Sequence(values))
    }

    fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
        let mut fields = Vec::new();
        let mut names = BTreeSet::new();
        while let Some(key) = map.next_key::<key::PayloadKey>()? {
            let name = match key {
                key::PayloadKey::Name(name) => name,
                key::PayloadKey::Number => {
                    let text = map.next_value::<String>()?;
                    let number = text.parse::<Number>().map_err(M::Error::custom)?;
                    return Ok(Content::Scalar(Value::Number(number)));
                }
            };
            if !names.insert(name.clone()) {
                return Err(M::Error::custom("duplicate field in historical payload"));
            }
            let value = if fields.is_empty()
                && self.human_readable
                && matches!(self.source, BufferSource::Serde)
                && name == key::NUMBER_TOKEN
            {
                // Raw JSON and serde_json::Value can supply this entry exactly.
                // An untagged Serde enum can only supply a prebuffered string:
                // accepting that string would turn a decimal into a real map.
                let raw = map.next_value::<Box<RawValue>>()?;
                BufferedValue::from_json(raw.get().as_bytes()).map_err(M::Error::custom)?
            } else {
                map.next_value_seed(self.source)?
            };
            fields.push((name, value));
        }
        Ok(Content::Map(fields))
    }
}
