use super::normalize::normalize;
use super::{parse, presence_of, record, JsonSchemaDocument, JsonSchemaErrorReason};
use crate::canonical;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use type_history_core::resolved::{
    FieldPresence, SchemaField, SchemaShape, SchemaVariant, SchemaVariantShape,
};

const IDENTITY: &str = "urn:typehistory:schema:billing.account.record";

fn field(name: &str, schema: SchemaShape) -> SchemaField {
    SchemaField {
        name: name.to_owned(),
        presence: presence_of(&schema),
        schema,
    }
}

fn every_node_kind() -> SchemaShape {
    SchemaShape::Record {
        fields: vec![
            field("flag", SchemaShape::Bool),
            field("text", SchemaShape::String),
            field("i8", SchemaShape::I8),
            field("i16", SchemaShape::I16),
            field("i32", SchemaShape::I32),
            field("i64", SchemaShape::I64),
            field("i128", SchemaShape::I128),
            field("u8", SchemaShape::U8),
            field("u16", SchemaShape::U16),
            field("u32", SchemaShape::U32),
            field("u64", SchemaShape::U64),
            field("u128", SchemaShape::U128),
            field("bytes", SchemaShape::Bytes),
            field(
                "optional",
                SchemaShape::Option {
                    value: Box::new(SchemaShape::U64),
                },
            ),
            field(
                "sequence",
                SchemaShape::Sequence {
                    value: Box::new(SchemaShape::String),
                },
            ),
            field(
                "array",
                SchemaShape::Array {
                    value: Box::new(SchemaShape::U8),
                    length: 4,
                },
            ),
            field(
                "nested",
                SchemaShape::Record {
                    fields: vec![field(
                        "inner",
                        SchemaShape::Option {
                            value: Box::new(SchemaShape::Record { fields: vec![] }),
                        },
                    )],
                },
            ),
            field(
                "unit_only",
                SchemaShape::Enum {
                    variants: vec![
                        SchemaVariant {
                            name: "B".to_owned(),
                            shape: SchemaVariantShape::Unit,
                        },
                        SchemaVariant {
                            name: "A".to_owned(),
                            shape: SchemaVariantShape::Unit,
                        },
                    ],
                },
            ),
            field(
                "single_unit",
                SchemaShape::Enum {
                    variants: vec![SchemaVariant {
                        name: "Only".to_owned(),
                        shape: SchemaVariantShape::Unit,
                    }],
                },
            ),
            field("choice", choice()),
        ],
    }
}

fn choice() -> SchemaShape {
    SchemaShape::Enum {
        variants: vec![
            SchemaVariant {
                name: "Disabled".to_owned(),
                shape: SchemaVariantShape::Unit,
            },
            SchemaVariant {
                name: "Count".to_owned(),
                shape: SchemaVariantShape::Newtype {
                    schema: Box::new(SchemaShape::U64),
                },
            },
            SchemaVariant {
                name: "Pair".to_owned(),
                shape: SchemaVariantShape::Tuple {
                    items: vec![SchemaShape::U64, SchemaShape::String],
                },
            },
            SchemaVariant {
                name: "Named".to_owned(),
                shape: SchemaVariantShape::Record {
                    fields: vec![field(
                        "value",
                        SchemaShape::Option {
                            value: Box::new(SchemaShape::String),
                        },
                    )],
                },
            },
        ],
    }
}

#[test]
fn every_node_kind_round_trips_through_the_written_document() {
    let shape = every_node_kind().normalized();
    let document = JsonSchemaDocument::from_shape(&shape, IDENTITY);
    assert_eq!(document.identity(), IDENTITY);
    assert_eq!(document.shape(), shape);
    assert_eq!(
        JsonSchemaDocument::from_normalized(document.as_value().clone()).expect("stored form"),
        document
    );
}

#[test]
fn written_documents_are_fixed_points_of_normalization() {
    let document = JsonSchemaDocument::from_shape(&every_node_kind(), IDENTITY);
    let mut value = document.as_value().clone();
    normalize(&mut value).expect("normalizes");
    assert_eq!(&value, document.as_value());
}

#[test]
fn reference_state_document_matches_the_agreed_form() {
    let shape = record(BTreeMap::from([
        ("country_code".to_owned(), SchemaShape::String),
        ("credited_funds_minor".to_owned(), SchemaShape::U128),
        ("currency_code".to_owned(), SchemaShape::String),
        ("debited_funds_minor".to_owned(), SchemaShape::U128),
        ("owner_display_name".to_owned(), SchemaShape::String),
    ]));
    let document = JsonSchemaDocument::from_shape(&shape, IDENTITY);
    assert_eq!(
        document.as_value(),
        &json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "$id": "urn:typehistory:schema:billing.account.record",
            "type": "object",
            "properties": {
                "country_code": {"type": "string"},
                "credited_funds_minor": {"type": "integer", "format": "uint128", "minimum": 0},
                "currency_code": {"type": "string"},
                "debited_funds_minor": {"type": "integer", "format": "uint128", "minimum": 0},
                "owner_display_name": {"type": "string"}
            },
            "required": [
                "country_code",
                "credited_funds_minor",
                "currency_code",
                "debited_funds_minor",
                "owner_display_name"
            ],
            "additionalProperties": false
        })
    );
}

#[test]
fn normalization_rewrites_every_schemars_optional_encoding_to_one_wrapper() {
    let raw = json!({
        "title": "Root",
        "description": "doc comment",
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "typed": {"type": ["integer", "null"], "format": "uint64", "minimum": 0},
            "listed": {"type": ["string", "null"], "enum": ["B", "A", null]},
            "constant": {"type": ["string", "null"], "enum": ["Only", null]},
            "wrapped": {"anyOf": [{"oneOf": [{"type": "string", "const": "Unit"}]}, {"type": "null"}]},
            "present": {"type": "string", "description": "field doc"}
        },
        "required": ["present", "typed", "listed", "constant", "wrapped"]
    });
    let document = JsonSchemaDocument::from_export(raw, IDENTITY).expect("normalizes");
    let optional = |inner: Value| json!({"anyOf": [inner, {"type": "null"}]});
    assert_eq!(
        document.as_value(),
        &json!({
            "$schema": JsonSchemaDocument::META_SCHEMA,
            "$id": IDENTITY.to_owned(),
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "typed": optional(json!({"type": "integer", "format": "uint64", "minimum": 0})),
                "listed": optional(json!({"type": "string", "enum": ["A", "B"]})),
                "constant": optional(json!({"const": "Only"})),
                "wrapped": optional(json!({"oneOf": [{"const": "Unit"}]})),
                "present": {"type": "string"}
            },
            "required": ["present"]
        })
    );
    let shape = document.shape();
    let SchemaShape::Record { fields } = &shape else {
        panic!("record");
    };
    assert_eq!(
        fields
            .iter()
            .map(|field| (field.name.as_str(), field.presence))
            .collect::<Vec<_>>(),
        vec![
            ("constant", FieldPresence::Optional),
            ("listed", FieldPresence::Optional),
            ("present", FieldPresence::Required),
            ("typed", FieldPresence::Optional),
            ("wrapped", FieldPresence::Optional),
        ]
    );
}

#[test]
fn normalization_splits_grouped_unit_variants_and_sorts_variants() {
    let raw = json!({
        "$schema": JsonSchemaDocument::META_SCHEMA,
        "$id": IDENTITY.to_owned(),
        "oneOf": [
            {"type": "object", "properties": {"Count": {"type": "integer", "format": "uint64", "minimum": 0}}, "required": ["Count"], "additionalProperties": false},
            {"type": "string", "enum": ["Z", "Disabled"]}
        ]
    });
    let mut value = raw;
    normalize(&mut value).expect("normalizes");
    let entries = value["oneOf"].as_array().expect("entries").clone();
    assert_eq!(entries[0]["required"], json!(["Count"]));
    assert_eq!(entries[1], json!({"const": "Disabled"}));
    assert_eq!(entries[2], json!({"const": "Z"}));
    let SchemaShape::Enum { variants } = parse(&value).expect("enum") else {
        panic!("enum")
    };
    assert_eq!(
        variants
            .iter()
            .map(|variant| variant.name.as_str())
            .collect::<Vec<_>>(),
        vec!["Count", "Disabled", "Z"]
    );
}

#[test]
fn a_newtype_variant_holding_a_record_reads_as_a_record_variant() {
    let inner = SchemaShape::Record {
        fields: vec![field("value", SchemaShape::String)],
    };
    let newtype = SchemaShape::Enum {
        variants: vec![
            SchemaVariant {
                name: "Built".to_owned(),
                shape: SchemaVariantShape::Newtype {
                    schema: Box::new(inner.clone()),
                },
            },
            SchemaVariant {
                name: "Empty".to_owned(),
                shape: SchemaVariantShape::Unit,
            },
        ],
    };
    let SchemaShape::Record { fields } = inner else {
        panic!("record")
    };
    let record_variant = SchemaShape::Enum {
        variants: vec![
            SchemaVariant {
                name: "Built".to_owned(),
                shape: SchemaVariantShape::Record { fields },
            },
            SchemaVariant {
                name: "Empty".to_owned(),
                shape: SchemaVariantShape::Unit,
            },
        ],
    };
    let written = JsonSchemaDocument::from_shape(&newtype, IDENTITY);
    assert_eq!(
        written,
        JsonSchemaDocument::from_shape(&record_variant, IDENTITY)
    );
    assert_eq!(written.shape(), record_variant);
}

#[test]
fn distinct_shapes_produce_distinct_canonical_bytes() {
    let shapes = [
        SchemaShape::Array {
            value: Box::new(SchemaShape::U8),
            length: 4,
        },
        SchemaShape::Array {
            value: Box::new(SchemaShape::U8),
            length: 5,
        },
        SchemaShape::Bytes,
        SchemaShape::Option {
            value: Box::new(SchemaShape::Bytes),
        },
        SchemaShape::Sequence {
            value: Box::new(SchemaShape::Option {
                value: Box::new(SchemaShape::U8),
            }),
        },
        SchemaShape::Option {
            value: Box::new(SchemaShape::U64),
        },
        SchemaShape::Option {
            value: Box::new(SchemaShape::U128),
        },
        SchemaShape::U8,
        SchemaShape::I8,
        SchemaShape::Enum {
            variants: vec![SchemaVariant {
                name: "A".to_owned(),
                shape: SchemaVariantShape::Unit,
            }],
        },
        SchemaShape::Enum {
            variants: vec![
                SchemaVariant {
                    name: "A".to_owned(),
                    shape: SchemaVariantShape::Record { fields: vec![] },
                },
                SchemaVariant {
                    name: "B".to_owned(),
                    shape: SchemaVariantShape::Unit,
                },
            ],
        },
        SchemaShape::Enum {
            variants: vec![
                SchemaVariant {
                    name: "A".to_owned(),
                    shape: SchemaVariantShape::Unit,
                },
                SchemaVariant {
                    name: "B".to_owned(),
                    shape: SchemaVariantShape::Unit,
                },
            ],
        },
        record(BTreeMap::from([("a".to_owned(), SchemaShape::String)])),
        record(BTreeMap::from([(
            "a".to_owned(),
            SchemaShape::Option {
                value: Box::new(SchemaShape::String),
            },
        )])),
    ];
    let mut seen = BTreeSet::new();
    for shape in &shapes {
        let document = JsonSchemaDocument::from_shape(shape, IDENTITY);
        assert!(
            seen.insert(document.canonical_bytes().expect("canonical")),
            "collision for {shape:?}"
        );
        assert_eq!(&document.shape(), &shape.clone().normalized());
    }
}

#[test]
fn loader_rejects_documents_outside_the_subset() {
    let root = |body: Value| {
        let mut object = body.as_object().expect("object").clone();
        object.insert("$schema".to_owned(), JsonSchemaDocument::META_SCHEMA.into());
        object.insert("$id".to_owned(), IDENTITY.to_owned().into());
        Value::Object(object)
    };
    let cases: Vec<(Value, JsonSchemaErrorReason)> = vec![
        (
            root(json!({"$ref": "#/$defs/x"})),
            JsonSchemaErrorReason::Reference,
        ),
        (
            root(json!({"type": "object", "properties": {}, "required": []})),
            JsonSchemaErrorReason::AdditionalProperties,
        ),
        (
            root(json!({"type": "integer"})),
            JsonSchemaErrorReason::IntegerFormat,
        ),
        (
            root(json!({"type": "integer", "format": "uint8", "minimum": 0, "maximum": 256})),
            JsonSchemaErrorReason::IntegerBounds,
        ),
        (
            root(json!({"type": "integer", "format": "uint64"})),
            JsonSchemaErrorReason::IntegerBounds,
        ),
        (
            root(json!({"type": "string", "pattern": "x"})),
            JsonSchemaErrorReason::UnknownKeyword("pattern".to_owned()),
        ),
        (
            root(
                json!({"type": "array", "items": {"type": "string"}, "minItems": 1, "maxItems": 2}),
            ),
            JsonSchemaErrorReason::ArrayLength,
        ),
        (root(json!({"type": "number"})), JsonSchemaErrorReason::Type),
        (
            root(json!({"oneOf": [{"type": "string"}]})),
            JsonSchemaErrorReason::Variant,
        ),
        (
            root(json!({"anyOf": [{"type": "string"}, {"type": "integer", "format": "int32"}]})),
            JsonSchemaErrorReason::Option,
        ),
    ];
    for (value, expected) in cases {
        let error = JsonSchemaDocument::from_normalized(value.clone()).expect_err("rejected");
        assert_eq!(error.reason, expected, "{value}");
    }

    let mut denormalized = JsonSchemaDocument::from_shape(&every_node_kind(), IDENTITY)
        .as_value()
        .clone();
    denormalized["properties"]["flag"]["title"] = "Flag".into();
    assert_eq!(
        JsonSchemaDocument::from_normalized(denormalized)
            .expect_err("title is stripped")
            .reason,
        JsonSchemaErrorReason::NotNormalized
    );
    assert_eq!(
        JsonSchemaDocument::from_normalized(json!({"type": "string"}))
            .expect_err("missing dialect")
            .reason,
        JsonSchemaErrorReason::MetaSchema
    );
    assert_eq!(
        JsonSchemaDocument::from_normalized(
            json!({"$schema": JsonSchemaDocument::META_SCHEMA, "type": "string"})
        )
        .expect_err("missing identity")
        .reason,
        JsonSchemaErrorReason::Identity
    );
}

#[test]
fn nested_optional_documents_are_rejected() {
    let nested = json!({
        "$schema": JsonSchemaDocument::META_SCHEMA,
        "$id": IDENTITY.to_owned(),
        "anyOf": [{"anyOf": [{"type": "string"}, {"type": "null"}]}, {"type": "null"}]
    });
    assert_eq!(
        JsonSchemaDocument::from_normalized(nested)
            .expect_err("nested option")
            .reason,
        JsonSchemaErrorReason::Option
    );
}

#[test]
fn serde_round_trip_preserves_normalized_bytes() {
    let document = JsonSchemaDocument::from_shape(&every_node_kind(), IDENTITY);
    let bytes = canonical::to_vec(&document).expect("canonical");
    let decoded: JsonSchemaDocument = serde_json::from_slice(&bytes).expect("decodes");
    assert_eq!(decoded, document);
    assert_eq!(bytes, document.canonical_bytes().expect("bytes"));
}
