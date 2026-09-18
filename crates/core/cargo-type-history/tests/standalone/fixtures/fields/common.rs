use serde::{Serialize, Serializer, ser::{SerializeMap, SerializeSeq}};
use serde_json::Value;
use std::fs;

// Serialize ordinary JSON fixture numbers as MessagePack numbers. serde_json's
// arbitrary_precision feature otherwise emits its private numeric marker map.
struct BinaryValue<'a>(&'a Value);

impl Serialize for BinaryValue<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self.0 {
            Value::Number(n) => {
                if let Some(n) = n.as_i64() { serializer.serialize_i64(n) }
                else if let Some(n) = n.as_u64() { serializer.serialize_u64(n) }
                else { serializer.serialize_f64(n.as_f64().unwrap()) }
            }
            Value::Array(values) => {
                let mut sequence = serializer.serialize_seq(Some(values.len()))?;
                for value in values { sequence.serialize_element(&Self(value))?; }
                sequence.end()
            }
            Value::Object(values) => {
                let mut map = serializer.serialize_map(Some(values.len()))?;
                for (key, value) in values { map.serialize_entry(key, &Self(value))?; }
                map.end()
            }
            value => value.serialize(serializer),
        }
    }
}

pub fn messagepack(value: &Value) -> Vec<u8> {
    rmp_serde::to_vec_named(&BinaryValue(value)).unwrap()
}

pub fn payload_first_binary(stable_name: &'static str, value: &Value) -> Vec<u8> {
    rmp_serde::to_vec_named(&PayloadFirst { payload: BinaryValue(value), version: 1, stable_name }).unwrap()
}

#[derive(Serialize)]
pub struct PayloadFirst<T> {
    pub payload: T,
    pub version: u32,
    pub stable_name: &'static str,
}

#[derive(Serialize)]
pub struct MetadataFirst<T> {
    pub stable_name: &'static str,
    pub version: u32,
    pub payload: T,
}

// A first writer leaves actual files; later source/feature variants must read
// those same bytes. Files live inside the outer Fixture's owned TempDir.
pub fn retain<T: Serialize>(stem: &str, value: &T) -> (Vec<u8>, Vec<u8>) {
    let json = format!("{stem}.json");
    let binary = format!("{stem}.msgpack");
    if !fs::exists(&json).unwrap() {
        assert!(!fs::exists(&binary).unwrap());
        fs::write(&json, serde_json::to_vec(value).unwrap()).unwrap();
        fs::write(&binary, rmp_serde::to_vec_named(value).unwrap()).unwrap();
    }
    (fs::read(json).unwrap(), fs::read(binary).unwrap())
}
