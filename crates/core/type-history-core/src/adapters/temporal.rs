//! One grammar and formatter for both native temporal libraries.

mod parse;
#[cfg(test)]
mod tests;

use super::ProfileError;
use crate::resolved::StorageProfile;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct DateParts {
    pub year: i32,
    pub month: u32,
    pub day: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct TimeParts {
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
    pub nanos: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct DateTimeParts {
    pub date: DateParts,
    pub time: TimeParts,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct OffsetParts {
    pub local: DateTimeParts,
    pub minutes: i32,
}

pub(super) fn invalid(profile: StorageProfile, reason: &str) -> ProfileError {
    ProfileError::new(profile.id(), reason)
}

impl DateParts {
    pub fn validate(self, profile: StorageProfile) -> Result<(), ProfileError> {
        if !(0..=9999).contains(&self.year) {
            return Err(invalid(profile, "year must be in 0000..9999"));
        }
        let leap = self.year % 4 == 0 && (self.year % 100 != 0 || self.year % 400 == 0);
        let days = match self.month {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 if leap => 29,
            2 => 28,
            _ => return Err(invalid(profile, "invalid Gregorian month")),
        };
        if self.day == 0 || self.day > days {
            return Err(invalid(profile, "invalid Gregorian day"));
        }
        Ok(())
    }

    pub fn text(self) -> String {
        format!("{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

impl TimeParts {
    pub fn validate(self, profile: StorageProfile) -> Result<(), ProfileError> {
        if self.hour > 23 || self.minute > 59 || self.second > 59 {
            return Err(invalid(
                profile,
                "time requires hour 00..23 and minute/second 00..59",
            ));
        }
        if self.nanos >= 1_000_000_000 {
            return Err(invalid(
                profile,
                "leap seconds are outside the temporal profile",
            ));
        }
        Ok(())
    }

    pub fn text(self) -> String {
        let mut text = format!("{:02}:{:02}:{:02}", self.hour, self.minute, self.second);
        if self.nanos != 0 {
            text.push('.');
            text.push_str(format!("{:09}", self.nanos).trim_end_matches('0'));
        }
        text
    }
}

impl DateTimeParts {
    pub fn validate(self, profile: StorageProfile) -> Result<(), ProfileError> {
        self.date.validate(profile)?;
        self.time.validate(profile)
    }

    pub fn text(self) -> String {
        format!("{}T{}", self.date.text(), self.time.text())
    }
}

impl OffsetParts {
    pub fn text(self) -> String {
        let magnitude = self.minutes.unsigned_abs();
        format!(
            "{}{}{:02}:{:02}",
            self.local.text(),
            if self.minutes < 0 { '-' } else { '+' },
            magnitude / 60,
            magnitude % 60
        )
    }
}

pub(super) fn offset_minutes(seconds: i32, profile: StorageProfile) -> Result<i32, ProfileError> {
    if seconds % 60 != 0 || !(-86_340..=86_340).contains(&seconds) {
        return Err(invalid(
            profile,
            "offset must be minute-aligned within -23:59..+23:59",
        ));
    }
    Ok(seconds / 60)
}
