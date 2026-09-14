//! Parse field histories and generate Rust types, conversions, and schema checks.
//!
//! A procedural macro or framework that calls this crate is a *frontend*. It
//! supplies the authored record and the package's frozen ledger. [`HistoryPlan`]
//! reconstructs the versions from field attributes; checking that plan against
//! the ledger produces an [`AuthorizedHistory`] ready for generation.
//!
//! [`generate_history`] emits numbered structs, the current alias, adjacent
//! conversions, a JSON decoder, and frozen schema checks. Use
//! [`generate::generate_versioned`] alongside it to support the `Versioned` wrapper.
//! [`GenerationPaths`] tells both generators where to find runtime support and
//! how to construct conversion errors.
#![forbid(unsafe_code)]
#![warn(missing_docs)]
pub mod admission;
pub mod canonical;
pub mod diagnostics;
pub mod generate;
pub mod history;
pub mod identifier;
pub mod json_schema;
pub mod json_schema_derive;
pub mod ledger;
pub mod lint_attributes;
pub mod model;
pub mod record;
pub mod resolved_schema;
pub use generate::{generate_history, GeneratedHistory, GeneratedVersion, GenerationPaths};
pub use history::{AuthorizedHistory, HistoryPlan};
pub use identifier::normalize_rust_identifier;
pub use model::{NamedField, RecordInput};

pub mod input;
pub mod source;
