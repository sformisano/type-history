//! Parse field histories and generate Rust types, conversions, and schema checks.
//!
//! A procedural macro or framework that calls this crate is a *frontend*. It
//! supplies the authored record and the package's frozen ledger. [`HistoryPlan`]
//! infers the current version from field attributes.
//! [`HistoryPlan::expand_authorized`] checks that version against the ledger, then
//! reconstructs every retained version as an [`AuthorizedHistory`].
//!
//! [`generate_history`] emits numbered structs, the current alias, adjacent
//! conversions, a JSON decoder, and frozen schema checks. Use
//! [`generate::generate_versioned`] alongside it to support the `Versioned` wrapper.
//! [`GenerationPaths`] tells both generators where to find runtime support and
//! how to construct conversion errors.
//! [`generate_history_with_options`] lets a frontend own payload traits and
//! select a dedicated observation cfg. [`GeneratedVersion`] exposes typed
//! payload declarations, structural shapes, and descriptive schema expressions.
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
pub use generate::{
    generate_history, generate_history_with_options, GeneratedHistory, GeneratedVersion,
    GenerationOptions, GenerationPaths, PayloadTraits,
};
pub use history::{AuthorizedHistory, HistoryPlan};
pub use identifier::normalize_rust_identifier;
pub use model::{NamedField, RecordDerives, RecordInput};

pub mod input;
pub mod source;
