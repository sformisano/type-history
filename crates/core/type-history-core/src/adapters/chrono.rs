//! Checked Chrono values using the shared text profiles, independent of Chrono Serde features.

use super::temporal::{self, DateParts, DateTimeParts, OffsetParts, TimeParts};
use super::ProfileError;
use crate::resolved::StorageProfile;
use ::chrono::{
    DateTime, Datelike, FixedOffset, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Timelike, Utc,
};

checked_wrapper!(/// Proleptic Gregorian date in years 0000 through 9999.
    Date, NaiveDate, Date);
checked_wrapper!(/// Local time with nanosecond precision and no leap seconds.
    LocalTime, NaiveTime, LocalTime);
checked_wrapper!(/// Local date and time without a timezone or offset.
    LocalDateTime, NaiveDateTime, LocalDateTime);
checked_wrapper!(/// UTC instant stored with an uppercase Z suffix.
    UtcInstant, DateTime<Utc>, UtcInstant);
checked_wrapper!(/// Instant retaining its minute offset. Equality compares only the instant.
    OffsetDateTime, DateTime<FixedOffset>, OffsetDateTime);

fn date_parts(value: NaiveDate) -> DateParts {
    DateParts {
        year: value.year(),
        month: value.month(),
        day: value.day(),
    }
}

fn time_parts(value: NaiveTime) -> TimeParts {
    TimeParts {
        hour: value.hour(),
        minute: value.minute(),
        second: value.second(),
        nanos: value.nanosecond(),
    }
}

fn datetime_parts(value: NaiveDateTime) -> DateTimeParts {
    DateTimeParts {
        date: date_parts(value.date()),
        time: time_parts(value.time()),
    }
}

fn native_date(parts: DateParts, profile: StorageProfile) -> Result<NaiveDate, ProfileError> {
    NaiveDate::from_ymd_opt(parts.year, parts.month, parts.day)
        .ok_or_else(|| temporal::invalid(profile, "native date is out of range"))
}

fn native_time(parts: TimeParts, profile: StorageProfile) -> Result<NaiveTime, ProfileError> {
    NaiveTime::from_hms_nano_opt(parts.hour, parts.minute, parts.second, parts.nanos)
        .ok_or_else(|| temporal::invalid(profile, "native time is out of range"))
}

fn native_datetime(
    parts: DateTimeParts,
    profile: StorageProfile,
) -> Result<NaiveDateTime, ProfileError> {
    Ok(native_date(parts.date, profile)?.and_time(native_time(parts.time, profile)?))
}

impl Date {
    fn validate(value: &NaiveDate) -> Result<(), ProfileError> {
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
    fn validate(value: &NaiveTime) -> Result<(), ProfileError> {
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
    fn validate(value: &NaiveDateTime) -> Result<(), ProfileError> {
        datetime_parts(*value).validate(StorageProfile::LocalDateTime)
    }
    fn to_text(self) -> String {
        datetime_parts(self.0).text()
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
    fn validate(value: &DateTime<Utc>) -> Result<(), ProfileError> {
        datetime_parts(value.naive_utc()).validate(StorageProfile::UtcInstant)
    }
    fn to_text(self) -> String {
        format!("{}Z", datetime_parts(self.0.naive_utc()).text())
    }
    fn from_text(text: &str) -> Result<Self, ProfileError> {
        Self::try_from(
            native_datetime(DateTimeParts::parse_utc(text)?, StorageProfile::UtcInstant)?.and_utc(),
        )
    }
}

impl OffsetDateTime {
    fn validate(value: &DateTime<FixedOffset>) -> Result<(), ProfileError> {
        let profile = StorageProfile::OffsetDateTime;
        temporal::offset_minutes(value.offset().local_minus_utc(), profile)?;
        datetime_parts(value.naive_utc()).validate(profile)?;
        let local = value
            .naive_utc()
            .checked_add_offset(*value.offset())
            .ok_or_else(|| temporal::invalid(profile, "local date is out of range"))?;
        datetime_parts(local).validate(profile)
    }
    fn to_text(self) -> String {
        // Checked construction established that the local date is representable.
        OffsetParts {
            local: datetime_parts(self.0.naive_local()),
            minutes: self.0.offset().local_minus_utc() / 60,
        }
        .text()
    }
    fn from_text(text: &str) -> Result<Self, ProfileError> {
        let profile = StorageProfile::OffsetDateTime;
        let parts = OffsetParts::parse(text)?;
        let local = native_datetime(parts.local, profile)?;
        let offset = FixedOffset::east_opt(parts.minutes * 60)
            .ok_or_else(|| temporal::invalid(profile, "native offset is out of range"))?;
        let value = offset
            .from_local_datetime(&local)
            .single()
            .ok_or_else(|| temporal::invalid(profile, "UTC date is out of range"))?;
        Self::try_from(value)
    }
}

#[cfg(test)]
#[path = "chrono_tests.rs"]
mod tests;
