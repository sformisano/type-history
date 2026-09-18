use super::chrono::{
    Date as ChronoDate, LocalDateTime as ChronoLocalDateTime, LocalTime as ChronoLocalTime,
    OffsetDateTime as ChronoOffsetDateTime, UtcInstant as ChronoUtcInstant,
};
use super::time::{
    Date as TimeDate, LocalDateTime as TimeLocalDateTime, LocalTime as TimeLocalTime,
    OffsetDateTime as TimeOffsetDateTime, UtcInstant as TimeUtcInstant,
};
use crate::resolved::{JsonSchemaField, ResolvedSchema, SetMembership};
use schemars::SchemaGenerator;
use serde::{de::DeserializeOwned, Serialize};

fn same_profile<C, T>(texts: &[&str])
where
    C: DeserializeOwned + Serialize + ResolvedSchema + JsonSchemaField + SetMembership,
    T: DeserializeOwned + Serialize + ResolvedSchema + JsonSchemaField + SetMembership,
{
    assert_eq!(C::resolved_wire_schema(), T::resolved_wire_schema());
    assert!(C::MEMBERSHIP.same(&T::MEMBERSHIP));
    assert_eq!(
        C::json_schema(&mut SchemaGenerator::default()),
        T::json_schema(&mut SchemaGenerator::default())
    );
    for text in texts {
        let json = serde_json::to_string(text).unwrap();
        let chrono: C = serde_json::from_str(&json).unwrap();
        let time: T = serde_json::from_str(&json).unwrap();
        let chrono_json = serde_json::to_string(&chrono).unwrap();
        let time_json = serde_json::to_string(&time).unwrap();
        assert_eq!(chrono_json, time_json, "{text}");
        let chrono_from_time: C = serde_json::from_str(&time_json).unwrap();
        let time_from_chrono: T = serde_json::from_str(&chrono_json).unwrap();
        assert_eq!(
            serde_json::to_string(&chrono_from_time).unwrap(),
            chrono_json
        );
        assert_eq!(serde_json::to_string(&time_from_chrono).unwrap(), time_json);
    }
}

#[test]
fn both_libraries_share_every_profile_schema_and_canonical_text() {
    same_profile::<ChronoDate, TimeDate>(&["0000-01-01", "0000-02-29", "9999-12-31"]);
    same_profile::<ChronoLocalTime, TimeLocalTime>(&[
        "00:00:00.0",
        "12:34:56.100",
        "23:59:59.999999999",
        "00:00:00.000000001",
    ]);
    same_profile::<ChronoLocalDateTime, TimeLocalDateTime>(&[
        "0000-01-01T00:00:00",
        "9999-12-31T23:59:59.999999999",
    ]);
    same_profile::<ChronoUtcInstant, TimeUtcInstant>(&[
        "0000-01-01T00:00:00Z",
        "9999-12-31T23:59:59.999999999Z",
    ]);
    same_profile::<ChronoOffsetDateTime, TimeOffsetDateTime>(&[
        "0000-01-02T00:00:00+23:59",
        "9999-12-30T23:59:59.999999999-23:59",
        "2000-01-01T00:00:00.100+00:00",
    ]);
}

#[test]
fn both_libraries_reject_unknown_offsets_and_precision_loss() {
    for text in [
        "2000-01-01T00:00:00-00:00",
        "2000-01-01T00:00:00.1234567890+00:00",
        "2000-01-01T00:00:60+00:00",
        "0000-01-01T00:00:00+00:01",
        "9999-12-31T23:59:59-00:01",
    ] {
        let json = serde_json::to_string(text).unwrap();
        let chrono = serde_json::from_str::<ChronoOffsetDateTime>(&json).unwrap_err();
        let time = serde_json::from_str::<TimeOffsetDateTime>(&json).unwrap_err();
        if text.ends_with("-00:00") {
            assert!(chrono.to_string().contains("unknown local offset"));
            assert!(time.to_string().contains("unknown local offset"));
        }
    }
}
