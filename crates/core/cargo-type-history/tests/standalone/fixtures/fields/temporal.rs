use history_api::adapters::chrono as temporal;
use history_api::versioned;
use temporal::{Date, LocalDateTime, LocalTime, OffsetDateTime, UtcInstant};

#[versioned(stable_name = "fields.temporal")]
pub struct Temporal {
    pub date: Date,
    pub time: LocalTime,
    pub local: LocalDateTime,
    pub utc: UtcInstant,
    pub offset: OffsetDateTime,
    pub nested: Option<(Date, OffsetDateTime)>,
}

#[cfg(test)]
mod common;
#[cfg(test)]
mod tests {
    use super::{common, Temporal};
    use history_api::Versioned;
    use serde_json::{json, Value};
    use std::fs;

    fn payload(date: &str, time: &str, offset: &str) -> Value {
        json!({"date":date,"time":time,"local":format!("{date}T{time}"),"utc":format!("{date}T{time}Z"),
            "offset":format!("{date}T{time}{offset}"),"nested":[date,format!("{date}T{time}{offset}")]})
    }

    fn read(json: &[u8], binary: &[u8], expected: &Value) {
        let json: Versioned<Temporal> = serde_json::from_slice(json).unwrap();
        let binary: Versioned<Temporal> = rmp_serde::from_slice(binary).unwrap();
        for stored in [json, binary] {
            let value = Temporal::from_versioned(stored).unwrap();
            // Compare encoded components, fractions, and offsets separately from
            // native Eq, which intentionally compares offset datetimes by instant.
            assert_eq!(serde_json::to_value(value.date).unwrap(), expected["date"]);
            assert_eq!(serde_json::to_value(value.time).unwrap(), expected["time"]);
            assert_eq!(serde_json::to_value(value.local).unwrap(), expected["local"]);
            assert_eq!(serde_json::to_value(value.utc).unwrap(), expected["utc"]);
            assert_eq!(serde_json::to_value(value.offset).unwrap(), expected["offset"]);
            assert_eq!(serde_json::to_value(value.nested).unwrap(), expected["nested"]);
        }
    }

    #[test]
    fn field_support_temporal_retained_profiles_read_in_both_directions() {
        for (index, (date, time, offset)) in [
            ("0000-01-01", "00:00:00", "+00:00"), ("9999-12-31", "23:59:59.999999999", "+00:00"),
            ("2000-02-29", "12:34:56.000000001", "+23:59"), ("2024-05-06", "01:02:03.12", "-23:59"),
        ].into_iter().enumerate() {
            let expected = payload(date, time, offset);
            let value: Temporal = serde_json::from_value(expected.clone()).unwrap();
            let (json, binary) = common::retain(&format!("temporal-{index}"), &value.clone().into_versioned());
            read(&json, &binary, &expected);
            let reverse_json = format!("temporal-reverse-{index}.json");
            let reverse_binary = format!("temporal-reverse-{index}.msgpack");
            if fs::exists(&reverse_json).unwrap() {
                read(&fs::read(&reverse_json).unwrap(), &fs::read(&reverse_binary).unwrap(), &expected);
            }
            // Every pass writes the current library's bytes for the next library.
            fs::write(reverse_json, serde_json::to_vec(&value.clone().into_versioned()).unwrap()).unwrap();
            fs::write(reverse_binary, rmp_serde::to_vec_named(&value.into_versioned()).unwrap()).unwrap();
        }
    }

    #[test]
    fn field_support_temporal_strict_domain_and_short_fractions() {
        let base = payload("2024-02-29", "12:34:56", "+00:00");
        for (field, invalid) in [
            ("date", "-0001-12-31"), ("date", "10000-01-01"), ("date", "2023-02-29"), ("date", "2024-2-29"),
            ("time", "12:34:60"), ("time", "24:00:00"), ("time", "12:34:56.0000000000"),
            ("local", "2024-02-29 12:34:56"), ("utc", "2024-02-29T12:34:56+00:00"),
            ("utc", "2024-02-29T12:34:56z"), ("offset", "2024-02-29T12:34:56-00:00"),
            ("offset", "2024-02-29T12:34:56+01:02:03"), ("offset", "2024-02-29T12:34:56+24:00"),
            ("offset", "0000-01-01T00:00:00+00:01"), ("offset", "9999-12-31T23:59:59-00:01"),
        ] {
            let mut value = base.clone(); value[field] = json!(invalid);
            let envelope = json!({"stable_name":"fields.temporal","version":1,"payload":value});
            assert!(serde_json::from_value::<Versioned<Temporal>>(envelope.clone()).is_err(), "{field}: {invalid}");
            assert!(rmp_serde::from_slice::<Versioned<Temporal>>(&common::messagepack(&envelope)).is_err(), "{field}: {invalid}");
        }
        for (input, canonical) in [("12:34:56.0", "12:34:56"), ("12:34:56.100", "12:34:56.1")] {
            let value = payload("2024-02-29", input, "+00:00");
            let expected = payload("2024-02-29", canonical, "+00:00");
            let envelope = json!({"stable_name":"fields.temporal","version":1,"payload":value});
            read(&serde_json::to_vec(&envelope).unwrap(), &common::messagepack(&envelope), &expected);
        }
    }
}
