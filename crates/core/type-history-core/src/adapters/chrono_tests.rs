use super::{Date, LocalDateTime, LocalTime, OffsetDateTime, UtcInstant};
use ::chrono::{DateTime, FixedOffset, NaiveDate, NaiveTime};
use std::collections::{BTreeSet, HashSet};

#[test]
fn chrono_checked_conversions_reject_leap_seconds_and_year_bounds() {
    for year in [-1, 10_000] {
        let date = NaiveDate::from_ymd_opt(year, 1, 1).unwrap();
        assert!(Date::try_from(date).is_err());
        assert!(LocalDateTime::try_from(date.and_hms_opt(0, 0, 0).unwrap()).is_err());
        assert!(UtcInstant::try_from(date.and_hms_opt(0, 0, 0).unwrap().and_utc()).is_err());
    }
    let leap = NaiveTime::from_hms_nano_opt(23, 59, 59, 1_000_000_000).unwrap();
    let date = NaiveDate::from_ymd_opt(2000, 1, 1).unwrap();
    assert!(LocalTime::try_from(leap)
        .unwrap_err()
        .reason()
        .contains("leap"));
    assert!(LocalDateTime::try_from(date.and_time(leap)).is_err());
    assert!(UtcInstant::try_from(date.and_time(leap).and_utc()).is_err());
    let one_second = FixedOffset::east_opt(1).unwrap();
    let native =
        DateTime::from_naive_utc_and_offset(date.and_hms_opt(0, 0, 0).unwrap(), one_second);
    assert!(OffsetDateTime::try_from(native)
        .unwrap_err()
        .reason()
        .contains("minute-aligned"));
}

#[test]
fn chrono_offset_guards_both_dates_and_retains_native_equality() {
    for text in [
        "0000-01-01T00:00:00+00:01",
        "9999-12-31T23:59:59-00:01",
        "-001-12-31T23:59:59-00:01",
        "10000-01-01T00:00:00+00:01",
    ] {
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
fn chrono_all_profiles_round_trip_boundaries_and_nanoseconds() {
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
