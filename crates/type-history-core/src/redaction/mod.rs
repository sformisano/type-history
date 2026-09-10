//! Optional redacted histories. Ordinary [`crate::HasHistory`] remains unchanged.
//!
//! Enable `redactable` on the runtime, build hook, and CLI, then select
//! `#[versioned(stable_name = "example.record.created", redacted)]`. Use [`HasRedactedHistory`]
//! and [`decode`] for these histories. The selected Redactable release is 0.12.0.
//!
//! Callback errors can declare a safe projection without standard error traits:
//!
//! ```
//! use type_history_core::{PayloadVersion, StableName};
//! use type_history_core::redactable::{BypassTextRedaction, RedactedValue, ToRedacted};
//! use type_history_core::redaction::RedactedDecodeError;
//! struct Rejected;
//! impl ToRedacted for Rejected {
//!     fn to_redacted(&self) -> RedactedValue {
//!         BypassTextRedaction("conversion rejected".to_owned()).to_redacted()
//!     }
//! }
//! let failure = RedactedDecodeError::upcast(
//!     StableName::new("billing.receipt.recorded"), PayloadVersion::INITIAL,
//!     PayloadVersion::INITIAL, PayloadVersion::try_from(2).unwrap(), Rejected,
//! );
//! assert_eq!(failure.redacted_cause().unwrap().text(), "conversion rejected");
//! ```

mod error;
mod schema;
#[cfg(test)]
mod tests;

use crate::{History, PayloadVersion, StableName};
pub use error::{RedactedDecodeError, RedactedReadError};

/// Current record whose generated decoder captures only redacted failures.
pub trait HasRedactedHistory: Sized {
    /// Stable name shared by every version of this type.
    const STABLE_NAME: StableName;
    /// Positive current payload version.
    const VERSION: PayloadVersion;
    /// Exact retained-version decoder and adjacent conversions.
    fn history() -> History<Self, RedactedDecodeError>;
}

/// Check stable name, decode the historical record, and return the current type.
///
/// This separate entry point never changes ordinary [`crate::decode`] errors.
pub fn decode<T: HasRedactedHistory>(
    stable_name: &StableName,
    version: PayloadVersion,
    bytes: &[u8],
) -> Result<T, RedactedReadError> {
    if stable_name != &T::STABLE_NAME {
        return Err(RedactedReadError::TypeMismatch {
            expected: T::STABLE_NAME,
            found: stable_name.clone(),
        });
    }
    T::history()
        .decode(version, bytes)
        .map_err(RedactedReadError::Decode)
}
