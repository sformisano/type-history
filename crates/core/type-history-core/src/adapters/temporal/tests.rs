use super::{DateParts, DateTimeParts, OffsetParts, TimeParts};
use crate::resolved::StorageProfile;

#[test]
fn shared_parser_accepts_exact_short_fractions_and_formats_shortest() {
    for (input, output) in [
        ("12:34:56.0", "12:34:56"),
        ("12:34:56.100", "12:34:56.1"),
        ("12:34:56.000000001", "12:34:56.000000001"),
        ("12:34:56.999999999", "12:34:56.999999999"),
    ] {
        assert_eq!(
            TimeParts::parse(input, StorageProfile::LocalTime)
                .unwrap()
                .text(),
            output
        );
    }
    for date in ["0000-02-29", "2000-02-29", "9999-12-31"] {
        assert_eq!(
            DateParts::parse(date, StorageProfile::Date).unwrap().text(),
            date
        );
    }
    for offset in ["+00:00", "+23:59", "-23:59"] {
        let text = format!("2000-01-01T12:34:56{offset}");
        assert_eq!(OffsetParts::parse(&text).unwrap().text(), text);
    }
}

#[test]
fn shared_parser_rejects_broader_native_grammars_and_unknown_offset() {
    for bad in [
        "-001-01-01",
        "10000-01-01",
        "1900-02-29",
        "2020-00-01",
        "2020-01-00",
        "2020-04-31",
    ] {
        assert!(
            DateParts::parse(bad, StorageProfile::Date).is_err(),
            "accepted {bad}"
        );
    }
    for bad in [
        "24:00:00",
        "23:60:00",
        "23:59:60",
        "12:34:56.",
        "12:34:56.1234567890",
        "12:34:56.0000000000",
        "1:23:45",
        "12:34:56Z",
        "12:34:56.１",
    ] {
        assert!(
            TimeParts::parse(bad, StorageProfile::LocalTime).is_err(),
            "accepted {bad}"
        );
    }
    for bad in [
        "2000-01-01 00:00:00",
        "2000-01-01t00:00:00",
        "2000-01-01T00:00:00z",
        "2000-01-01T00:00:00+00:00",
    ] {
        assert!(DateTimeParts::parse_utc(bad).is_err(), "accepted {bad}");
    }
    for bad in [
        "2000-01-01T00:00:00Z",
        "2000-01-01T00:00:00+00:00:00",
        "2000-01-01T00:00:00+24:00",
        "2000-01-01T00:00:00-00:00",
        "2000-01-01T00:00:00+00:60",
    ] {
        assert!(OffsetParts::parse(bad).is_err(), "accepted {bad}");
    }
    assert!(OffsetParts::parse("2000-01-01T00:00:00-00:00")
        .unwrap_err()
        .reason()
        .contains("unknown local offset"));
}
