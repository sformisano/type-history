//! Checked Time values using the shared text profiles, independent of Time Serde features.

use super::temporal::{self, DateParts, DateTimeParts, OffsetParts, TimeParts};
use super::ProfileError;
use crate::resolved::StorageProfile;
use ::time::{
    Date as NativeDate, Month, OffsetDateTime as NativeOffsetDateTime, PlainDateTime, Time,
    UtcOffset,
};

checked_wrapper!(/// Proleptic Gregorian date in years 0000 through 9999.
    Date, NativeDate, Date);
checked_wrapper!(/// Local time with nanosecond precision and no leap seconds.
    LocalTime, Time, LocalTime);
checked_wrapper!(/// Local date and time without a timezone or offset.
    LocalDateTime, PlainDateTime, LocalDateTime);
checked_wrapper!(/// UTC instant. Checked conversion requires the native offset to be zero.
    UtcInstant, NativeOffsetDateTime, UtcInstant);
checked_wrapper!(/// Instant retaining its minute offset. Equality compares only the instant.
    OffsetDateTime, NativeOffsetDateTime, OffsetDateTime);

fn date_parts(value: NativeDate) -> DateParts {
    DateParts {
        year: value.year(),
        month: value.month() as u32,
        day: u32::from(value.day()),
    }
}

fn time_parts(value: Time) -> TimeParts {
    TimeParts {
        hour: u32::from(value.hour()),
        minute: u32::from(value.minute()),
        second: u32::from(value.second()),
        nanos: value.nanosecond(),
    }
}

fn datetime_parts(date: NativeDate, time: Time) -> DateTimeParts {
    DateTimeParts {
        date: date_parts(date),
        time: time_parts(time),
    }
}

fn native_date(parts: DateParts, profile: StorageProfile) -> Result<NativeDate, ProfileError> {
    let month = Month::try_from(parts.month as u8)
        .map_err(|_| temporal::invalid(profile, "native month is out of range"))?;
    NativeDate::from_calendar_date(parts.year, month, parts.day as u8)
        .map_err(|_| temporal::invalid(profile, "native date is out of range"))
}

fn native_time(parts: TimeParts, profile: StorageProfile) -> Result<Time, ProfileError> {
    Time::from_hms_nano(
        parts.hour as u8,
        parts.minute as u8,
        parts.second as u8,
        parts.nanos,
    )
    .map_err(|_| temporal::invalid(profile, "native time is out of range"))
}

fn native_datetime(
    parts: DateTimeParts,
    profile: StorageProfile,
) -> Result<PlainDateTime, ProfileError> {
    Ok(PlainDateTime::new(
        native_date(parts.date, profile)?,
        native_time(parts.time, profile)?,
    ))
}

impl Date {
    fn validate(value: &NativeDate) -> Result<(), ProfileError> {
        date_parts(*value).validate(StorageProfile::Date)
    }
    fn to_text(self) -> String {
        date_parts(self.0).text()
    }
    fn from_text(text: &str) -> Result<Self, ProfileError> {
        let profile = StorageProfile::Date;
        Self::try_from(native_date(DateParts::parse(text, profile)?, profile)?)
    }
}

impl LocalTime {
    fn validate(value: &Time) -> Result<(), ProfileError> {
        time_parts(*value).validate(StorageProfile::LocalTime)
    }
    fn to_text(self) -> String {
        time_parts(self.0).text()
    }
    fn from_text(text: &str) -> Result<Self, ProfileError> {
        let profile = StorageProfile::LocalTime;
        Self::try_from(native_time(TimeParts::parse(text, profile)?, profile)?)
    }
}

impl LocalDateTime {
    fn validate(value: &PlainDateTime) -> Result<(), ProfileError> {
        datetime_parts(value.date(), value.time()).validate(StorageProfile::LocalDateTime)
    }
    fn to_text(self) -> String {
        datetime_parts(self.0.date(), self.0.time()).text()
    }
    fn from_text(text: &str) -> Result<Self, ProfileError> {
        let profile = StorageProfile::LocalDateTime;
        Self::try_from(native_datetime(
            DateTimeParts::parse(text, profile)?,
            profile,
        )?)
    }
}

impl UtcInstant {
    fn validate(value: &NativeOffsetDateTime) -> Result<(), ProfileError> {
        let profile = StorageProfile::UtcInstant;
        if value.offset() != UtcOffset::UTC {
            return Err(temporal::invalid(
                profile,
                "UTC instant requires offset zero",
            ));
        }
        datetime_parts(value.date(), value.time()).validate(profile)
    }
    fn to_text(self) -> String {
        format!("{}Z", datetime_parts(self.0.date(), self.0.time()).text())
    }
    fn from_text(text: &str) -> Result<Self, ProfileError> {
        let local = native_datetime(DateTimeParts::parse_utc(text)?, StorageProfile::UtcInstant)?;
        Self::try_from(NativeOffsetDateTime::new_utc(local.date(), local.time()))
    }
}

impl OffsetDateTime {
    fn validate(value: &NativeOffsetDateTime) -> Result<(), ProfileError> {
        let profile = StorageProfile::OffsetDateTime;
        temporal::offset_minutes(value.offset().whole_seconds(), profile)?;
        datetime_parts(value.date(), value.time()).validate(profile)?;
        let utc = value
            .checked_to_utc()
            .ok_or_else(|| temporal::invalid(profile, "UTC date is out of range"))?;
        datetime_parts(utc.date(), utc.time()).validate(profile)
    }
    fn to_text(self) -> String {
        OffsetParts {
            local: datetime_parts(self.0.date(), self.0.time()),
            minutes: self.0.offset().whole_seconds() / 60,
        }
        .text()
    }
    fn from_text(text: &str) -> Result<Self, ProfileError> {
        let profile = StorageProfile::OffsetDateTime;
        let parts = OffsetParts::parse(text)?;
        let local = native_datetime(parts.local, profile)?;
        let offset = UtcOffset::from_whole_seconds(parts.minutes * 60)
            .map_err(|_| temporal::invalid(profile, "native offset is out of range"))?;
        Self::try_from(NativeOffsetDateTime::new_in_offset(
            local.date(),
            local.time(),
            offset,
        ))
    }
}

#[cfg(test)]
#[path = "time_tests.rs"]
mod tests;
