use type_history_core::resolved::{
    FieldPresence, Membership, SchemaField, SchemaShape, SchemaVariant, SchemaVariantShape,
};

use crate::json_schema::{JsonSchemaDocument, JsonSchemaErrorReason};

const IDENTITY: &str = "urn:typehistory:schema:writer.inputs";

#[test]
fn malformed_shapes_return_errors_before_lossy_or_panicking_writes() {
    let field = SchemaField {
        name: "same".to_owned(),
        presence: FieldPresence::Optional,
        schema: SchemaShape::String,
    };
    let variant = SchemaVariant {
        name: "same".to_owned(),
        shape: SchemaVariantShape::Unit,
    };
    let malformed = [
        (
            SchemaShape::Tuple { items: vec![] },
            JsonSchemaErrorReason::Tuple,
        ),
        (
            SchemaShape::Record {
                fields: vec![field.clone(), field],
            },
            JsonSchemaErrorReason::DuplicateField,
        ),
        (
            SchemaShape::Enum {
                variants: vec![variant.clone(), variant],
            },
            JsonSchemaErrorReason::Enum,
        ),
        (
            SchemaShape::Enum {
                variants: vec![SchemaVariant {
                    name: "tuple".to_owned(),
                    shape: SchemaVariantShape::Tuple { items: vec![] },
                }],
            },
            JsonSchemaErrorReason::Tuple,
        ),
        (
            SchemaShape::Set {
                value: Box::new(SchemaShape::String),
                membership: Membership {
                    id: "valid-parent".to_owned(),
                    parameters: vec![Membership {
                        id: String::new(),
                        parameters: vec![],
                    }],
                },
            },
            JsonSchemaErrorReason::Membership,
        ),
    ];
    for (shape, reason) in malformed {
        for candidate in [
            shape.clone(),
            SchemaShape::Map {
                value: Box::new(shape),
            },
        ] {
            assert_eq!(
                JsonSchemaDocument::try_from_shape(&candidate, IDENTITY)
                    .unwrap_err()
                    .reason,
                reason
            );
        }
    }
}

#[test]
fn admitted_empty_names_enums_and_equivalent_representations_remain_valid() {
    let cases = [
        SchemaShape::Enum { variants: vec![] },
        SchemaShape::Enum {
            variants: vec![SchemaVariant {
                name: String::new(),
                shape: SchemaVariantShape::Unit,
            }],
        },
        SchemaShape::Record {
            fields: vec![SchemaField {
                name: String::new(),
                presence: FieldPresence::Required,
                schema: SchemaShape::Bool,
            }],
        },
        SchemaShape::Sequence {
            value: Box::new(SchemaShape::U8),
        },
        SchemaShape::Enum {
            variants: vec![SchemaVariant {
                name: "arbitrary/~ name".to_owned(),
                shape: SchemaVariantShape::Newtype {
                    schema: Box::new(SchemaShape::Tuple {
                        items: vec![SchemaShape::U8],
                    }),
                },
            }],
        },
    ];
    for shape in cases {
        let document = JsonSchemaDocument::try_from_shape(&shape, IDENTITY).unwrap();
        assert_eq!(
            document,
            JsonSchemaDocument::from_export(document.as_value().clone(), IDENTITY).unwrap()
        );
    }
    assert_eq!(
        JsonSchemaDocument::try_from_shape(
            &SchemaShape::Sequence {
                value: Box::new(SchemaShape::U8)
            },
            IDENTITY
        )
        .unwrap(),
        JsonSchemaDocument::try_from_shape(&SchemaShape::Bytes, IDENTITY).unwrap(),
    );
}

#[test]
fn duplicate_names_are_reported_at_escaped_document_paths() {
    let field = SchemaField {
        name: "a/~b".to_owned(),
        presence: FieldPresence::Optional,
        schema: SchemaShape::Bool,
    };
    let error = JsonSchemaDocument::try_from_shape(
        &SchemaShape::Record {
            fields: vec![field.clone(), field],
        },
        IDENTITY,
    )
    .unwrap_err();
    assert_eq!(error.path, "/properties/a~1~0b");
}
