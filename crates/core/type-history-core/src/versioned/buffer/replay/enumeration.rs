//! Externally tagged enum replay without losing buffered child values.

use serde::{
    de::{DeserializeSeed, EnumAccess, Error, IntoDeserializer, VariantAccess, Visitor},
    Deserializer,
};
use serde_json::{Error as JsonError, Value};

use super::{BufferedValue, Content};

pub(super) struct BufferedEnum {
    name: String,
    value: Option<BufferedValue>,
}

impl BufferedEnum {
    pub(super) fn new(value: BufferedValue) -> Result<Self, JsonError> {
        match value.value {
            Content::Scalar(Value::String(name)) => Ok(Self { name, value: None }),
            Content::Map(mut fields) if fields.len() == 1 => {
                let (name, value) = fields.pop().expect("one enum entry");
                Ok(Self {
                    name,
                    value: Some(value),
                })
            }
            _ => Err(JsonError::custom(
                "an enum must be a variant string or a map with exactly one entry",
            )),
        }
    }
}

impl<'de> EnumAccess<'de> for BufferedEnum {
    type Error = JsonError;
    type Variant = BufferedVariant;

    fn variant_seed<V: DeserializeSeed<'de>>(
        self,
        seed: V,
    ) -> Result<(V::Value, Self::Variant), Self::Error> {
        let name = seed.deserialize(self.name.into_deserializer())?;
        Ok((name, BufferedVariant(self.value)))
    }
}

pub(super) struct BufferedVariant(Option<BufferedValue>);

impl BufferedVariant {
    fn payload(self) -> Result<BufferedValue, JsonError> {
        self.0
            .ok_or_else(|| JsonError::custom("an enum payload variant requires a one-entry map"))
    }
}

impl<'de> VariantAccess<'de> for BufferedVariant {
    type Error = JsonError;

    fn unit_variant(self) -> Result<(), Self::Error> {
        match self.0 {
            None => Ok(()),
            Some(value) if matches!(value.value, Content::Scalar(Value::Null)) => Ok(()),
            Some(_) => Err(JsonError::custom(
                "a unit enum variant cannot contain a payload",
            )),
        }
    }

    fn newtype_variant_seed<T: DeserializeSeed<'de>>(
        self,
        seed: T,
    ) -> Result<T::Value, Self::Error> {
        seed.deserialize(self.payload()?)
    }

    fn tuple_variant<V: Visitor<'de>>(
        self,
        len: usize,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        let value = self.payload()?;
        if !matches!(&value.value, Content::Sequence(values) if values.len() == len) {
            return Err(JsonError::custom(format!(
                "an enum tuple variant requires exactly {len} values"
            )));
        }
        value.deserialize_tuple(len, visitor)
    }

    fn struct_variant<V: Visitor<'de>>(
        self,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        self.payload()?
            .deserialize_struct("enum variant", fields, visitor)
    }
}
