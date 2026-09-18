use history_api::adapters::time::{Date as CheckedDate, LocalDateTime as CheckedLocalDateTime, OffsetDateTime as CheckedOffsetDateTime, UtcInstant as CheckedUtcInstant};
use time_native::{Date, Month, OffsetDateTime, Time, UtcOffset};

#[test]
fn field_support_time_large_dates_native_outliers_exist_then_wrappers_reject() {
    for year in [-1, 10000] {
        let date = Date::from_calendar_date(year, Month::January, 1).unwrap();
        assert_eq!(date.year(), year, "native fixture must construct");
        let local = date.with_time(Time::MIDNIGHT);
        let utc = local.assume_utc();
        let offset = local.assume_offset(UtcOffset::from_hms(1, 0, 0).unwrap());
        assert!(CheckedDate::try_from(date).is_err());
        assert!(CheckedLocalDateTime::try_from(local).is_err());
        assert!(CheckedUtcInstant::try_from(utc).is_err());
        assert!(CheckedOffsetDateTime::try_from(offset).is_err());
    }
    for (year, month, day, hour, offset_hour) in [
        (0, Month::January, 1, 0, 1), (-1, Month::December, 31, 23, -1),
        (9999, Month::December, 31, 23, -1), (10000, Month::January, 1, 0, 1),
    ] {
        let value = OffsetDateTime::new_in_offset(
            Date::from_calendar_date(year, month, day).unwrap(),
            Time::from_hms(hour, 0, 0).unwrap(), UtcOffset::from_hms(offset_hour, 0, 0).unwrap(),
        );
        let utc = value.checked_to_utc().unwrap();
        assert_ne!((0..=9999).contains(&value.year()), (0..=9999).contains(&utc.year()));
        assert!(CheckedOffsetDateTime::try_from(value).is_err(), "independent local/UTC guard: {value}");
    }
    for year in [0, 9999] {
        let date = Date::from_calendar_date(year, Month::January, 1).unwrap();
        let local = date.with_time(Time::MIDNIGHT);
        assert!(CheckedDate::try_from(date).is_ok());
        assert!(CheckedLocalDateTime::try_from(local).is_ok());
        assert!(CheckedUtcInstant::try_from(local.assume_utc()).is_ok());
        assert!(CheckedOffsetDateTime::try_from(local.assume_utc()).is_ok());
    }
    let second_offset = Date::from_calendar_date(2024, Month::January, 1).unwrap().midnight().assume_offset(UtcOffset::from_hms(0, 0, 1).unwrap());
    assert!(CheckedOffsetDateTime::try_from(second_offset).is_err());
    assert!(CheckedUtcInstant::try_from(second_offset).is_err());
}
