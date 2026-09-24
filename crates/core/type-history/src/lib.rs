//! Versioned Rust records with explicit field conversions and frozen schema checks.
//!
//! A shop starts by storing receipts with an amount in US cents. Later, it adds a
//! currency field. New code needs that field, but old receipts still contain only
//! the amount. Type History keeps the old record shape and runs your conversion
//! to supply `"USD"` when those receipts are read.
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
//! After [setting up a library package](https://github.com/sformisano/type-history/blob/v0.3.1/book/src/setup.md),
//! start the receipt history with a stable name:
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
//! Frozen schemas protect the stored shape; your conversion functions and tests
//! must preserve what the values mean.
//!
//! Conversions run from older versions to the current version. A consumer built
//! with only V1 rejects a V2 receipt. Services sharing a types crate must therefore
//! coordinate which versions they publish and accept; see the
//! [distributed-system walkthrough](https://github.com/sformisano/type-history/blob/v0.3.1/README.md#using-type-history-in-a-distributed-system).
//!
//! Install the matching 0.3.1 crates from crates.io. The [package setup guide](https://github.com/sformisano/type-history/blob/v0.3.1/book/src/setup.md)
//! shows the matching runtime, build hook, and CLI configuration.
//! The guide configures schema checks and initializes the ledger before any history declaration.
//! The current setup requires Rust 1.97 or later. Linux is the tested lifecycle host.
//! Windows and macOS have not been validated.
//! The [book](https://github.com/sformisano/type-history/blob/v0.3.1/book/README.md)
//! links to field rules, lifecycle commands, and error handling.
//!
//! For existing storage that keeps JSON payloads and metadata separately, the
//! low-level [`decode`] API reads the payload using that metadata. Validate it
//! when reading it from storage:
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
//! A receipt may contain another record or an enum. The [`Schema`] derive
//! describes how those supporting types are serialized so changes can be checked
//! against frozen schemas. It supports named records, nonempty tuple structs,
//! and externally tagged enums, including inside supported containers.
//! Keep earlier enum definitions when a containing field
//! changes type; `#[versioned]` remains restricted to concrete named-field structs.
//! [`ResolvedSchema`], [`JsonSchemaField`], and [`FieldSchema`] provide the
//! underlying schema description and export traits.
//!
//! String-keyed hash/tree maps share a storage contract. Sets carry an explicit
//! [`SetMembership`] contract and differ from vectors. Custom membership IDs are
//! trusted declarations of equality; schema derivation does not verify custom
//! `Eq`, `Hash`, or `Ord`. A membership change requires a versioned field update.
//! Bare tuples support arities 1 through 16. Owned `Box` values preserve the inner
//! contract; feature `rc` adds `Rc` and `Arc` by value.
//! Histories containing tuples above arity 12 can disable generated native traits
//! with `derive_debug = false` and `derive_partial_eq = false`.
//!
//! Optional features `typed-floats`, `uuid`, `rust-decimal`, `chrono`, and `time`
//! add finite numeric types and checked [`adapters`]. Defaults are empty. Adapters
//! own their serialization, so native dependencies' Serde features do not change
//! stored profiles. See the [field integration guide](https://github.com/sformisano/type-history/blob/v0.3.1/book/src/integration.md).

extern crate self as type_history;

pub use type_history_core::resolved::{
    ConstantMembership, FieldSchema, JsonSchemaField, ResolvedSchema, SetMembership,
};
pub use type_history_core::{
    adapters, decode, DecodeContext, DecodeError, DecodeFailureKind, HasHistory, History,
    InvalidPayloadVersion, InvalidStableName, PayloadVersion, ReadError, StableName, Versioned,
};
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
