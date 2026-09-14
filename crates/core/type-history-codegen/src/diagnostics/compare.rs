use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::Value;
use type_history_core::resolved::{SchemaField, SchemaShape, SchemaVariant, SchemaVariantShape};

use super::{DifferenceCode, PathSegment, SchemaDifference};

/// Compare normalized descriptors, retaining independent sibling differences.
pub fn compare_shapes(expected: &SchemaShape, actual: &SchemaShape) -> Vec<SchemaDifference> {
    let mut differences = Differences(Vec::new());
    differences.shape(
        &expected.clone().normalized(),
        &actual.clone().normalized(),
        &[],
    );
    differences.0.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then_with(|| left.code.as_str().cmp(right.code.as_str()))
    });
    differences.0
}

struct Differences(Vec<SchemaDifference>);

impl Differences {
    fn push<E: Serialize, A: Serialize>(
        &mut self,
        code: DifferenceCode,
        path: &[PathSegment],
        expected: E,
        actual: A,
    ) {
        self.0.push(SchemaDifference {
            code, path: path.to_vec(),
            expected: serde_json::to_value(expected).expect("schema descriptors serialize"),
            actual: serde_json::to_value(actual).expect("schema descriptors serialize"),
            hint: "Restore the frozen schema and add the next containing-struct version for this change.".to_owned(),
        });
    }

    fn shape(&mut self, expected: &SchemaShape, actual: &SchemaShape, path: &[PathSegment]) {
        if expected == actual {
            return;
        }
        match (expected, actual) {
            (SchemaShape::Record { fields: a }, SchemaShape::Record { fields: b }) => {
                self.fields(a, b, path)
            }
            (SchemaShape::Enum { variants: a }, SchemaShape::Enum { variants: b }) => {
                self.variants(a, b, path)
            }
            (SchemaShape::Option { value: a }, SchemaShape::Option { value: b }) => {
                self.shape(a, b, &child(path, PathSegment::OptionValue));
            }
            (SchemaShape::Sequence { value: a }, SchemaShape::Sequence { value: b }) => {
                self.shape(a, b, &child(path, PathSegment::SequenceItem));
            }
            (
                SchemaShape::Array {
                    value: a,
                    length: al,
                },
                SchemaShape::Array {
                    value: b,
                    length: bl,
                },
            ) => {
                if al != bl {
                    self.push(DifferenceCode::ArrayLengthChanged, path, al, bl);
                }
                self.shape(a, b, &child(path, PathSegment::ArrayItem));
            }
            _ => self.push(DifferenceCode::ShapeKindChanged, path, expected, actual),
        }
    }

    fn fields(&mut self, expected: &[SchemaField], actual: &[SchemaField], path: &[PathSegment]) {
        let a: BTreeMap<_, _> = expected.iter().map(|field| (&field.name, field)).collect();
        let b: BTreeMap<_, _> = actual.iter().map(|field| (&field.name, field)).collect();
        for (name, expected) in &a {
            let path = child(
                path,
                PathSegment::Field {
                    name: (*name).clone(),
                },
            );
            if let Some(actual) = b.get(name) {
                if expected.presence != actual.presence {
                    self.push(
                        DifferenceCode::FieldPresenceChanged,
                        &path,
                        expected.presence,
                        actual.presence,
                    );
                }
                self.shape(&expected.schema, &actual.schema, &path);
            } else {
                self.push(DifferenceCode::FieldMissing, &path, expected, Value::Null);
            }
        }
        for (name, actual) in &b {
            if !a.contains_key(name) {
                self.push(
                    DifferenceCode::FieldAdded,
                    &child(
                        path,
                        PathSegment::Field {
                            name: (*name).clone(),
                        },
                    ),
                    Value::Null,
                    actual,
                );
            }
        }
    }

    fn variants(
        &mut self,
        expected: &[SchemaVariant],
        actual: &[SchemaVariant],
        path: &[PathSegment],
    ) {
        let a: BTreeMap<_, _> = expected
            .iter()
            .map(|variant| (&variant.name, variant))
            .collect();
        let b: BTreeMap<_, _> = actual
            .iter()
            .map(|variant| (&variant.name, variant))
            .collect();
        for (name, expected) in &a {
            let path = child(
                path,
                PathSegment::Variant {
                    name: (*name).clone(),
                },
            );
            if let Some(actual) = b.get(name) {
                self.variant(&expected.shape, &actual.shape, &path);
            } else {
                self.push(DifferenceCode::VariantMissing, &path, expected, Value::Null);
            }
        }
        for (name, actual) in &b {
            if !a.contains_key(name) {
                self.push(
                    DifferenceCode::VariantAdded,
                    &child(
                        path,
                        PathSegment::Variant {
                            name: (*name).clone(),
                        },
                    ),
                    Value::Null,
                    actual,
                );
            }
        }
    }

    fn variant(
        &mut self,
        expected: &SchemaVariantShape,
        actual: &SchemaVariantShape,
        path: &[PathSegment],
    ) {
        match (expected, actual) {
            (SchemaVariantShape::Unit, SchemaVariantShape::Unit) => {}
            (
                SchemaVariantShape::Newtype { schema: a },
                SchemaVariantShape::Newtype { schema: b },
            ) => self.shape(a, b, &child(path, PathSegment::VariantValue)),
            (
                SchemaVariantShape::Record { fields: a },
                SchemaVariantShape::Record { fields: b },
            ) => self.fields(a, b, path),
            (SchemaVariantShape::Tuple { items: a }, SchemaVariantShape::Tuple { items: b }) => {
                if a.len() != b.len() {
                    self.push(DifferenceCode::TupleLengthChanged, path, a.len(), b.len());
                }
                for index in 0..a.len().max(b.len()) {
                    let path = child(path, PathSegment::TupleItem { index });
                    match (a.get(index), b.get(index)) {
                        (Some(a), Some(b)) => self.shape(a, b, &path),
                        (a, b) => self.push(DifferenceCode::ShapeKindChanged, &path, a, b),
                    }
                }
            }
            _ => self.push(DifferenceCode::VariantKindChanged, path, expected, actual),
        }
    }
}

fn child(path: &[PathSegment], segment: PathSegment) -> Vec<PathSegment> {
    let mut result = path.to_vec();
    result.push(segment);
    result
}
