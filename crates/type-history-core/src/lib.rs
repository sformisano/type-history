//! Runtime decoding and structural wire contracts shared by history frontends.

#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![warn(rustdoc::broken_intra_doc_links)]

pub mod canonical_json;
mod error;
mod history;
#[cfg(feature = "redactable")]
pub mod redaction;
#[cfg(feature = "redactable")]
pub use redactable;
mod metadata;
pub mod resolved;
mod versioned;

pub use error::{decode, DecodeError, HasHistory, ReadError};
pub use history::{DecodeContext, DecodeFailureKind, History};
pub use metadata::{InvalidPayloadVersion, InvalidStableName, PayloadVersion, StableName};
pub use resolved::{
    ConstantShape, FieldSchema, JsonSchemaField, ResolvedSchema, SchemaShape, WireNode,
};
pub use versioned::Versioned;
#[doc(hidden)]
pub use versioned::VersionedHistory;

#[doc(hidden)]
pub use {schemars, serde, serde_json};
