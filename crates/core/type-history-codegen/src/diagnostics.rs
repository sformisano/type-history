//! Stable differences between compiler-resolved wire schemas.

mod compare;
mod marker;
#[cfg(test)]
mod tests;

use std::fmt::{Display, Formatter, Result as FmtResult};

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use compare::compare_shapes;
pub use marker::{parse_compiler_marker, DiagnosticDecodeError, FrozenSchemaObservation};

/// Stable v1 schema difference codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DifferenceCode {
    /// A previously present named field is absent.
    FieldMissing,
    /// A new named field appears in a frozen record.
    FieldAdded,
    /// A named field's omission rule changed.
    FieldPresenceChanged,
    /// Wire node kinds differ, or a tuple position is absent.
    ShapeKindChanged,
    /// A fixed array's element count changed.
    ArrayLengthChanged,
    /// A set's declared element equality changed.
    MembershipChanged,
    /// A logical storage profile changed.
    ProfileChanged,
    /// A previously present variant is absent.
    VariantMissing,
    /// A new variant appears in a frozen enum.
    VariantAdded,
    /// A variant's payload family changed.
    VariantKindChanged,
    /// A variant tuple's arity changed.
    TupleLengthChanged,
}

impl DifferenceCode {
    /// Stable machine spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::FieldMissing => "field_missing",
            Self::FieldAdded => "field_added",
            Self::FieldPresenceChanged => "field_presence_changed",
            Self::ShapeKindChanged => "shape_kind_changed",
            Self::ArrayLengthChanged => "array_length_changed",
            Self::MembershipChanged => "membership_changed",
            Self::ProfileChanged => "profile_changed",
            Self::VariantMissing => "variant_missing",
            Self::VariantAdded => "variant_added",
            Self::VariantKindChanged => "variant_kind_changed",
            Self::TupleLengthChanged => "tuple_length_changed",
        }
    }
}

impl Display for DifferenceCode {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        formatter.write_str(self.as_str())
    }
}

/// One unambiguous step through a wire schema.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PathSegment {
    /// A named record field.
    Field {
        /// Durable wire name.
        name: String,
    },
    /// The non-null value of an option.
    OptionValue,
    /// An element of a variable-length sequence.
    SequenceItem,
    /// An element of a fixed-length array.
    ArrayItem,
    /// The value contract of a string-keyed map.
    MapValue,
    /// The element contract of a set.
    SetItem,
    /// A named enum variant.
    Variant {
        /// Durable variant name.
        name: String,
    },
    /// A positional tuple element.
    TupleItem {
        /// Zero-based position.
        index: usize,
    },
    /// The payload of a newtype variant.
    VariantValue,
}

/// One independent difference, without frontend-owned package or source context.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SchemaDifference {
    /// Stable machine-readable reason.
    pub code: DifferenceCode,
    /// Complete route from the payload root to the difference.
    pub path: Vec<PathSegment>,
    /// Frozen descriptor or scalar; null denotes absence.
    pub expected: Value,
    /// Observed descriptor or scalar; null denotes absence.
    pub actual: Value,
    /// Suggested correction that preserves existing historical meanings.
    pub hint: String,
}

/// Render typed segments without confusing names containing punctuation.
pub fn readable_path(path: &[PathSegment]) -> String {
    let mut rendered = "$".to_owned();
    for segment in path {
        match segment {
            PathSegment::Field { name } => {
                rendered.push_str(&format!("[{}]", json_name(name)));
            }
            PathSegment::Variant { name } => {
                rendered.push_str(&format!(".variant[{}]", json_name(name)));
            }
            PathSegment::OptionValue => rendered.push_str(".option_value"),
            PathSegment::SequenceItem => rendered.push_str(".sequence_item"),
            PathSegment::ArrayItem => rendered.push_str(".array_item"),
            PathSegment::MapValue => rendered.push_str(".map_value"),
            PathSegment::SetItem => rendered.push_str(".set_item"),
            PathSegment::TupleItem { index } => rendered.push_str(&format!("[{index}]")),
            PathSegment::VariantValue => rendered.push_str(".variant_value"),
        }
    }
    rendered
}

fn json_name(name: &str) -> String {
    serde_json::to_string(name).expect("wire names serialize as JSON strings")
}
