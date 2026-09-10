//! Runtime decoding and structural wire contracts shared by history frontends.

#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![warn(rustdoc::broken_intra_doc_links)]

pub mod canonical_json;
mod error;
mod history;
mod identity;
pub mod resolved;

pub use error::{decode, DecodeError, HasHistory, ReadError};
pub use history::{DecodeContext, DecodeFailureKind, History};
pub use identity::{InvalidPayloadVersion, InvalidTypeIdentity, PayloadVersion, TypeIdentity};
pub use resolved::{
    ConstantShape, FieldSchema, JsonSchemaField, ResolvedSchema, SchemaShape, WireNode,
};

#[doc(hidden)]
pub use {schemars, serde, serde_json};
