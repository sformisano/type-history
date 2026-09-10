use history_api::redactable::{BypassTextRedaction, RedactedValue, ToRedacted};
use std::cell::Cell;
use std::error::Error;
use std::fmt::{Debug, Display, Formatter, Result as FmtResult};

thread_local! { static CALLS: Cell<u32> = const { Cell::new(0) }; }
const SECRET: &str = "STORED_RAW_SECRET_984";

pub struct ConversionError;
impl Debug for ConversionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        formatter.write_str(SECRET)
    }
}
impl Display for ConversionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        formatter.write_str(SECRET)
    }
}
impl Error for ConversionError {}
impl ToRedacted for ConversionError {
    fn to_redacted(&self) -> RedactedValue {
        BypassTextRedaction("safe stored conversion rejection".to_owned()).to_redacted()
    }
}

#[history_api::versioned(stable_id = "example.stored.secret", redacted)]
pub struct Secure {
    #[sensitive]
    #[history(updated_at = v2, previous_type = String, cast_from_previous_value = convert)]
    pub value: u32,
}

fn convert(_: String) -> Result<u32, ConversionError> {
    CALLS.with(|calls| calls.set(calls.get() + 1));
    Err(ConversionError)
}

#[cfg(test)]
mod stored_redacted_tests {
    use super::{CALLS, SECRET, Secure};
    use history_api::redactable::ToRedacted;
    use history_api::{DecodeFailureKind, Versioned};
    use std::error::Error;

    #[test]
    fn raw_historical_storage_and_safe_delayed_conversion_are_separate() {
        CALLS.with(|calls| calls.set(0));
        let wire = format!(r#"{{"example.stored.secret":{{"1":{{"value":"{SECRET}"}}}}}}"#);
        let record: Versioned<Secure> = serde_json::from_str(&wire).unwrap();
        assert_eq!(serde_json::to_string(&record).unwrap(), wire);
        assert!(!format!("{record:?}").contains(SECRET));
        CALLS.with(|calls| assert_eq!(calls.get(), 0));
        let error = Secure::from_versioned(record).unwrap_err();
        CALLS.with(|calls| assert_eq!(calls.get(), 1));
        assert_eq!(error.kind(), DecodeFailureKind::Upcast);
        assert_eq!(error.source_version().get(), 1);
        assert_eq!(error.adjacent_from().unwrap().get(), 1);
        assert_eq!(error.adjacent_to().unwrap().get(), 2);
        assert!(error.source().is_none());
        assert_eq!(
            error.redacted_cause().unwrap().text(),
            "safe stored conversion rejection"
        );
        for rendered in [
            format!("{error}"),
            format!("{error:?}"),
            serde_json::to_string(&error).unwrap(),
            error.to_redacted().text().to_owned(),
            error.to_redacted().json().to_string(),
        ] {
            assert!(!rendered.contains(SECRET), "leaked {rendered}");
        }
    }

    #[test]
    fn malformed_redacted_envelopes_discard_parser_details_without_conversion() {
        CALLS.with(|calls| calls.set(0));
        for wire in [
            format!(r#"{{"example.stored.secret":{{"2":{{"value":"{SECRET}"}}}}}}"#),
            format!(r#"{{"example.stored.secret":{{"1":{{"value":"ok","{SECRET}":true}}}}}}"#),
            format!(r#"{{"example.stored.secret":{{"1":{{"value":"{SECRET}"}},"2":{{}}}}}}"#),
            format!(r#"{{"{SECRET}":{{"1":{{"value":"ok"}}}}}}"#),
        ] {
            let error = serde_json::from_str::<Versioned<Secure>>(&wire).unwrap_err();
            assert!(
                error
                    .to_string()
                    .contains("invalid redacted versioned value")
            );
            assert!(!error.to_string().contains(SECRET));
            assert!(!format!("{error:?}").contains(SECRET));
            CALLS.with(|calls| assert_eq!(calls.get(), 0));
        }
    }
}
