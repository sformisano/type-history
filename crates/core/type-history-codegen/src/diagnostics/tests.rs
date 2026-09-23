use serde_json::{json, Value};
use type_history_core::resolved::{
    FieldPresence, Membership, SchemaField, SchemaShape, SchemaVariant, SchemaVariantShape,
    StorageProfile,
};

use super::{compare_shapes, readable_path, DifferenceCode, PathSegment};

fn field(name: &str, schema: SchemaShape) -> SchemaField {
    SchemaField {
        name: name.into(),
        presence: FieldPresence::Required,
        schema,
    }
}
fn record(fields: Vec<SchemaField>) -> SchemaShape {
    SchemaShape::Record { fields }
}
fn enumeration(name: &str, shape: SchemaVariantShape) -> SchemaShape {
    SchemaShape::Enum {
        variants: vec![SchemaVariant {
            name: name.into(),
            shape,
        }],
    }
}

#[test]
fn every_primitive_kind_change_reports_complete_subtrees() {
    let shapes = [
        SchemaShape::Bool,
        SchemaShape::I8,
        SchemaShape::I16,
        SchemaShape::I32,
        SchemaShape::I64,
        SchemaShape::I128,
        SchemaShape::U8,
        SchemaShape::U16,
        SchemaShape::U32,
        SchemaShape::U64,
        SchemaShape::U128,
        SchemaShape::String,
        SchemaShape::Bytes,
    ];
    for expected in &shapes {
        for actual in &shapes {
            let differences = compare_shapes(expected, actual);
            if expected == actual {
                assert!(differences.is_empty());
                continue;
            }
            assert_eq!(differences.len(), 1);
            let difference = &differences[0];
            assert_eq!(difference.code, DifferenceCode::ShapeKindChanged);
            assert!(difference.path.is_empty());
            assert_eq!(difference.expected, serde_json::to_value(expected).unwrap());
            assert_eq!(difference.actual, serde_json::to_value(actual).unwrap());
            assert!(!difference.hint.is_empty());
        }
    }
}

#[test]
fn nested_changes_keep_full_typed_paths_and_independent_siblings() {
    let nested = |scalar| SchemaShape::Option {
        value: Box::new(SchemaShape::Sequence {
            value: Box::new(SchemaShape::Array {
                length: 3,
                value: Box::new(enumeration(
                    "V",
                    SchemaVariantShape::Tuple {
                        items: vec![scalar],
                    },
                )),
            }),
        }),
    };
    let expected = record(vec![
        field("b", SchemaShape::U32),
        field("a", nested(SchemaShape::Bool)),
    ]);
    let actual = record(vec![
        field("a", nested(SchemaShape::String)),
        field("b", SchemaShape::String),
    ]);
    let differences = compare_shapes(&expected, &actual);
    assert_eq!(differences.len(), 2);
    assert_eq!(
        serde_json::to_value(&differences[0].path).unwrap(),
        json!([
            {"kind":"field","name":"a"},{"kind":"option_value"},{"kind":"sequence_item"},
            {"kind":"array_item"},{"kind":"variant","name":"V"},{"kind":"tuple_item","index":0}
        ])
    );
    assert_eq!(
        readable_path(&differences[0].path),
        "$[\"a\"].option_value.sequence_item.array_item.variant[\"V\"][0]"
    );
    assert_eq!(differences, compare_shapes(&expected.normalized(), &actual));
}

#[test]
fn missing_added_presence_and_nested_field_changes_are_aggregated() {
    let expected = record(vec![
        field("missing", record(vec![field("inner", SchemaShape::Bool)])),
        field("value", SchemaShape::U32),
    ]);
    let actual = record(vec![
        field("added", SchemaShape::Bytes),
        SchemaField {
            name: "value".into(),
            presence: FieldPresence::Optional,
            schema: SchemaShape::Option {
                value: Box::new(SchemaShape::U32),
            },
        },
    ]);
    let differences = compare_shapes(&expected, &actual);
    assert_eq!(
        differences
            .iter()
            .map(|value| value.code)
            .collect::<Vec<_>>(),
        vec![
            DifferenceCode::FieldAdded,
            DifferenceCode::FieldMissing,
            DifferenceCode::FieldPresenceChanged,
            DifferenceCode::ShapeKindChanged
        ]
    );
    assert_eq!(differences[0].expected, Value::Null);
    assert_eq!(differences[1].actual, Value::Null);
    assert_eq!(
        differences[1].expected["schema"]["fields"][0]["name"],
        "inner"
    );
    assert_eq!(differences[2].expected, "required");
    assert_eq!(differences[2].actual, "optional");
}

#[test]
fn array_and_tuple_lengths_do_not_hide_child_changes_or_missing_subtrees() {
    let expected = enumeration(
        "V",
        SchemaVariantShape::Tuple {
            items: vec![
                SchemaShape::Array {
                    length: 2,
                    value: Box::new(SchemaShape::U8),
                },
                SchemaShape::Bool,
            ],
        },
    );
    let actual = enumeration(
        "V",
        SchemaVariantShape::Tuple {
            items: vec![SchemaShape::Array {
                length: 3,
                value: Box::new(SchemaShape::U32),
            }],
        },
    );
    let differences = compare_shapes(&expected, &actual);
    assert_eq!(differences.len(), 4);
    assert_eq!(differences[0].code, DifferenceCode::TupleLengthChanged);
    assert_eq!(differences[0].expected, 2);
    assert_eq!(differences[0].actual, 1);
    assert_eq!(differences[1].code, DifferenceCode::ArrayLengthChanged);
    assert_eq!(differences[2].path.last(), Some(&PathSegment::ArrayItem));
    assert_eq!(differences[3].expected, json!({"kind":"bool"}));
    assert_eq!(differences[3].actual, Value::Null);
    let reversed = compare_shapes(&actual, &expected);
    assert_eq!(reversed[3].expected, Value::Null);
    assert_eq!(reversed[3].actual, json!({"kind":"bool"}));
}

#[test]
fn all_variant_payloads_compare_and_kind_changes_stop_at_the_variant() {
    let payloads = [
        SchemaVariantShape::Unit,
        SchemaVariantShape::Newtype {
            schema: Box::new(SchemaShape::Bool),
        },
        SchemaVariantShape::Tuple {
            items: vec![SchemaShape::Bool],
        },
        SchemaVariantShape::Record {
            fields: vec![field("a", SchemaShape::Bool)],
        },
    ];
    for (a, expected) in payloads.iter().enumerate() {
        for (b, actual) in payloads.iter().enumerate() {
            let differences = compare_shapes(
                &enumeration("V", expected.clone()),
                &enumeration("V", actual.clone()),
            );
            if a == b {
                assert!(differences.is_empty());
            } else {
                assert_eq!(differences.len(), 1);
                assert_eq!(differences[0].code, DifferenceCode::VariantKindChanged);
            }
        }
    }
    for (expected, actual, last) in [
        (
            SchemaVariantShape::Newtype {
                schema: Box::new(SchemaShape::Bool),
            },
            SchemaVariantShape::Newtype {
                schema: Box::new(SchemaShape::String),
            },
            PathSegment::VariantValue,
        ),
        (
            SchemaVariantShape::Record {
                fields: vec![field("a", SchemaShape::Bool)],
            },
            SchemaVariantShape::Record {
                fields: vec![field("a", SchemaShape::String)],
            },
            PathSegment::Field { name: "a".into() },
        ),
    ] {
        let differences = compare_shapes(&enumeration("V", expected), &enumeration("V", actual));
        assert_eq!(differences.len(), 1);
        assert_eq!(differences[0].path.last(), Some(&last));
    }
    let differences = compare_shapes(
        &enumeration("Old", payloads[2].clone()),
        &enumeration("New", payloads[3].clone()),
    );
    assert_eq!(differences[0].code, DifferenceCode::VariantAdded);
    assert_eq!(differences[0].actual["fields"][0]["name"], "a");
    assert_eq!(differences[1].code, DifferenceCode::VariantMissing);
    assert_eq!(differences[1].expected["items"][0], json!({"kind":"bool"}));
}

#[test]
fn incompatible_containers_report_complete_subtrees() {
    let shapes = [
        SchemaShape::Bool,
        record(vec![field("a", SchemaShape::U32)]),
        enumeration("V", SchemaVariantShape::Unit),
        SchemaShape::Option {
            value: Box::new(SchemaShape::U32),
        },
        SchemaShape::Sequence {
            value: Box::new(SchemaShape::U32),
        },
        SchemaShape::Array {
            value: Box::new(SchemaShape::U32),
            length: 2,
        },
        SchemaShape::Map {
            value: Box::new(SchemaShape::U32),
        },
        SchemaShape::Set {
            value: Box::new(SchemaShape::U32),
            membership: membership("example:exact:v1"),
        },
        SchemaShape::Tuple {
            items: vec![SchemaShape::U32],
        },
        SchemaShape::Profile {
            profile: StorageProfile::Finite32,
        },
    ];
    for (a, expected) in shapes.iter().enumerate() {
        for (b, actual) in shapes.iter().enumerate() {
            let differences = compare_shapes(expected, actual);
            if a == b {
                assert!(differences.is_empty());
            } else {
                assert_eq!(differences.len(), 1);
                assert_eq!(differences[0].code, DifferenceCode::ShapeKindChanged);
                assert_eq!(
                    differences[0].expected,
                    serde_json::to_value(expected).unwrap()
                );
            }
        }
    }
}

fn membership(id: &str) -> Membership {
    Membership {
        id: id.to_owned(),
        parameters: vec![],
    }
}

#[test]
fn collection_profile_and_membership_changes_have_precise_paths() {
    let expected = SchemaShape::Map {
        value: Box::new(SchemaShape::Set {
            value: Box::new(SchemaShape::Tuple {
                items: vec![SchemaShape::Profile {
                    profile: StorageProfile::Finite32,
                }],
            }),
            membership: membership("example:exact:v1"),
        }),
    };
    let actual = SchemaShape::Map {
        value: Box::new(SchemaShape::Set {
            value: Box::new(SchemaShape::Tuple {
                items: vec![SchemaShape::Profile {
                    profile: StorageProfile::Finite64,
                }],
            }),
            membership: membership("example:folded:v1"),
        }),
    };
    let differences = compare_shapes(&expected, &actual);
    assert_eq!(differences.len(), 2);
    assert_eq!(differences[0].code, DifferenceCode::MembershipChanged);
    assert_eq!(differences[0].path, vec![PathSegment::MapValue]);
    assert_eq!(differences[0].expected["id"], "example:exact:v1");
    assert_eq!(differences[1].code, DifferenceCode::ProfileChanged);
    assert_eq!(
        differences[1].path,
        vec![
            PathSegment::MapValue,
            PathSegment::SetItem,
            PathSegment::TupleItem { index: 0 },
        ]
    );
    assert_eq!(differences[1].expected, "type-history:finite32:v1");
    assert_eq!(
        readable_path(&differences[1].path),
        "$.map_value.set_item[0]"
    );
}

#[test]
fn readable_names_and_codes_have_stable_unambiguous_spellings() {
    let path = [PathSegment::Field {
        name: "a.b\"\\🦀".into(),
    }];
    assert_eq!(readable_path(&path), "$[\"a.b\\\"\\\\🦀\"]");
    for code in [
        DifferenceCode::FieldMissing,
        DifferenceCode::FieldAdded,
        DifferenceCode::FieldPresenceChanged,
        DifferenceCode::ShapeKindChanged,
        DifferenceCode::ArrayLengthChanged,
        DifferenceCode::MembershipChanged,
        DifferenceCode::ProfileChanged,
        DifferenceCode::VariantMissing,
        DifferenceCode::VariantAdded,
        DifferenceCode::VariantKindChanged,
        DifferenceCode::TupleLengthChanged,
    ] {
        assert_eq!(serde_json::to_value(code).unwrap(), code.as_str());
    }
}
