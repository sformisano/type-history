//! The `cargo type-history` commands for checking and freezing histories.
//!
//! Initialize a package's ledger before declaring its first history. As fields
//! change, use `check` to compare the source with frozen schemas and `freeze` to
//! record a version before storing data with it. `reset` and `import` handle
//! explicit corrections and imported histories.
//!
//! The binary passes its arguments to [`run`]. See the
//! [lifecycle guide](https://github.com/sformisano/type-history/blob/v0.3.1/book/src/lifecycle.md)
//! for commands, draft rules, and reset limits.
#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::ffi::OsString;
use type_history_build::contract::STANDALONE;
use type_history_build::lifecycle::{self, LifecycleOps};
use type_history_build::{discover, standalone_export};

/// CLI failure with an actionable explanation.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct CliError(String);

/// Run an explicit lifecycle operation or a read-only workspace check.
pub fn run(arguments: impl IntoIterator<Item = OsString>) -> Result<(), CliError> {
    lifecycle::run_with_check(
        arguments,
        &STANDALONE,
        &LifecycleOps {
            discover: discover::read,
            decode_export: standalone_export::decode,
        },
    )
    .map_err(|error| CliError(error.to_string()))
}
