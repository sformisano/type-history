//! Standalone CLI for explicit, checked field-history lifecycle operations.
#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::ffi::OsString;
use typehistory_build::contract::STANDALONE;
use typehistory_build::lifecycle::{self, LifecycleOps};
use typehistory_build::{discover, standalone_export};

/// CLI failure with an actionable explanation.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct CliError(String);

/// Run an explicit lifecycle operation or a read-only workspace check.
pub fn run(arguments: impl IntoIterator<Item = OsString>) -> Result<(), CliError> {
    lifecycle::run(
        arguments,
        &STANDALONE,
        &LifecycleOps {
            discover: discover::read,
            decode_export: standalone_export::decode,
        },
    )
    .map_err(|error| CliError(error.to_string()))
}
