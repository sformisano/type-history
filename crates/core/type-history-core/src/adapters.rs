//! Checked field adapters with stable storage contracts.
//!
//! `uuid`, `rust-decimal`, `chrono`, and `time` enable their respective wrappers.
//! Their dependencies require only `std`; these wrappers never call native Serde.
//! `typed-floats` instead enables the dependency's checked numeric Serde support.
//! Every wrapper validates its native value through `TryFrom` and exposes only
//! shared borrowing or ownership recovery after validation.

pub use crate::resolved::ProfileError;

#[macro_use]
mod support;
#[cfg(feature = "uuid")]
mod uuid;
#[cfg(feature = "uuid")]
pub use uuid::UuidText;
#[cfg(feature = "rust-decimal")]
mod decimal;
#[cfg(feature = "rust-decimal")]
pub use decimal::DecimalText;
#[cfg(feature = "chrono")]
pub mod chrono;
#[cfg(all(test, feature = "chrono", feature = "time"))]
mod cross_tests;
#[cfg(feature = "typed-floats")]
mod floats;
#[cfg(any(feature = "chrono", feature = "time"))]
mod temporal;
#[cfg(feature = "time")]
pub mod time;
