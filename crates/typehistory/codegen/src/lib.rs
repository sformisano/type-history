//! Shared type-history parsing, authority, schema comparison and Rust generation.
#![forbid(unsafe_code)]
#![warn(missing_docs)]
pub mod binding_contract;
pub mod canonical;
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
pub mod sensitivity;
pub use generate::{generate_history, GeneratedHistory, GeneratedVersion, GenerationPaths};
pub use history::{AuthorizedHistory, HistoryPlan};
pub use identifier::normalize_rust_identifier;
pub use model::{NamedField, RecordInput};

pub mod input;
pub mod source;
