use super::{invalid, DateParts, DateTimeParts, OffsetParts, TimeParts};
use crate::resolved::{ProfileError, StorageProfile};

fn digits(text: &[u8], profile: StorageProfile) -> Result<u32, ProfileError> {
    if text.is_empty() || !text.iter().all(u8::is_ascii_digit) {
        return Err(invalid(profile, "expected fixed-width ASCII digits"));
    }
    Ok(text
        .iter()
        .fold(0, |value, digit| value * 10 + u32::from(digit - b'0')))
}

impl DateParts {
    pub fn parse(text: &str, profile: StorageProfile) -> Result<Self, ProfileError> {
        let bytes = text.as_bytes();
        if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
            return Err(invalid(profile, "expected YYYY-MM-DD"));
        }
        let parts = Self {
            year: digits(&bytes[..4], profile)? as i32,
            month: digits(&bytes[5..7], profile)?,
            day: digits(&bytes[8..], profile)?,
        };
        parts.validate(profile)?;
        Ok(parts)
    }
}

impl TimeParts {
    pub fn parse(text: &str, profile: StorageProfile) -> Result<Self, ProfileError> {
        let bytes = text.as_bytes();
        if bytes.len() < 8 || bytes[2] != b':' || bytes[5] != b':' {
            return Err(invalid(
                profile,
                "expected HH:MM:SS with an optional nanosecond fraction",
            ));
        }
        let nanos = if bytes.len() == 8 {
            0
        } else {
            if bytes[8] != b'.' || !(10..=18).contains(&bytes.len()) {
                return Err(invalid(profile, "fraction must contain 1..9 digits"));
            }
            digits(&bytes[9..], profile)? * 10u32.pow((18 - bytes.len()) as u32)
        };
        let parts = Self {
            hour: digits(&bytes[..2], profile)?,
            minute: digits(&bytes[3..5], profile)?,
            second: digits(&bytes[6..8], profile)?,
            nanos,
        };
        parts.validate(profile)?;
        Ok(parts)
    }
}

impl DateTimeParts {
    pub fn parse(text: &str, profile: StorageProfile) -> Result<Self, ProfileError> {
        if !text.is_ascii() || text.as_bytes().get(10) != Some(&b'T') {
            return Err(invalid(
                profile,
                "expected date and time joined by uppercase T",
            ));
        }
        Ok(Self {
            date: DateParts::parse(&text[..10], profile)?,
            time: TimeParts::parse(&text[11..], profile)?,
        })
    }

    pub fn parse_utc(text: &str) -> Result<Self, ProfileError> {
        let profile = StorageProfile::UtcInstant;
        let text = text
            .strip_suffix('Z')
            .ok_or_else(|| invalid(profile, "UTC instant must end in uppercase Z"))?;
        Self::parse(text, profile)
    }
}

impl OffsetParts {
    pub fn parse(text: &str) -> Result<Self, ProfileError> {
        let profile = StorageProfile::OffsetDateTime;
        if !text.is_ascii() || text.len() < 25 {
            return Err(invalid(
                profile,
                "expected local datetime and signed HH:MM offset",
            ));
        }
        let (local, offset) = text.split_at(text.len() - 6);
        let bytes = offset.as_bytes();
        if !matches!(bytes[0], b'+' | b'-') || bytes[3] != b':' {
            return Err(invalid(profile, "offset must use signed HH:MM"));
        }
        let hours = digits(&bytes[1..3], profile)?;
        let minutes = digits(&bytes[4..6], profile)?;
        if hours > 23 || minutes > 59 {
            return Err(invalid(profile, "offset exceeds 23:59"));
        }
        if offset == "-00:00" {
            return Err(invalid(
                profile,
                "unknown local offset -00:00 cannot be preserved",
            ));
        }
        let magnitude = (hours * 60 + minutes) as i32;
        Ok(Self {
            local: DateTimeParts::parse(local, profile)?,
            minutes: if bytes[0] == b'-' {
                -magnitude
            } else {
                magnitude
            },
        })
    }
}
