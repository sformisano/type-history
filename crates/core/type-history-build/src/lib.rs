//! Check frozen schemas during builds and manage the schema ledger.
//!
//! When a shop freezes its first receipt version, the ledger records that version's
//! serialized shape. Later builds must keep it unchanged so stored receipts remain
//! readable. Call [`compile()`] from the application's or shared types crate's
//! `build.rs` to enable those checks.
//!
//! Initialize `type-history/schemas.json` with `cargo type-history init` before
//! declaring histories, and commit it with the source. Builds check the declared
//! histories against that ledger and configure the macro's schema checks. They
//! do not rewrite the ledger.
//!
//! Development builds warn about drafts. Set `TYPE_HISTORY_REQUIRE_FROZEN=1` to
//! reject them during development too; release-derived profiles always reject
//! drafts. The [`lifecycle`] module provides the checked operations used by the CLI
//! to initialize, check, freeze, reset, and import histories. They reuse compiled
//! dependencies in `type-history-snapshot` inside Cargo's target directory;
//! `TYPE_HISTORY_PRIVATE_SNAPSHOT=1` selects a private temporary snapshot.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::error::Error;
use std::result::Result as StandardResult;

/// Build or authoring operation result with its original actionable failure.
pub type Result<T> = StandardResult<T, Box<dyn Error>>;

pub use compile::compile;
pub use type_history_codegen::ledger::HistoryLedger;

pub mod check_report;
pub mod compile;
pub mod contract;
pub mod discover;
pub mod inventory;
pub mod lifecycle;
pub mod options;
pub mod package;
pub mod snapshot;
pub mod standalone_export;
pub mod workspace;

mod admission;
mod compiler_diagnostics;
mod export;
mod toolchain;
mod transaction;
