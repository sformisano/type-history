use super::ProfileError;
use crate::resolved::StorageProfile;
use rust_decimal::Decimal;

const MAX_COEFFICIENT: u128 = (1u128 << 96) - 1;

checked_wrapper!(
    /// Exact 96-bit decimal text retaining scale. Equality follows numeric value.
    DecimalText, Decimal, DecimalText
);

fn invalid(reason: &str) -> ProfileError {
    ProfileError::new(StorageProfile::DecimalText.id(), reason)
}

impl DecimalText {
    fn validate(value: &Decimal) -> Result<(), ProfileError> {
        if value.scale() > 28 || value.mantissa().unsigned_abs() > MAX_COEFFICIENT {
            return Err(invalid("decimal exceeds 96-bit coefficient or scale 28"));
        }
        Ok(())
    }

    fn to_text(self) -> String {
        let magnitude = self.0.mantissa().unsigned_abs();
        let scale = self.0.scale() as usize;
        let digits = magnitude.to_string();
        let sign = if self.0.is_sign_negative() { "-" } else { "" };
        if scale == 0 {
            return format!("{sign}{digits}");
        }
        if digits.len() <= scale {
            format!("{sign}0.{}{}", "0".repeat(scale - digits.len()), digits)
        } else {
            let (integer, fraction) = digits.split_at(digits.len() - scale);
            format!("{sign}{integer}.{fraction}")
        }
    }

    fn from_text(text: &str) -> Result<Self, ProfileError> {
        let negative = text.starts_with('-');
        let unsigned = text.strip_prefix('-').unwrap_or(text);
        let (integer, fraction) = match unsigned.split_once('.') {
            Some((integer, fraction)) if !fraction.is_empty() => (integer, fraction),
            Some(_) => return Err(invalid("decimal fraction must contain digits")),
            None => (unsigned, ""),
        };
        if integer.is_empty()
            || !integer.bytes().all(|b| b.is_ascii_digit())
            || !fraction.bytes().all(|b| b.is_ascii_digit())
        {
            return Err(invalid(
                "expected signed decimal digits without exponent, whitespace or separators",
            ));
        }
        if fraction.len() > 28 {
            return Err(invalid("decimal scale exceeds 28"));
        }
        let mut coefficient = 0u128;
        for digit in integer.bytes().chain(fraction.bytes()) {
            coefficient = coefficient
                .checked_mul(10)
                .and_then(|n| n.checked_add(u128::from(digit - b'0')))
                .filter(|n| *n <= MAX_COEFFICIENT)
                .ok_or_else(|| invalid("decimal coefficient exceeds 96 bits"))?;
        }
        let mut value = Decimal::from_parts(
            coefficient as u32,
            (coefficient >> 32) as u32,
            (coefficient >> 64) as u32,
            negative,
            fraction.len() as u32,
        );
        // Native constructors normalize zero's sign, but native mutation can
        // create negative zero. Preserve that sign in the owned text profile.
        value.set_sign_negative(negative);
        Self::try_from(value)
    }
}

#[cfg(test)]
mod tests {
    use super::DecimalText;
    use rust_decimal::Decimal;
    use std::collections::{BTreeSet, HashSet};

    #[test]
    fn decimal_preserves_coefficient_and_scale() {
        for text in [
            "123.400",
            "0",
            "-0",
            "-0.000",
            "0.0000000000000000000000000000",
            "-123.400",
            "79228162514264337593543950335",
            "-7.9228162514264337593543950335",
            "0.0000000000000000000000000001",
        ] {
            let original = DecimalText::from_text(text).unwrap();
            let json = serde_json::to_string(&original).unwrap();
            assert_eq!(json, format!("\"{text}\""));
            let decoded: DecimalText = serde_json::from_str(&json).unwrap();
            assert_eq!(
                decoded.as_inner().mantissa(),
                original.as_inner().mantissa()
            );
            assert_eq!(decoded.as_inner().scale(), original.as_inner().scale());
            assert_eq!(
                decoded.as_inner().is_sign_negative(),
                original.as_inner().is_sign_negative()
            );
        }
        assert_eq!(
            DecimalText::from_text("1.0").unwrap(),
            DecimalText::from_text("1.00").unwrap()
        );
    }

    #[test]
    fn decimal_native_negative_zero_and_numeric_membership_survive() {
        let mut native = Decimal::from_parts(0, 0, 0, false, 28);
        native.set_sign_negative(true);
        let wrapped = DecimalText::try_from(native).unwrap();
        let text = serde_json::to_string(&wrapped).unwrap();
        assert_eq!(text, "\"-0.0000000000000000000000000000\"");
        let decoded: DecimalText = serde_json::from_str(&text).unwrap();
        assert!(decoded.as_inner().is_sign_negative());
        assert_eq!(decoded.as_inner().scale(), 28);
        let a = DecimalText::from_text("1.0").unwrap();
        let b = DecimalText::from_text("1.00").unwrap();
        assert_eq!(HashSet::from([a, b]).len(), 1);
        assert_eq!(BTreeSet::from([a, b]).len(), 1);
    }

    #[test]
    fn decimal_rejects_loss_and_native_parser_conveniences() {
        for text in [
            "+1",
            "1e2",
            "1_000",
            " 1",
            "1 ",
            ".1",
            "1.",
            "--1",
            "1.2.3",
            "0.00000000000000000000000000001",
            "79228162514264337593543950336",
            "79228162514264337593543950335.0",
        ] {
            assert!(DecimalText::from_text(text).is_err(), "accepted {text}");
        }
    }
}
