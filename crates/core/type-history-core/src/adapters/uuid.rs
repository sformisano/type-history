use super::ProfileError;
use crate::resolved::StorageProfile;
use ::uuid::Uuid;

checked_wrapper!(
    /// UUID bits stored as lowercase, hyphenated text in every codec.
    UuidText, Uuid, UuidText
);

impl UuidText {
    fn validate(_: &Uuid) -> Result<(), ProfileError> {
        Ok(())
    }

    fn to_text(self) -> String {
        self.0.hyphenated().to_string()
    }

    fn from_text(text: &str) -> Result<Self, ProfileError> {
        let invalid = || {
            ProfileError::new(
                StorageProfile::UuidText.id(),
                "expected lowercase hyphenated UUID text",
            )
        };
        if text.len() != 36
            || !text.bytes().enumerate().all(|(index, byte)| {
                if matches!(index, 8 | 13 | 18 | 23) {
                    byte == b'-'
                } else {
                    byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)
                }
            })
        {
            return Err(invalid());
        }
        Uuid::parse_str(text).map(Self).map_err(|_| invalid())
    }
}

#[cfg(test)]
mod tests {
    use super::UuidText;
    use ::uuid::Uuid;

    #[test]
    fn uuid_all_bits_and_grammar_are_checked() {
        for bits in [0, u128::MAX, 0x1234_5678_9abc_def0_0123_4567_89ab_cdef] {
            let value = UuidText::try_from(Uuid::from_u128(bits)).unwrap();
            let text = serde_json::to_string(&value).unwrap();
            let decoded: UuidText = serde_json::from_str(&text).unwrap();
            assert_eq!(decoded.into_inner().as_u128(), bits);
            assert_eq!(text.len(), 38);
        }
        for bad in [
            "FFFFFFFF-FFFF-FFFF-FFFF-FFFFFFFFFFFF",
            "00000000000000000000000000000000",
            "00000000-0000-0000-0000-00000000000g",
            " 00000000-0000-0000-0000-000000000000",
        ] {
            assert!(UuidText::from_text(bad)
                .unwrap_err()
                .reason()
                .contains("lowercase hyphenated"));
        }
    }
}
