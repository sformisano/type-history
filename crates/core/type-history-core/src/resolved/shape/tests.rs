use super::{FieldPresence, SchemaField, SchemaShape, SchemaVariant, SchemaVariantShape};
use serde_json::{from_str, to_string};

fn named_variants() -> Vec<(SchemaVariant, &'static str)> {
    vec![
        (
            SchemaVariant {
                name: "unit".to_owned(),
                shape: SchemaVariantShape::Unit,
            },
            r#"{"name":"unit","kind":"unit"}"#,
        ),
        (
            SchemaVariant {
                name: "newtype".to_owned(),
                shape: SchemaVariantShape::Newtype {
                    schema: Box::new(SchemaShape::U32),
                },
            },
            r#"{"name":"newtype","kind":"newtype","schema":{"kind":"u32"}}"#,
        ),
        (
            SchemaVariant {
                name: "tuple".to_owned(),
                shape: SchemaVariantShape::Tuple {
                    items: vec![SchemaShape::U32, SchemaShape::String],
                },
            },
            r#"{"name":"tuple","kind":"tuple","items":[{"kind":"u32"},{"kind":"string"}]}"#,
        ),
        (
            SchemaVariant {
                name: "record".to_owned(),
                shape: SchemaVariantShape::Record {
                    fields: vec![SchemaField {
                        name: "value".to_owned(),
                        presence: FieldPresence::Required,
                        schema: SchemaShape::U32,
                    }],
                },
            },
            r#"{"name":"record","kind":"record","fields":[{"name":"value","presence":"required","schema":{"kind":"u32"}}]}"#,
        ),
    ]
}

#[test]
fn named_variant_families_round_trip_with_unchanged_bytes() {
    for (variant, expected_json) in named_variants() {
        assert_eq!(to_string(&variant).unwrap(), expected_json);
        assert_eq!(from_str::<SchemaVariant>(expected_json).unwrap(), variant);
    }
}

#[test]
fn enclosing_and_nested_enums_round_trip_with_unchanged_bytes() {
    let enumeration = SchemaShape::Enum {
        variants: named_variants()
            .into_iter()
            .map(|(variant, _)| variant)
            .collect(),
    };
    let expected_enumeration = r#"{"kind":"enum","variants":[{"name":"unit","kind":"unit"},{"name":"newtype","kind":"newtype","schema":{"kind":"u32"}},{"name":"tuple","kind":"tuple","items":[{"kind":"u32"},{"kind":"string"}]},{"name":"record","kind":"record","fields":[{"name":"value","presence":"required","schema":{"kind":"u32"}}]}]}"#;
    assert_eq!(to_string(&enumeration).unwrap(), expected_enumeration);
    assert_eq!(
        from_str::<SchemaShape>(expected_enumeration).unwrap(),
        enumeration
    );

    let nested = SchemaShape::Enum {
        variants: vec![SchemaVariant {
            name: "outer".to_owned(),
            shape: SchemaVariantShape::Newtype {
                schema: Box::new(SchemaShape::Enum {
                    variants: vec![SchemaVariant {
                        name: "inner".to_owned(),
                        shape: SchemaVariantShape::Unit,
                    }],
                }),
            },
        }],
    };
    let expected_nested = r#"{"kind":"enum","variants":[{"name":"outer","kind":"newtype","schema":{"kind":"enum","variants":[{"name":"inner","kind":"unit"}]}}]}"#;
    assert_eq!(to_string(&nested).unwrap(), expected_nested);
    assert_eq!(from_str::<SchemaShape>(expected_nested).unwrap(), nested);
}

#[test]
fn named_variants_accept_reordered_fields() {
    let reordered = [
        r#"{"kind":"unit","name":"unit"}"#,
        r#"{"schema":{"kind":"u32"},"kind":"newtype","name":"newtype"}"#,
        r#"{"items":[{"kind":"u32"},{"kind":"string"}],"name":"tuple","kind":"tuple"}"#,
        r#"{"fields":[{"schema":{"kind":"u32"},"presence":"required","name":"value"}],"kind":"record","name":"record"}"#,
    ];

    for ((expected, _), json) in named_variants().into_iter().zip(reordered) {
        assert_eq!(from_str::<SchemaVariant>(json).unwrap(), expected);
    }
}

#[test]
fn named_variants_reject_duplicate_keys() {
    let invalid = [
        r#"{"name":"first","name":"second","kind":"unit"}"#,
        r#"{"name":"unit","kind":"unit","kind":"unit"}"#,
        r#"{"name":"newtype","kind":"newtype","schema":{"kind":"u32"},"schema":{"kind":"u64"}}"#,
        r#"{"name":"tuple","kind":"tuple","items":[],"items":[]}"#,
        r#"{"name":"record","kind":"record","fields":[],"fields":[]}"#,
    ];

    assert_all_invalid(&invalid);
}

#[test]
fn named_variants_reject_unknown_missing_and_mismatched_fields() {
    let invalid = [
        r#"{"name":"unit","kind":"unit","unknown":true}"#,
        r#"{"name":"unit","kind":"unknown"}"#,
        r#"{"kind":"unit"}"#,
        r#"{"name":"unit"}"#,
        r#"{"name":"newtype","kind":"newtype"}"#,
        r#"{"name":"tuple","kind":"tuple"}"#,
        r#"{"name":"record","kind":"record"}"#,
        r#"{"name":"unit","kind":"unit","schema":{"kind":"u32"}}"#,
        r#"{"name":"newtype","kind":"newtype","items":[]}"#,
        r#"{"name":"tuple","kind":"tuple","fields":[]}"#,
        r#"{"name":"record","kind":"record","schema":{"kind":"u32"}}"#,
        r#"{"name":"newtype","kind":"newtype","schema":{"kind":"u32"},"items":[]}"#,
    ];

    assert_all_invalid(&invalid);
}

#[test]
fn named_variants_reject_null_and_wrong_field_types() {
    let invalid = [
        r#"{"name":null,"kind":"unit"}"#,
        r#"{"name":"unit","kind":null}"#,
        r#"{"name":"newtype","kind":"newtype","schema":null}"#,
        r#"{"name":"tuple","kind":"tuple","items":null}"#,
        r#"{"name":"record","kind":"record","fields":null}"#,
        r#"{"name":1,"kind":"unit"}"#,
        r#"{"name":"unit","kind":1}"#,
        r#"{"name":"newtype","kind":"newtype","schema":[]}"#,
        r#"{"name":"tuple","kind":"tuple","items":{}}"#,
        r#"{"name":"record","kind":"record","fields":{}}"#,
    ];

    assert_all_invalid(&invalid);
}

#[test]
fn enclosing_enum_rejects_an_invalid_nested_variant() {
    let invalid = r#"{"kind":"enum","variants":[{"name":"outer","kind":"newtype","schema":{"kind":"enum","variants":[{"name":"inner","kind":"unit","kind":"unit"}]}}]}"#;

    assert!(from_str::<SchemaShape>(invalid).is_err());
}

fn assert_all_invalid(values: &[&str]) {
    for value in values {
        assert!(
            from_str::<SchemaVariant>(value).is_err(),
            "unexpectedly accepted {value}"
        );
    }
}

#[test]
fn newtype_records_normalize_to_the_same_named_variant() {
    let fields = vec![SchemaField {
        name: "amount".into(),
        presence: FieldPresence::Required,
        schema: SchemaShape::U64,
    }];
    let newtype = SchemaShape::Enum {
        variants: vec![SchemaVariant {
            name: "Paid".into(),
            shape: SchemaVariantShape::Newtype {
                schema: Box::new(SchemaShape::Record {
                    fields: fields.clone(),
                }),
            },
        }],
    };
    let record = SchemaShape::Enum {
        variants: vec![SchemaVariant {
            name: "Paid".into(),
            shape: SchemaVariantShape::Record { fields },
        }],
    };
    assert_ne!(newtype, record);
    assert_eq!(newtype.normalized(), record.normalized());
}
