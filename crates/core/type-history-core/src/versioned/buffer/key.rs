//! Distinguish actual JSON object keys from serde_json's numeric token.

use serde::{
    de::{Error, Visitor},
    Deserialize, Deserializer,
};
use std::fmt::{Formatter, Result as FmtResult};

pub(super) const NUMBER_TOKEN: &str = "$serde_json::private::Number";

pub(super) enum PayloadKey {
    Name(String),
    Number,
}

impl<'de> Deserialize<'de> for PayloadKey {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        if deserializer.is_human_readable() {
            // serde_json presents an ordinary key as Some(key), including from
            // a Value. Its synthetic arbitrary-precision number key is a string
            // scalar even when an option is requested. Comparing the name alone
            // would mistake a legal object entry for a number.
            deserializer.deserialize_option(KeyVisitor)
        } else {
            String::deserialize(deserializer).map(Self::Name)
        }
    }
}

struct KeyVisitor;

impl<'de> Visitor<'de> for KeyVisitor {
    type Value = PayloadKey;

    fn expecting(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        formatter.write_str("a string key or a JSON number token")
    }

    fn visit_some<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        String::deserialize(deserializer).map(PayloadKey::Name)
    }

    fn visit_str<E: Error>(self, value: &str) -> Result<Self::Value, E> {
        if value == NUMBER_TOKEN {
            Ok(PayloadKey::Number)
        } else {
            Ok(PayloadKey::Name(value.to_owned()))
        }
    }
}
