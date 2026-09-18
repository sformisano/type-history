use super::{Date, LocalDateTime, LocalTime, OffsetDateTime, UtcInstant};
use ::time::{
    Date as NativeDate, Month, OffsetDateTime as NativeOffsetDateTime, PlainDateTime, Time,
    UtcOffset,
};
use std::collections::{BTreeSet, HashSet};

#[test]
fn time_checked_conversions_reject_year_bounds_and_offsets() {
    let negative = NativeDate::from_calendar_date(-1, Month::January, 1).unwrap();
    assert!(Date::try_from(negative).is_err());
    assert!(LocalDateTime::try_from(PlainDateTime::new(negative, Time::MIDNIGHT)).is_err());
    assert!(UtcInstant::try_from(NativeOffsetDateTime::new_utc(negative, Time::MIDNIGHT)).is_err());
    let date = NativeDate::from_calendar_date(2000, Month::January, 1).unwrap();
    let minute = UtcOffset::from_whole_seconds(60).unwrap();
    let native = NativeOffsetDateTime::new_in_offset(date, Time::MIDNIGHT, minute);
    assert!(UtcInstant::try_from(native)
        .unwrap_err()
        .reason()
        .contains("offset zero"));
    let second = UtcOffset::from_whole_seconds(1).unwrap();
    let native = NativeOffsetDateTime::new_in_offset(date, Time::MIDNIGHT, second);
    assert!(OffsetDateTime::try_from(native)
        .unwrap_err()
        .reason()
        .contains("minute-aligned"));
}

#[test]
fn time_offset_guards_both_dates_and_retains_native_equality() {
    for text in ["0000-01-01T00:00:00+00:01", "9999-12-31T23:59:59-00:01"] {
        assert!(OffsetDateTime::from_text(text).is_err(), "accepted {text}");
    }
    let a = OffsetDateTime::from_text("2000-01-01T00:00:00+00:00").unwrap();
    let b = OffsetDateTime::from_text("2000-01-01T01:00:00+01:00").unwrap();
    assert_eq!(a, b);
    assert_eq!(HashSet::from([a, b]).len(), 1);
    assert_eq!(BTreeSet::from([a, b]).len(), 1);
    assert_ne!(
        serde_json::to_string(&a).unwrap(),
        serde_json::to_string(&b).unwrap()
    );
}

#[test]
fn time_all_profiles_round_trip_boundaries_and_nanoseconds() {
    macro_rules! round_trip {
        ($type:ty, $($text:literal),+ $(,)?) => {$({
            let input = concat!("\"", $text, "\"");
            let value: $type = serde_json::from_str(input).unwrap();
            assert_eq!(serde_json::to_string(&value).unwrap(), input);
        })+};
    }
    round_trip!(Date, "0000-01-01", "9999-12-31");
    round_trip!(LocalTime, "00:00:00.000000001", "23:59:59.999999999");
    round_trip!(
        LocalDateTime,
        "0000-01-01T00:00:00",
        "9999-12-31T23:59:59.999999999"
    );
    round_trip!(
        UtcInstant,
        "0000-01-01T00:00:00Z",
        "9999-12-31T23:59:59.999999999Z"
    );
    round_trip!(
        OffsetDateTime,
        "0000-01-02T00:00:00+23:59",
        "9999-12-30T23:59:59.999999999-23:59"
    );
}
