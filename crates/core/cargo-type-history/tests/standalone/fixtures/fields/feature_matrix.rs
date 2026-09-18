#[cfg(test)]
mod tests {
    use history_api::ResolvedSchema;
    use serde::{Serialize, de::DeserializeOwned};
    use serde_json::{json, Value};
    use std::collections::BTreeMap;
    use std::fs;

    fn snapshot<T: ResolvedSchema + Serialize + DeserializeOwned>(input: &str) -> Value {
        let value: T = serde_json::from_str(input).unwrap();
        let json = serde_json::to_vec(&value).unwrap();
        let binary = rmp_serde::to_vec_named(&value).unwrap();
        let decoded: T = rmp_serde::from_slice(&binary).unwrap();
        assert_eq!(serde_json::to_vec(&decoded).unwrap(), json);
        json!({"schema":T::resolved_wire_schema(),"json":json,"messagepack":binary})
    }

    #[test]
    fn field_support_feature_snapshot_and_checked_domains() {
        #[allow(unused_mut)]
        let mut snapshots = BTreeMap::<&str, Value>::new();
        #[cfg(feature = "typed-floats")]
        {
            use float_native::NonNaNFinite;
            snapshots.insert("typed-floats", json!([
                snapshot::<NonNaNFinite<f32>>("-0.0"), snapshot::<NonNaNFinite<f64>>("-0.0"),
                snapshot::<NonNaNFinite<f32>>("1e-45"), snapshot::<NonNaNFinite<f64>>("5e-324")
            ]));
            assert!(NonNaNFinite::<f64>::try_from(f64::INFINITY).is_err());
        }
        #[cfg(feature = "uuid")]
        {
            use history_api::adapters::UuidText;
            snapshots.insert("uuid", snapshot::<UuidText>("\"ffffffff-ffff-ffff-ffff-ffffffffffff\""));
            assert!(serde_json::from_str::<UuidText>("\"FFFFFFFF-FFFF-FFFF-FFFF-FFFFFFFFFFFF\"").is_err());
        }
        #[cfg(feature = "rust-decimal")]
        {
            use history_api::adapters::DecimalText;
            snapshots.insert("rust-decimal", json!([snapshot::<DecimalText>("\"123.400\""), snapshot::<DecimalText>("\"7.9228162514264337593543950335\"")]));
            assert!(serde_json::from_str::<DecimalText>("\"0.00000000000000000000000000000\"").is_err());
        }
        #[cfg(feature = "chrono")]
        {
            use history_api::adapters::chrono::{Date, LocalTime, LocalDateTime, UtcInstant, OffsetDateTime};
            snapshots.insert("chrono", temporal!(Date, LocalTime, LocalDateTime, UtcInstant, OffsetDateTime));
        }
        #[cfg(feature = "time")]
        {
            use history_api::adapters::time::{Date, LocalTime, LocalDateTime, UtcInstant, OffsetDateTime};
            snapshots.insert("time", temporal!(Date, LocalTime, LocalDateTime, UtcInstant, OffsetDateTime));
        }
        #[cfg(feature = "rc")]
        {
            use std::rc::Rc;
            use std::sync::Arc;
            snapshots.insert("rc", json!([snapshot::<Rc<Option<u32>>>("null"), snapshot::<Arc<(String, u32)>>("[\"x\",7]")]));
        }
        fs::write("feature-snapshots.json", serde_json::to_vec(&snapshots).unwrap()).unwrap();
    }

    #[allow(unused_macros)]
    macro_rules! temporal {
        ($date:ty, $time:ty, $local:ty, $utc:ty, $offset:ty) => {{
            for invalid in ["\"0000-01-01T00:00:00+00:01\"", "\"2024-01-01T00:00:00-00:00\"", "\"2024-01-01T00:00:00+00:00:01\""] {
                assert!(serde_json::from_str::<$offset>(invalid).is_err());
            }
            assert!(serde_json::from_str::<$time>("\"23:59:60\"").is_err());
            json!([
                snapshot::<$date>("\"0000-01-01\""), snapshot::<$date>("\"9999-12-31\""),
                snapshot::<$time>("\"23:59:59.999999999\""), snapshot::<$local>("\"0000-01-01T00:00:00.000000001\""),
                snapshot::<$utc>("\"9999-12-31T23:59:59.999999999Z\""), snapshot::<$offset>("\"0000-01-01T00:00:00+00:00\""),
                snapshot::<$offset>("\"2024-01-01T12:34:56.1+23:59\"")
            ])
        }};
    }
    use temporal;
}
