//! Ordinary Cargo build enforcement and explicit type history lifecycle operations.
//!
//! Call [`compile()`] from a standalone consumer's `build.rs`.
//! The committed `type-history/schemas.json` ledger protects retained wire shapes.
//! Initialize it before declaring histories. Ordinary builds validate source,
//! inventory, and consuming-profile readiness without rewriting authority.
//! `TYPE_HISTORY_REQUIRE_FROZEN=1` and `AXIS_REQUIRE_FROZEN_EVENTS=1` strengthen
//! development checks; release-derived profiles always reject drafts.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::error::Error;
use std::result::Result as StandardResult;

/// Build or authoring operation result with its original actionable failure.
pub type Result<T> = StandardResult<T, Box<dyn Error>>;

pub use compile::compile;
pub use type_history_codegen::ledger::HistoryLedger;

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

mod export;
mod transaction;
