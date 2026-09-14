//! Runtime structural shapes and their canonical field ordering.

use serde::{Deserialize, Serialize};

mod deserialize;
#[cfg(test)]
mod tests;

/// Runtime form of one structural wire schema.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SchemaShape {
    Bool,
    I8,
    I16,
    I32,
    I64,
    I128,
    U8,
    U16,
    U32,
    U64,
    U128,
    String,
    Bytes,
    /// An optional value with one non-optional inner shape.
    Option {
        /// Shape present when the value is not null.
        value: Box<SchemaShape>,
    },
    /// A variable-length homogeneous sequence.
    Sequence {
        /// Shape shared by every element.
        value: Box<SchemaShape>,
    },
    /// A fixed-length homogeneous sequence.
    Array {
        /// Shape shared by every element.
        value: Box<SchemaShape>,
        /// Required number of elements.
        length: usize,
    },
    /// A closed record with named fields.
    Record {
        /// Fields in canonical name order after normalization.
        fields: Vec<SchemaField>,
    },
    /// An externally tagged enumeration.
    Enum {
        /// Variants in canonical name order after normalization.
        variants: Vec<SchemaVariant>,
    },
}

impl SchemaShape {
    /// Recursively sort named fields and variants into canonical order.
    pub fn normalized(self) -> Self {
        match self {
            Self::Option { value } => Self::Option {
                value: Box::new(value.normalized()),
            },
            Self::Sequence { value } => Self::Sequence {
                value: Box::new(value.normalized()),
            },
            Self::Array { value, length } => Self::Array {
                value: Box::new(value.normalized()),
                length,
            },
            Self::Record { mut fields } => {
                for field in &mut fields {
                    field.schema = field.schema.clone().normalized();
                }
                fields.sort_by(|left, right| left.name.cmp(&right.name));
                Self::Record { fields }
            }
            Self::Enum { mut variants } => {
                for variant in &mut variants {
                    variant.shape.normalize();
                }
                variants.sort_by(|left, right| left.name.cmp(&right.name));
                Self::Enum { variants }
            }
            primitive => primitive,
        }
    }
}

/// One named field and its presence rule in a closed record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SchemaField {
    /// Durable serialized field name.
    pub name: String,
    /// Whether the field must appear in the encoded record.
    pub presence: FieldPresence,
    /// Structural wire shape of the field value.
    pub schema: SchemaShape,
}

/// Whether a record field must appear in its encoded object.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldPresence {
    /// The encoded object must contain the field.
    Required,
    /// The encoded object may omit the field.
    Optional,
}

/// One named variant in an externally tagged enumeration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SchemaVariant {
    /// Durable serialized variant name.
    pub name: String,
    #[serde(flatten)]
    /// Payload shape carried by the variant.
    pub shape: SchemaVariantShape,
}

/// Payload shape carried by one enumeration variant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SchemaVariantShape {
    /// A variant without a payload.
    Unit,
    /// A variant containing one unnamed value.
    Newtype {
        /// Shape of the contained value.
        schema: Box<SchemaShape>,
    },
    /// A variant containing an ordered tuple payload.
    Tuple {
        /// Shapes in tuple position order.
        items: Vec<SchemaShape>,
    },
    /// A variant containing a closed named record.
    Record {
        /// Fields in canonical name order after normalization.
        fields: Vec<SchemaField>,
    },
}

impl SchemaVariantShape {
    fn normalize(&mut self) {
        match self {
            Self::Unit => {}
            Self::Newtype { schema } => {
                let normalized = schema.clone().normalized();
                *self = match normalized {
                    SchemaShape::Record { fields } => Self::Record { fields },
                    schema => Self::Newtype {
                        schema: Box::new(schema),
                    },
                };
            }
            Self::Tuple { items } => {
                for item in items {
                    *item = item.clone().normalized();
                }
            }
            Self::Record { fields } => {
                for field in fields.iter_mut() {
                    field.schema = field.schema.clone().normalized();
                }
                fields.sort_by(|left, right| left.name.cmp(&right.name));
            }
        }
    }
}
