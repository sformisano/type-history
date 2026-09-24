//! Read stored versions and describe the schemas checked by Type History.
//!
//! [`Versioned`] keeps a record's stable name, version, and data together. Generated
//! code selects the stored type when deserializing it, then applies adjacent
//! conversions when the application requests the current type. [`decode`] handles
//! JSON payloads whose metadata is stored separately.
//!
//! The [`resolved`] module describes serialized field shapes so builds can compare
//! them with frozen schemas. Most applications use these APIs through the
//! `type-history` crate, which also re-exports the declaration macros. Framework
//! and macro authors can use this crate as the runtime for their generated code.

#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![warn(rustdoc::broken_intra_doc_links)]

pub mod adapters;
pub mod canonical_json;
mod error;
mod history;
mod metadata;
pub mod resolved;
mod versioned;

pub use error::{decode, DecodeError, HasHistory, ReadError};
pub use history::{DecodeContext, DecodeFailureKind, History};
pub use metadata::{InvalidPayloadVersion, InvalidStableName, PayloadVersion, StableName};
pub use resolved::{
    ConstantMembership, ConstantShape, FieldSchema, JsonSchemaField, ProfileError, ResolvedSchema,
    SchemaShape, SetMembership, WireNode,
};
pub use versioned::Versioned;
#[doc(hidden)]
pub use versioned::{decode_json_payload, VersionedHistory};

#[doc(hidden)]
pub use {schemars, serde, serde_json};
