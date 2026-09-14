use history_api::{Schema, versioned};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Schema)]
#[serde(deny_unknown_fields)]
pub struct Details {
    pub amount: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Schema)]
pub enum Choice {
    First,
    Second,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Schema)]
#[serde(deny_unknown_fields)]
pub enum Event {
    Idle,
    Count(u32),
    Pair(u32, String),
    Named { amount: u32, note: Option<String> },
    Record(Details),
    Bytes(Vec<u8>),
    Wide(i128, u128),
    Nested(Choice),
    Empty {},
}

#[versioned(stable_name = "billing.enum.events")]
pub struct Events {
    pub direct: Event,
    pub optional: Option<Event>,
    pub list: Vec<Event>,
    pub fixed: [Event; 1],
}

#[cfg(test)]
mod tests {
    use super::{Choice, Details, Event, Events};
    use history_api::Versioned;
    use serde::{
        Serialize, Serializer,
        ser::{SerializeMap, SerializeSeq},
    };
    use serde_json::{Value, json};

    #[test]
    fn every_variant_and_container_round_trips_both_codecs() {
        for event in [
            Event::Idle,
            Event::Count(17),
            Event::Pair(18, "tuple".into()),
            Event::Named {
                amount: 19,
                note: Some("named".into()),
            },
            Event::Record(Details { amount: 20 }),
            Event::Bytes(vec![0, 128, 255]),
            Event::Wide(i128::MIN, u128::MAX),
            Event::Nested(Choice::Second),
            Event::Empty {},
        ] {
            let expected = Events {
                direct: event.clone(),
                optional: Some(event.clone()),
                list: vec![event.clone()],
                fixed: [event],
            };
            let stored = expected.clone().into_versioned();
            let json = serde_json::to_vec(&stored).unwrap();
            let actual: Versioned<Events> = serde_json::from_slice(&json).unwrap();
            assert_eq!(Events::from_versioned(actual).unwrap(), expected);
            let binary = rmp_serde::to_vec_named(&stored).unwrap();
            let actual: Versioned<Events> = rmp_serde::from_slice(&binary).unwrap();
            assert_eq!(Events::from_versioned(actual).unwrap(), expected);
        }
        let expected = Events {
            direct: Event::Idle,
            optional: None,
            list: vec![],
            fixed: [Event::Idle],
        };
        let bytes = serde_json::to_vec(&expected.clone().into_versioned()).unwrap();
        let decoded = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(Events::from_versioned(decoded).unwrap(), expected);
    }

    fn envelope(event: Value) -> Value {
        json!({"stable_name":"billing.enum.events", "version":1, "payload": {
            "direct":event, "optional":null, "list":[], "fixed":["Idle"]
        }})
    }

    // JSON's arbitrary-precision Number uses a private Serde map. These small
    // fixture integers must use MessagePack's integer model, including version.
    struct WireValue<'a>(&'a Value);

    impl Serialize for WireValue<'_> {
        fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            match self.0 {
                Value::Number(number) => {
                    serializer.serialize_i64(number.as_i64().expect("small fixture integer"))
                }
                Value::Array(values) => {
                    let mut sequence = serializer.serialize_seq(Some(values.len()))?;
                    for value in values {
                        sequence.serialize_element(&Self(value))?;
                    }
                    sequence.end()
                }
                Value::Object(values) => {
                    let mut map = serializer.serialize_map(Some(values.len()))?;
                    for (key, value) in values {
                        map.serialize_entry(key, &Self(value))?;
                    }
                    map.end()
                }
                value => value.serialize(serializer),
            }
        }
    }

    fn messagepack(value: &Value) -> Vec<u8> {
        rmp_serde::to_vec_named(&WireValue(value)).unwrap()
    }

    #[test]
    fn malformed_enum_payloads_fail_in_both_codecs() {
        let valid = messagepack(&envelope(json!({"Count":1})));
        let decoded: Versioned<Events> = rmp_serde::from_slice(&valid).unwrap();
        assert_eq!(
            Events::from_versioned(decoded).unwrap().direct,
            Event::Count(1)
        );
        for invalid in [
            json!({}),
            json!({"Idle":null,"Count":1}),
            json!("Unknown"),
            json!("Count"),
            json!({"Count":"bad"}),
            json!({"Idle":7}),
            json!({"Pair":[1]}),
            json!({"Pair":[1,"a",2]}),
            json!({"Named":[1,null]}),
            json!({"Named":{"note":null}}),
            json!({"Record":[1]}),
            json!({"Named":{"amount":1,"extra":2}}),
        ] {
            let payload = envelope(invalid);
            assert!(
                serde_json::from_value::<Versioned<Events>>(payload.clone()).is_err(),
                "{payload}"
            );
            assert!(
                rmp_serde::from_slice::<Versioned<Events>>(&messagepack(&payload)).is_err(),
                "{payload}"
            );
        }
        for invalid in [
            r#"{"Count":1,"Count":2}"#,
            r#"{"Named":{"amount":1,"amount":2}}"#,
        ] {
            let payload = format!(
                r#"{{"stable_name":"billing.enum.events","version":1,"payload":{{"direct":{invalid},"optional":null,"list":[],"fixed":["Idle"]}}}}"#
            );
            assert!(serde_json::from_str::<Versioned<Events>>(&payload).is_err());
        }
    }

    struct Binary;
    impl Serialize for Binary {
        fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            serializer.serialize_bytes(&[0, 128, 255])
        }
    }

    #[test]
    fn binary_enum_payload_keeps_bytes_and_named_records_reject_compact_sequences() {
        // Start with a valid named envelope and replace its array payload with MessagePack bin8.
        let mut bytes = messagepack(&envelope(json!({"Bytes":[0,128,255]})));
        let array = messagepack(&json!([0, 128, 255]));
        let offset = bytes
            .windows(array.len())
            .position(|window| window == array)
            .unwrap();
        bytes.splice(
            offset..offset + array.len(),
            rmp_serde::to_vec(&Binary).unwrap(),
        );
        let decoded = rmp_serde::from_slice::<Versioned<Events>>(&bytes).unwrap();
        assert_eq!(
            Events::from_versioned(decoded).unwrap().direct,
            Event::Bytes(vec![0, 128, 255])
        );
        let compact = Events {
            direct: Event::Named {
                amount: 7,
                note: None,
            },
            optional: None,
            list: vec![],
            fixed: [Event::Idle],
        }
        .into_versioned();
        assert!(
            rmp_serde::from_slice::<Versioned<Events>>(&rmp_serde::to_vec(&compact).unwrap())
                .is_err()
        );
    }

    #[test]
    fn messagepack_duplicate_variant_tags_and_named_fields_fail() {
        for (valid, duplicate) in [
            (
                json!({"Count":1}),
                b"\x82\xa5Count\x01\xa5Count\x02".as_slice(),
            ),
            (
                json!({"Named":{"amount":1}}),
                b"\x81\xa5Named\x82\xa6amount\x01\xa6amount\x02".as_slice(),
            ),
        ] {
            let mut bytes = messagepack(&envelope(valid.clone()));
            let original = messagepack(&valid);
            let offset = bytes
                .windows(original.len())
                .position(|window| window == original)
                .unwrap();
            bytes.splice(offset..offset + original.len(), duplicate.iter().copied());
            let error = rmp_serde::from_slice::<Versioned<Events>>(&bytes)
                .err()
                .expect("duplicate rejected");
            assert!(
                error
                    .to_string()
                    .contains("duplicate field in historical payload"),
                "{error}"
            );
        }
    }
}
