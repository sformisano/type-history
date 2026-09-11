//! Versioned Rust records with explicit field conversions and frozen schema checks.
//!
//! `#[versioned]` generates a Rust struct for each supported version of a record.
//! A new `ReceiptCreated` history starts with `ReceiptCreatedV1`. Later versions
//! add `ReceiptCreatedV2`, `ReceiptCreatedV3`, and so on. The original name aliases
//! the latest struct: at V2, the macro generates `pub type ReceiptCreated = ReceiptCreatedV2;`.
//!
//! Use the alias's `into_versioned` method to create a [`Versioned`] containing
//! the data and its generated metadata.
//! Serialize that record with your chosen Serde format and storage. After
//! deserialization, the alias's `from_versioned` method applies the declared
//! conversions and returns the latest type. Application code reads and writes
//! through that alias.
//!
//! Import the attribute and declare its stable name in an initialized library:
//!
//! ```rust,ignore
//! use type_history::versioned;
//!
//! #[versioned(stable_name = "shop.receipt.created")]
//! pub struct ReceiptCreated {
//!     pub amount_cents: u64,
//! }
//! ```
//!
//! The stable name is frozen with the schemas. Builds reject changes to that value,
//! even when the Rust type or its module is renamed.
//!
//! Add the runtime and build hook to `Cargo.toml`:
//!
//! ```toml
//! [dependencies]
//! type-history = "0.1"
//!
//! [build-dependencies]
//! type-history-build = "0.1"
//! ```
//!
//! Install the CLI with `cargo install cargo-type-history --version 0.1 --locked`.
//! Configure `type-history.toml` and call `type_history_build::compile()` from `build.rs`.
//! Before declaring histories, initialize the empty ledger with
//! `cargo type-history init --package YOUR_PACKAGE`. Replace `YOUR_PACKAGE` with
//! the library package's name. The [setup guide](https://github.com/sformisano/type-history/blob/main/book/src/setup.md)
//! walks through these steps. The [book](https://github.com/sformisano/type-history/blob/main/book/README.md)
//! links to field rules, lifecycle commands, and error handling.
//!
//! For existing storage that keeps JSON payloads and metadata separately, the
//! low-level [`decode`] API remains available. Validate metadata at that boundary:
//!
//! ```
//! # use std::error::Error;
//! use type_history::{PayloadVersion, StableName};
//!
//! let stable_name = StableName::try_from("billing.invoice.issued".to_owned())?;
//! let version = PayloadVersion::try_from(1_u32)?;
//! assert_eq!(stable_name.as_str(), "billing.invoice.issued");
//! assert_eq!(version.get(), 1);
//! assert!(PayloadVersion::try_from(0_u32).is_err());
//! # Ok::<(), Box<dyn Error>>(())
//! ```
//!
//! [`ResolvedSchema`] describes structural wire types. [`JsonSchemaField`] and
//! [`FieldSchema`] adapt those field types for schema export. The [`Schema`]
//! derive supplies the field contract for supporting named records.
//!
//! The optional `redactable` feature enables explicit `#[versioned(..., redacted)]`
//! histories. Enable it on the build hook and CLI too. These histories use the
//! same record methods with separate safe conversion errors. Their low-level
//! JSON API is `redaction::decode`. Ordinary histories retain their error
//! contracts in every feature mode.

extern crate self as type_history;

pub use type_history_core::resolved::{FieldSchema, JsonSchemaField, ResolvedSchema};
pub use type_history_core::{
    decode, DecodeContext, DecodeError, DecodeFailureKind, HasHistory, History,
    InvalidPayloadVersion, InvalidStableName, PayloadVersion, ReadError, StableName, Versioned,
};
#[cfg(feature = "redactable")]
pub use type_history_core::{redactable, redaction};
pub use type_history_macros::{versioned, Schema};

/// Dependencies addressed by generated code.
#[doc(hidden)]
pub mod __private {
    pub use type_history_core::resolved::*;
    pub use type_history_core::*;
}

#[cfg(test)]
mod tests {
    use super::{ResolvedSchema, Schema};
    use type_history_core::resolved::{
        export_json_schema, FieldPresence, SchemaField, SchemaShape,
    };
    use type_history_core::serde_json::Value;

    // These authored records exercise the supporting derive without adding codecs.
    #[allow(dead_code)]
    #[derive(Schema)]
    struct Address {
        number: u32,
    }

    #[allow(dead_code)]
    #[derive(Schema)]
    // A public record may keep the nominal type of a private field private.
    pub struct Envelope {
        address: Option<Address>,
    }

    #[test]
    fn schema_derive_resolves_nested_record_fields_and_exports_their_closed_shape() {
        assert_eq!(
            Envelope::resolved_wire_schema(),
            SchemaShape::Record {
                fields: vec![SchemaField {
                    name: "address".to_owned(),
                    presence: FieldPresence::Optional,
                    schema: SchemaShape::Option {
                        value: Box::new(SchemaShape::Record {
                            fields: vec![SchemaField {
                                name: "number".to_owned(),
                                presence: FieldPresence::Required,
                                schema: SchemaShape::U32,
                            }],
                        }),
                    },
                }],
            }
        );
        let exported = export_json_schema::<Envelope>();
        assert_eq!(exported["additionalProperties"], false);
        let address = &exported["properties"]["address"];
        // Raw schemas can express nullability through either types or alternatives.
        let alternatives = address["anyOf"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_else(|| std::slice::from_ref(address));
        let has_type = |shape: &Value, expected: &str| {
            shape["type"] == expected
                || shape["type"]
                    .as_array()
                    .is_some_and(|types| types.iter().any(|ty| ty == expected))
        };
        assert!(alternatives.iter().any(|shape| has_type(shape, "null")));
        let address = alternatives
            .iter()
            .find(|shape| has_type(shape, "object"))
            .expect("nested record shape");
        assert_eq!(address["additionalProperties"], false);
        assert_eq!(address["properties"]["number"]["type"], "integer");
        assert_eq!(address["required"][0], "number");
    }
}
