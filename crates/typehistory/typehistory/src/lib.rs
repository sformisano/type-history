//! Standalone field histories with exact historical decoding.
//!
//! `#[history]` replaces a named record inventory with retained version structs
//! and a current alias implementing [`HasHistory`]. [`decode`] checks the stored
//! identity and version before parsing bytes and applying adjacent conversions.
//!
//! A standalone library requires `typehistory.toml`, a build dependency on
//! `typehistory-build`, and `typehistory_build::compile()` in `build.rs`.
//! Initialize its empty ledger with `cargo typehistory init` before declaring
//! histories. The [guide](https://github.com/sformisano/typehistory/blob/main/crates/typehistory/README.md)
//! contains executable setup, field grammar, lifecycle commands, and safe-error boundaries.
//!
//! Validate stored identity and version metadata before selecting a history:
//!
//! ```
//! # use std::error::Error;
//! use typehistory::{PayloadVersion, TypeIdentity};
//!
//! let identity = TypeIdentity::try_from("billing.invoice.issued".to_owned())?;
//! let version = PayloadVersion::try_from(1_u32)?;
//! assert_eq!(identity.as_str(), "billing.invoice.issued");
//! assert_eq!(version.get(), 1);
//! assert!(PayloadVersion::try_from(0_u32).is_err());
//! # Ok::<(), Box<dyn Error>>(())
//! ```
//!
//! [`ResolvedSchema`] describes structural wire types. [`JsonSchemaField`] and
//! [`FieldSchema`] adapt those field types for schema export. The [`Schema`]
//! derive supplies the field contract for supporting named records.

extern crate self as typehistory;

pub use typehistory_core::resolved::{FieldSchema, JsonSchemaField, ResolvedSchema};
pub use typehistory_core::{
    decode, DecodeContext, DecodeError, DecodeFailureKind, HasHistory, History,
    InvalidPayloadVersion, InvalidTypeIdentity, PayloadVersion, ReadError, TypeIdentity,
};
pub use typehistory_macros::{history, Schema};

/// Dependencies addressed by generated code.
#[doc(hidden)]
pub mod __private {
    pub use typehistory_core::resolved::*;
    pub use typehistory_core::*;
}

#[cfg(test)]
mod tests {
    use super::{ResolvedSchema, Schema};
    use typehistory_core::resolved::{export_json_schema, FieldPresence, SchemaField, SchemaShape};
    use typehistory_core::serde_json::Value;

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
