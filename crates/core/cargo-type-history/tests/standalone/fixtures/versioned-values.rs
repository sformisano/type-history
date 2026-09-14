use history_api::Schema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Schema)]
#[serde(deny_unknown_fields)]
pub struct Detail {
    pub description: String,
    pub total: u128,
}

#[versioned(stable_name = "example.structured")]
pub struct Structured {
    pub enabled: bool,
    pub note: Option<String>,
    pub numbers: Vec<i128>,
    pub bytes: Vec<u8>,
    pub fixed: [u16; 2],
    pub detail: Detail,
}

#[versioned(stable_name = "example.binary")]
pub struct BinaryPayload {
    pub bytes: Vec<u8>,
    pub fixed: [u8; 3],
}

#[cfg(test)]
mod value_tests {
    use super::{BinaryPayload, Detail, Structured, Wide};
    use history_api::{DecodeFailureKind, HasHistory, PayloadVersion, Versioned};
    use rmp_serde::{from_slice as from_messagepack, to_vec_named};
    use serde::{Serialize, Serializer};
    use serde_json::{from_slice, from_str, to_value, to_vec};

    // Keep the payload first even in formats that preserve struct field order.
    #[derive(Serialize)]
    struct PayloadFirst<'a, T> {
        payload: &'a T,
        version: u32,
        stable_name: &'a str,
    }

    #[test]
    fn messagepack_binary_values_keep_their_byte_sequence() {
        struct Bytes;
        impl Serialize for Bytes {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_bytes(&[0, 128, 255])
            }
        }
        #[derive(Serialize)]
        struct BinaryFields {
            bytes: Bytes,
            fixed: Bytes,
        }
        let payload = BinaryFields {
            bytes: Bytes,
            fixed: Bytes,
        };
        let raw = to_vec_named(&payload).unwrap();
        let expected: BinaryPayload = from_messagepack(&raw).unwrap();
        assert_eq!(expected.bytes, [0, 128, 255]);
        assert_eq!(expected.fixed, [0, 128, 255]);

        let envelope = PayloadFirst {
            payload: &payload,
            version: 1,
            stable_name: "example.binary",
        };
        let binary = to_vec_named(&envelope).unwrap();
        let stored: Versioned<BinaryPayload> = from_messagepack(&binary).unwrap();
        assert_eq!(BinaryPayload::from_versioned(stored).unwrap(), expected);
    }

    #[test]
    fn payload_first_preserves_wide_integers_in_json_and_messagepack() {
        for (unsigned, signed) in [
            (u128::MAX, i128::MIN),
            (0, 0),
            (u128::from(u64::MAX) + 1, i128::from(i64::MIN) - 1),
            (u128::MAX, i128::MAX),
        ] {
            let expected = Wide { unsigned, signed };
            let raw = to_vec(&expected).unwrap();
            assert_eq!(
                Wide::history()
                    .decode(PayloadVersion::INITIAL, &raw)
                    .unwrap(),
                expected
            );
            let envelope = PayloadFirst {
                payload: &expected,
                version: 1,
                stable_name: "example.wide.record",
            };
            let json = to_vec(&envelope).unwrap();
            assert!(json.starts_with(br#"{"payload":"#));
            let stored: Versioned<Wide> = from_slice(&json).unwrap();
            assert_eq!(Wide::from_versioned(stored).unwrap(), expected);

            let binary = to_vec_named(&envelope).unwrap();
            let stored: Versioned<Wide> = from_messagepack(&binary).unwrap();
            assert_eq!(Wide::from_versioned(stored).unwrap(), expected);

            let binary = to_vec_named(&expected.clone().into_versioned()).unwrap();
            let stored: Versioned<Wide> = from_messagepack(&binary).unwrap();
            assert_eq!(Wide::from_versioned(stored).unwrap(), expected);
        }
    }

    #[test]
    fn invalid_wide_integers_fail_in_raw_json_and_versioned_payloads() {
        for (field, number) in [
            ("unsigned", "340282366920938463463374607431768211456"),
            ("signed", "-170141183460469231731687303715884105729"),
            ("signed", "170141183460469231731687303715884105728"),
            ("unsigned", "-1"),
            ("unsigned", "1.5"),
            ("signed", "-1.5"),
            ("unsigned", "1e0"),
            ("signed", "1e0"),
        ] {
            let (unsigned, signed) = if field == "unsigned" {
                (number, "0")
            } else {
                ("0", number)
            };
            let payload = format!(r#"{{"unsigned":{unsigned},"signed":{signed}}}"#);
            let error = Wide::history()
                .decode(PayloadVersion::INITIAL, payload.as_bytes())
                .expect_err("invalid integer must fail before conversion");
            assert_eq!(error.kind(), DecodeFailureKind::Decode, "{field}: {number}");
            assert_eq!(error.source_version(), PayloadVersion::INITIAL);

            let wire = format!(
                r#"{{"payload":{payload},"version":1,"stable_name":"example.wide.record"}}"#
            );
            let error = from_str::<Versioned<Wide>>(&wire)
                .expect_err("invalid integer must fail envelope decoding");
            assert!(error.is_data(), "{field}: {number}: {error}");
        }
    }

    #[test]
    fn messagepack_wide_integers_require_exactly_sixteen_bytes() {
        struct IntegerBytes<'a>(&'a [u8]);
        impl Serialize for IntegerBytes<'_> {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_bytes(self.0)
            }
        }
        #[derive(Serialize)]
        struct BinaryIntegers<'a> {
            unsigned: IntegerBytes<'a>,
            signed: IntegerBytes<'a>,
        }
        let zeros = [0; 17];
        for (unsigned_length, signed_length, rejected_type) in [
            (16, 16, None),
            (15, 16, Some("u128")),
            (17, 16, Some("u128")),
            (16, 15, Some("i128")),
            (16, 17, Some("i128")),
        ] {
            let payload = BinaryIntegers {
                unsigned: IntegerBytes(&zeros[..unsigned_length]),
                signed: IntegerBytes(&zeros[..signed_length]),
            };
            let envelope = PayloadFirst {
                payload: &payload,
                version: 1,
                stable_name: "example.wide.record",
            };
            let bytes = to_vec_named(&envelope).unwrap();
            let decoded = from_messagepack::<Versioned<Wide>>(&bytes);
            if let Some(rejected_type) = rejected_type {
                let error = decoded.expect_err("invalid binary integer length must fail");
                let expected =
                    format!("a binary {rejected_type} payload must contain exactly 16 bytes");
                assert!(error.to_string().contains(&expected), "{error}");
            } else {
                let value = Wide::from_versioned(decoded.unwrap()).unwrap();
                assert_eq!(
                    value,
                    Wide {
                        unsigned: 0,
                        signed: 0
                    }
                );
            }
        }
    }

    #[test]
    fn supported_containers_and_nested_records_survive_buffering() {
        for note in [None, Some("note".to_owned())] {
            let expected = Structured {
                enabled: true,
                note,
                numbers: vec![i128::MIN, 0, i128::MAX],
                bytes: vec![0, 128, 255],
                fixed: [0, u16::MAX],
                detail: Detail {
                    description: "nested".into(),
                    total: u128::MAX,
                },
            };
            let raw = to_vec(&expected).unwrap();
            assert_eq!(
                Structured::history()
                    .decode(PayloadVersion::INITIAL, &raw)
                    .unwrap(),
                expected
            );
            let envelope = PayloadFirst {
                payload: &expected,
                version: 1,
                stable_name: "example.structured",
            };
            let json = to_vec(&envelope).unwrap();
            let stored: Versioned<Structured> = from_slice(&json).unwrap();
            assert_eq!(Structured::from_versioned(stored).unwrap(), expected);

            let binary = to_vec_named(&envelope).unwrap();
            let stored: Versioned<Structured> = from_messagepack(&binary).unwrap();
            assert_eq!(Structured::from_versioned(stored).unwrap(), expected);

            // An intermediate Serde value also puts payload before version.
            let json = to_vec(&to_value(expected.clone().into_versioned()).unwrap()).unwrap();
            let stored: Versioned<Structured> = from_slice(&json).unwrap();
            assert_eq!(Structured::from_versioned(stored).unwrap(), expected);
        }
    }

    #[test]
    fn nested_records_reject_positional_duplicate_and_unknown_fields() {
        for detail in [
            r#"{"description":"one","description":"two","total":1}"#,
            r#"{"description":"one","total":1,"extra":true}"#,
            r#"["one",1]"#,
        ] {
            let payload = format!(
                r#"{{"enabled":true,"note":null,"numbers":[],"bytes":[],"fixed":[0,1],"detail":{detail}}}"#
            );
            let wire = format!(
                r#"{{"payload":{payload},"version":1,"stable_name":"example.structured"}}"#
            );
            assert!(from_str::<Versioned<Structured>>(&wire).is_err());
            assert!(Structured::history()
                .decode(PayloadVersion::INITIAL, payload.as_bytes())
                .is_err());
        }
    }
}
