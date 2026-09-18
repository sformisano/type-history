//! Replay buffered data through Serde's public value deserializers.

use serde::{
    de::{
        value::{MapDeserializer, SeqDeserializer},
        Error, IntoDeserializer, Visitor,
    },
    Deserialize, Deserializer,
};
use serde_json::{Error as JsonError, Number, Value};

use super::{BufferedValue, Content};

mod enumeration;
use enumeration::BufferedEnum;

impl<'de> IntoDeserializer<'de, JsonError> for BufferedValue {
    type Deserializer = Self;

    fn into_deserializer(self) -> Self {
        self
    }
}

macro_rules! deserialize_integer {
    ($method:ident) => {
        fn $method<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
            match self.value {
                Content::Scalar(value) => value.$method(visitor),
                Content::Map(fields) if self.human_readable => number(fields)?.$method(visitor),
                _ => self.deserialize_any(visitor),
            }
        }
    };
}

impl<'de> Deserializer<'de> for BufferedValue {
    type Error = JsonError;

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        match self.value {
            Content::Scalar(value) => value.deserialize_any(visitor),
            Content::Float(value) => visitor.visit_f64(value),
            Content::Bytes(value) => visitor.visit_byte_buf(value),
            Content::Sequence(values) => {
                SeqDeserializer::new(values.into_iter()).deserialize_any(visitor)
            }
            Content::Map(fields) => {
                MapDeserializer::new(fields.into_iter()).deserialize_any(visitor)
            }
        }
    }

    deserialize_integer!(deserialize_i8);
    deserialize_integer!(deserialize_i16);
    deserialize_integer!(deserialize_i32);
    deserialize_integer!(deserialize_i64);
    deserialize_integer!(deserialize_u8);
    deserialize_integer!(deserialize_u16);
    deserialize_integer!(deserialize_u32);
    deserialize_integer!(deserialize_u64);

    fn deserialize_f32<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        match self.value {
            Content::Scalar(value) => value.deserialize_f32(visitor),
            // Preserve JSON text until the requested width is known. Parsing
            // through f64 first can double-round an otherwise exact f32 input.
            Content::Map(fields) if self.human_readable => number(fields)?.deserialize_f32(visitor),
            Content::Float(value) => visitor.visit_f32(value as f32),
            _ => self.deserialize_any(visitor),
        }
    }

    fn deserialize_f64<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        match self.value {
            Content::Scalar(value) => value.deserialize_f64(visitor),
            Content::Map(fields) if self.human_readable => number(fields)?.deserialize_f64(visitor),
            Content::Float(value) => visitor.visit_f64(value),
            _ => self.deserialize_any(visitor),
        }
    }

    fn deserialize_i128<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        match self.value {
            Content::Scalar(value) => value.deserialize_i128(visitor),
            Content::Map(fields) if self.human_readable => {
                number(fields)?.deserialize_i128(visitor)
            }
            // MessagePack represents 128-bit integers as 16 big-endian bytes.
            Content::Bytes(bytes) => {
                let bytes = bytes.try_into().map_err(|_| {
                    JsonError::custom("a binary i128 payload must contain exactly 16 bytes")
                })?;
                visitor.visit_i128(i128::from_be_bytes(bytes))
            }
            _ => self.deserialize_any(visitor),
        }
    }

    fn deserialize_u128<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        match self.value {
            Content::Scalar(value) => value.deserialize_u128(visitor),
            Content::Map(fields) if self.human_readable => {
                number(fields)?.deserialize_u128(visitor)
            }
            Content::Bytes(bytes) => {
                let bytes = bytes.try_into().map_err(|_| {
                    JsonError::custom("a binary u128 payload must contain exactly 16 bytes")
                })?;
                visitor.visit_u128(u128::from_be_bytes(bytes))
            }
            _ => self.deserialize_any(visitor),
        }
    }

    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        if matches!(self.value, Content::Scalar(Value::Null)) {
            visitor.visit_none()
        } else {
            visitor.visit_some(self)
        }
    }

    fn deserialize_seq<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        match self.value {
            // MessagePack binary values can also represent a sequence of bytes.
            Content::Bytes(bytes) => {
                SeqDeserializer::new(bytes.into_iter()).deserialize_any(visitor)
            }
            _ => self.deserialize_any(visitor),
        }
    }

    fn deserialize_tuple<V: Visitor<'de>>(
        self,
        _len: usize,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        self.deserialize_seq(visitor)
    }

    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        visitor.visit_newtype_struct(self)
    }

    fn is_human_readable(&self) -> bool {
        self.human_readable
    }

    fn deserialize_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        if !matches!(self.value, Content::Map(_)) {
            return Err(JsonError::custom("a history record must use named fields"));
        }
        self.deserialize_any(visitor)
    }

    fn deserialize_enum<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        visitor.visit_enum(BufferedEnum::new(self)?)
    }

    serde::forward_to_deserialize_any! {
        bool char str string bytes byte_buf unit unit_struct
        tuple_struct map identifier ignored_any
    }
}

fn number(fields: Vec<(String, BufferedValue)>) -> Result<Number, JsonError> {
    if fields.len() != 1 {
        return Err(JsonError::custom("expected a numeric payload value"));
    }
    // JSON's arbitrary-precision numbers use Serde's map data model. Defer
    // recognition to Number's public API only when a numeric field is requested.
    Number::deserialize(MapDeserializer::new(fields.into_iter()))
}
