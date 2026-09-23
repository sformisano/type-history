//! Rustc-resolved structural wire schemas used by generated histories and their frontends.
//!
//! The wire language tracks encoding, domain, membership, and field presence. A nested
//! `Option<Option<T>>` also fails selection: the wire and the JSON Schema
//! comparison document cannot distinguish `Some(None)` from `None`.
//!
//! The public type-level marker nodes in this module support cross-crate generated
//! code. Authors normally use [`ResolvedSchema`] and [`SchemaShape`] instead.

#![allow(missing_docs)]

mod constant;
mod containers;
mod json_schema;
#[doc(hidden)]
pub mod membership;
mod nodes;
#[doc(hidden)]
pub mod profile;
mod shape;
mod tuples;
pub use constant::{
    ConstantFields, ConstantItems, ConstantName, ConstantShape, ConstantVariant, ConstantVariants,
};
pub use json_schema::{apply_named_presence, export_json_schema, FieldSchema, JsonSchemaField};
pub use membership::{ConstantMembership, Membership, SetMembership};
pub use nodes::{Map, Newtype, Profile, Set, Tuple};
pub use profile::{profile_schema, ProfileError, ProfileMarker, StorageProfile};
pub use shape::{FieldPresence, SchemaField, SchemaShape, SchemaVariant, SchemaVariantShape};

use std::marker::PhantomData;

/// Structural schema resolved through Rust trait selection.
#[diagnostic::on_unimplemented(
    message = "`{Self}` is not supported as a persisted field",
    label = "this type has no supported structural wire schema",
    note = "For a supported named record or enum, derive `type_history::Schema` or use your framework's schema declaration.",
    note = "For other types, choose a supported serialized representation; wrapping an unsupported field alone is insufficient."
)]
pub trait ResolvedSchema {
    /// Type-level form used for compile-time shape comparisons.
    type Wire: WireNode;

    /// Return the normalized runtime form of the selected wire schema.
    fn resolved_wire_schema() -> SchemaShape {
        Self::Wire::schema().normalized()
    }
}

/// One type-level node in the closed structural wire language.
pub trait WireNode {
    /// Static shape used by generated compile-time equality checks.
    const SHAPE: ConstantShape;
    /// Whether a named field can be absent, independently of value nullability.
    const FIELD_PRESENCE: FieldPresence = FieldPresence::Required;
    /// Build the equivalent runtime schema shape.
    fn schema() -> SchemaShape {
        Self::SHAPE.schema()
    }
}

pub struct Bool;
pub struct I8;
pub struct I16;
pub struct I32;
pub struct I64;
pub struct I128;
pub struct U8;
pub struct U16;
pub struct U32;
pub struct U64;
pub struct U128;
pub struct StringValue;
pub struct Optional<Value>(PhantomData<Value>);
pub struct Vector<Value>(PhantomData<Value>);
pub struct FixedArray<Value, const LENGTH: usize>(PhantomData<Value>);
pub struct Record<Fields>(PhantomData<Fields>);
pub struct Enumeration<Variants>(PhantomData<Variants>);

macro_rules! primitive_node {
    ($node:ident, $shape:ident, $rust:ty) => {
        impl WireNode for $node {
            const SHAPE: ConstantShape = ConstantShape::$shape;
        }

        impl ResolvedSchema for $rust {
            type Wire = $node;
        }
    };
}

primitive_node!(Bool, Bool, bool);
primitive_node!(I8, I8, i8);
primitive_node!(I16, I16, i16);
primitive_node!(I32, I32, i32);
primitive_node!(I64, I64, i64);
primitive_node!(I128, I128, i128);
primitive_node!(U8, U8, u8);
primitive_node!(U16, U16, u16);
primitive_node!(U32, U32, u32);
primitive_node!(U64, U64, u64);
primitive_node!(U128, U128, u128);
primitive_node!(StringValue, String, String);

impl<Value: WireNode> WireNode for Optional<Value> {
    const FIELD_PRESENCE: FieldPresence = FieldPresence::Optional;
    const SHAPE: ConstantShape = ConstantShape::Option(&Value::SHAPE);
}

impl<Value: ResolvedSchema> ResolvedSchema for Option<Value>
where
    Value::Wire: NonOptionalNode,
{
    type Wire = Optional<Value::Wire>;
}

/// Implemented by wire nodes whose values cannot be null.
/// `Option<T>` requires this bound, including through transparent newtypes.
#[diagnostic::on_unimplemented(
    message = "`Option<Option<_>>` is not a supported persisted type",
    label = "nested optional persisted field",
    note = "the wire format and the JSON Schema comparison document cannot distinguish `Some(None)` from `None`; flatten the field to one `Option<T>`"
)]
pub trait NonOptionalNode: WireNode {}

macro_rules! non_optional_primitive {
    ($($node:ty),+ $(,)?) => {
        $(impl NonOptionalNode for $node {})+
    };
}

non_optional_primitive!(
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
    StringValue
);

impl<Value: WireNode> NonOptionalNode for Vector<Value> {}
impl<Value: WireNode, const LENGTH: usize> NonOptionalNode for FixedArray<Value, LENGTH> {}
impl<Fields: WireFields> NonOptionalNode for Record<Fields> {}
impl<Variants: WireVariants> NonOptionalNode for Enumeration<Variants> {}

impl<Value: WireNode> WireNode for Vector<Value> {
    const SHAPE: ConstantShape = if matches!(Value::SHAPE, ConstantShape::U8) {
        ConstantShape::Bytes
    } else {
        ConstantShape::Sequence(&Value::SHAPE)
    };
}

impl<Value: ResolvedSchema> ResolvedSchema for Vec<Value> {
    type Wire = Vector<Value::Wire>;
}

impl<Value: WireNode, const LENGTH: usize> WireNode for FixedArray<Value, LENGTH> {
    const SHAPE: ConstantShape = ConstantShape::Array(&Value::SHAPE, LENGTH);
}

impl<Value: ResolvedSchema, const LENGTH: usize> ResolvedSchema for [Value; LENGTH] {
    type Wire = FixedArray<Value::Wire, LENGTH>;
}

pub struct End;
pub struct Item<Head, Tail>(PhantomData<(Head, Tail)>);

pub trait WireItems {
    const ITEMS: ConstantItems;
    fn schemas() -> Vec<SchemaShape> {
        Self::ITEMS.items()
    }
}

impl WireItems for End {
    const ITEMS: ConstantItems = ConstantItems::End;
}

impl<Head: WireNode, Tail: WireItems> WireItems for Item<Head, Tail> {
    const ITEMS: ConstantItems = ConstantItems::Item(&Head::SHAPE, &Tail::ITEMS);
}

pub struct NameEnd;
pub struct NameByte<const VALUE: u8, Tail>(PhantomData<Tail>);

/// Eight exact wire-name bytes, most significant byte first, followed by `Tail`.
/// This compact marker has the same constant representation as eight [`NameByte`] nodes.
pub struct NameChunk<const BYTES: u64, Tail>(PhantomData<Tail>);

impl<const BYTES: u64, Tail: WireName> WireName for NameChunk<BYTES, Tail> {
    const NAME: ConstantName = ConstantName::Byte(
        (BYTES >> 56) as u8,
        &ConstantName::Byte(
            (BYTES >> 48) as u8,
            &ConstantName::Byte(
                (BYTES >> 40) as u8,
                &ConstantName::Byte(
                    (BYTES >> 32) as u8,
                    &ConstantName::Byte(
                        (BYTES >> 24) as u8,
                        &ConstantName::Byte(
                            (BYTES >> 16) as u8,
                            &ConstantName::Byte(
                                (BYTES >> 8) as u8,
                                &ConstantName::Byte(BYTES as u8, &Tail::NAME),
                            ),
                        ),
                    ),
                ),
            ),
        ),
    );
    fn append(bytes: &mut Vec<u8>) {
        bytes.extend_from_slice(&BYTES.to_be_bytes());
        Tail::append(bytes);
    }
}

pub trait WireName {
    const NAME: ConstantName;
    fn append(bytes: &mut Vec<u8>);

    fn value() -> String {
        let mut bytes = Vec::new();
        Self::append(&mut bytes);
        String::from_utf8(bytes).expect("history generation produced a UTF-8 wire name")
    }
}

impl WireName for NameEnd {
    const NAME: ConstantName = ConstantName::End;
    fn append(_bytes: &mut Vec<u8>) {}
}

impl<const VALUE: u8, Tail: WireName> WireName for NameByte<VALUE, Tail> {
    const NAME: ConstantName = ConstantName::Byte(VALUE, &Tail::NAME);
    fn append(bytes: &mut Vec<u8>) {
        bytes.push(VALUE);
        Tail::append(bytes);
    }
}

pub struct Field<Name, Value>(PhantomData<(Name, Value)>);

pub trait WireFields {
    const FIELDS: ConstantFields;
    fn fields() -> Vec<SchemaField> {
        Self::FIELDS.fields()
    }
}

impl WireFields for End {
    const FIELDS: ConstantFields = ConstantFields::End;
}

impl<Name: WireName, Value: WireNode, Tail: WireFields> WireFields
    for Item<Field<Name, Value>, Tail>
{
    const FIELDS: ConstantFields = ConstantFields::Field(
        Name::NAME,
        Value::FIELD_PRESENCE,
        &Value::SHAPE,
        &Tail::FIELDS,
    );
}

impl<Fields: WireFields> WireNode for Record<Fields> {
    const SHAPE: ConstantShape = ConstantShape::Record(Fields::FIELDS);
}

pub struct Variant<Name, Shape>(PhantomData<(Name, Shape)>);
pub struct UnitVariant;
pub struct NewtypeVariant<Value>(PhantomData<Value>);
pub struct TupleVariant<Items>(PhantomData<Items>);
pub struct RecordVariant<Fields>(PhantomData<Fields>);

pub trait WireVariantNode {
    const SHAPE: ConstantVariant;
    fn shape() -> SchemaVariantShape {
        Self::SHAPE.shape()
    }
}

impl WireVariantNode for UnitVariant {
    const SHAPE: ConstantVariant = ConstantVariant::Unit;
}

impl<Value: WireNode> WireVariantNode for NewtypeVariant<Value> {
    const SHAPE: ConstantVariant = ConstantVariant::newtype(&Value::SHAPE);
}

impl<Items: WireItems> WireVariantNode for TupleVariant<Items> {
    const SHAPE: ConstantVariant = ConstantVariant::Tuple(Items::ITEMS);
}

impl<Fields: WireFields> WireVariantNode for RecordVariant<Fields> {
    const SHAPE: ConstantVariant = ConstantVariant::Record(Fields::FIELDS);
}

pub trait WireVariants {
    const VARIANTS: ConstantVariants;
    fn variants() -> Vec<SchemaVariant> {
        Self::VARIANTS.variants()
    }
}

impl WireVariants for End {
    const VARIANTS: ConstantVariants = ConstantVariants::End;
}

impl<Name: WireName, Shape: WireVariantNode, Tail: WireVariants> WireVariants
    for Item<Variant<Name, Shape>, Tail>
{
    const VARIANTS: ConstantVariants =
        ConstantVariants::Variant(Name::NAME, Shape::SHAPE, &Tail::VARIANTS);
}

impl<Variants: WireVariants> WireNode for Enumeration<Variants> {
    const SHAPE: ConstantShape = ConstantShape::Enum(Variants::VARIANTS);
}

#[cfg(test)]
mod tests {
    use super::*;

    type Name = NameByte<b'i', NameByte<b'd', NameEnd>>;
    type Fields = Item<Field<Name, U64>, End>;

    #[test]
    fn external_nodes_can_use_the_default_or_override_runtime_schema() {
        struct DefaultNode;
        impl WireNode for DefaultNode {
            const SHAPE: ConstantShape = ConstantShape::Bool;
        }
        struct CustomNode;
        impl WireNode for CustomNode {
            const SHAPE: ConstantShape = ConstantShape::String;
            fn schema() -> SchemaShape {
                SchemaShape::String
            }
        }
        assert_eq!(DefaultNode::schema(), SchemaShape::Bool);
        assert_eq!(CustomNode::schema(), SchemaShape::String);
    }

    #[test]
    fn structural_record_schema_is_exact_and_normalized() {
        assert_eq!(
            Record::<Fields>::schema(),
            SchemaShape::Record {
                fields: vec![SchemaField {
                    name: "id".to_owned(),
                    presence: FieldPresence::Required,
                    schema: SchemaShape::U64,
                }],
            }
        );
    }

    #[test]
    fn byte_vectors_keep_the_bytes_wire_node() {
        assert_eq!(Vector::<U8>::schema(), SchemaShape::Bytes);
        assert_eq!(
            Vector::<U16>::schema(),
            SchemaShape::Sequence {
                value: Box::new(SchemaShape::U16),
            }
        );
    }
}
