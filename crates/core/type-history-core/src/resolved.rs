//! Rustc-resolved structural wire schemas used by generated histories and their frontends.
//!
//! The v1 language is deliberately closed. It has no map or set node, so maps
//! and sets in record payloads fail trait selection. A nested
//! `Option<Option<T>>` also fails selection: the wire and the JSON Schema
//! comparison document cannot distinguish `Some(None)` from `None`.
//!
//! The public type-level marker nodes in this module support cross-crate generated
//! code. Authors normally use [`ResolvedSchema`] and [`SchemaShape`] instead.

#![allow(missing_docs)]

mod constant;
mod json_schema;
#[doc(hidden)]
#[path = "resolved/diagnostic.rs"]
pub mod schema_diagnostic;
mod shape;
pub use constant::{
    ConstantFields, ConstantItems, ConstantName, ConstantShape, ConstantVariant, ConstantVariants,
};
pub use json_schema::{export_json_schema, FieldSchema, JsonSchemaField};
pub use shape::{FieldPresence, SchemaField, SchemaShape, SchemaVariant, SchemaVariantShape};

use std::marker::PhantomData;

/// Structural schema resolved through Rust trait selection.
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
    /// Build the equivalent runtime schema shape.
    fn schema() -> SchemaShape;
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
            fn schema() -> SchemaShape {
                SchemaShape::$shape
            }
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
    const SHAPE: ConstantShape = ConstantShape::Option(&Value::SHAPE);
    fn schema() -> SchemaShape {
        SchemaShape::Option {
            value: Box::new(Value::schema()),
        }
    }
}

impl<Value: ResolvedSchema> ResolvedSchema for Option<Value>
where
    Value::Wire: NonOptionalNode,
{
    type Wire = Optional<Value::Wire>;
}

/// Implemented by every wire node except [`Optional`], so `Option<T>` is a
/// persisted type only when `T` is not itself optional.
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
    fn schema() -> SchemaShape {
        let value = Value::schema();
        if value == SchemaShape::U8 {
            SchemaShape::Bytes
        } else {
            SchemaShape::Sequence {
                value: Box::new(value),
            }
        }
    }
}

impl<Value: ResolvedSchema> ResolvedSchema for Vec<Value> {
    type Wire = Vector<Value::Wire>;
}

impl<Value: WireNode, const LENGTH: usize> WireNode for FixedArray<Value, LENGTH> {
    const SHAPE: ConstantShape = ConstantShape::Array(&Value::SHAPE, LENGTH);
    fn schema() -> SchemaShape {
        SchemaShape::Array {
            value: Box::new(Value::schema()),
            length: LENGTH,
        }
    }
}

impl<Value: ResolvedSchema, const LENGTH: usize> ResolvedSchema for [Value; LENGTH] {
    type Wire = FixedArray<Value::Wire, LENGTH>;
}

pub struct End;
pub struct Item<Head, Tail>(PhantomData<(Head, Tail)>);

pub trait WireItems {
    const ITEMS: ConstantItems;
    fn schemas() -> Vec<SchemaShape>;
}

impl WireItems for End {
    const ITEMS: ConstantItems = ConstantItems::End;
    fn schemas() -> Vec<SchemaShape> {
        Vec::new()
    }
}

impl<Head: WireNode, Tail: WireItems> WireItems for Item<Head, Tail> {
    const ITEMS: ConstantItems = ConstantItems::Item(&Head::SHAPE, &Tail::ITEMS);
    fn schemas() -> Vec<SchemaShape> {
        let mut output = vec![Head::schema()];
        output.extend(Tail::schemas());
        output
    }
}

pub struct NameEnd;
pub struct NameByte<const VALUE: u8, Tail>(PhantomData<Tail>);

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
    fn fields() -> Vec<SchemaField>;
}

impl WireFields for End {
    const FIELDS: ConstantFields = ConstantFields::End;
    fn fields() -> Vec<SchemaField> {
        Vec::new()
    }
}

impl<Name: WireName, Value: WireNode, Tail: WireFields> WireFields
    for Item<Field<Name, Value>, Tail>
{
    const FIELDS: ConstantFields = ConstantFields::Field(Name::NAME, &Value::SHAPE, &Tail::FIELDS);
    fn fields() -> Vec<SchemaField> {
        let schema = Value::schema();
        let mut output = vec![SchemaField {
            name: Name::value(),
            presence: if matches!(schema, SchemaShape::Option { .. }) {
                FieldPresence::Optional
            } else {
                FieldPresence::Required
            },
            schema,
        }];
        output.extend(Tail::fields());
        output
    }
}

impl<Fields: WireFields> WireNode for Record<Fields> {
    const SHAPE: ConstantShape = ConstantShape::Record(Fields::FIELDS);
    fn schema() -> SchemaShape {
        SchemaShape::Record {
            fields: Fields::fields(),
        }
    }
}

pub struct Variant<Name, Shape>(PhantomData<(Name, Shape)>);
pub struct UnitVariant;
pub struct NewtypeVariant<Value>(PhantomData<Value>);
pub struct TupleVariant<Items>(PhantomData<Items>);
pub struct RecordVariant<Fields>(PhantomData<Fields>);

pub trait WireVariantNode {
    const SHAPE: ConstantVariant;
    fn shape() -> SchemaVariantShape;
}

impl WireVariantNode for UnitVariant {
    const SHAPE: ConstantVariant = ConstantVariant::Unit;
    fn shape() -> SchemaVariantShape {
        SchemaVariantShape::Unit
    }
}

impl<Value: WireNode> WireVariantNode for NewtypeVariant<Value> {
    const SHAPE: ConstantVariant = ConstantVariant::newtype(&Value::SHAPE);
    fn shape() -> SchemaVariantShape {
        match Value::schema() {
            SchemaShape::Record { fields } => SchemaVariantShape::Record { fields },
            schema => SchemaVariantShape::Newtype {
                schema: Box::new(schema),
            },
        }
    }
}

impl<Items: WireItems> WireVariantNode for TupleVariant<Items> {
    const SHAPE: ConstantVariant = ConstantVariant::Tuple(Items::ITEMS);
    fn shape() -> SchemaVariantShape {
        SchemaVariantShape::Tuple {
            items: Items::schemas(),
        }
    }
}

impl<Fields: WireFields> WireVariantNode for RecordVariant<Fields> {
    const SHAPE: ConstantVariant = ConstantVariant::Record(Fields::FIELDS);
    fn shape() -> SchemaVariantShape {
        SchemaVariantShape::Record {
            fields: Fields::fields(),
        }
    }
}

pub trait WireVariants {
    const VARIANTS: ConstantVariants;
    fn variants() -> Vec<SchemaVariant>;
}

impl WireVariants for End {
    const VARIANTS: ConstantVariants = ConstantVariants::End;
    fn variants() -> Vec<SchemaVariant> {
        Vec::new()
    }
}

impl<Name: WireName, Shape: WireVariantNode, Tail: WireVariants> WireVariants
    for Item<Variant<Name, Shape>, Tail>
{
    const VARIANTS: ConstantVariants =
        ConstantVariants::Variant(Name::NAME, Shape::SHAPE, &Tail::VARIANTS);
    fn variants() -> Vec<SchemaVariant> {
        let mut output = vec![SchemaVariant {
            name: Name::value(),
            shape: Shape::shape(),
        }];
        output.extend(Tail::variants());
        output
    }
}

impl<Variants: WireVariants> WireNode for Enumeration<Variants> {
    const SHAPE: ConstantShape = ConstantShape::Enum(Variants::VARIANTS);
    fn schema() -> SchemaShape {
        SchemaShape::Enum {
            variants: Variants::variants(),
        }
    }
}

/// Implemented only when the two schema types are exactly equal.
pub trait SameSchema<Expected> {}

macro_rules! same_primitive_schema {
    ($($node:ty),+ $(,)?) => {
        $(impl SameSchema<$node> for $node {})+
    };
}

same_primitive_schema!(
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

impl<Current, Expected> SameSchema<Optional<Expected>> for Optional<Current> where
    Current: SameSchema<Expected>
{
}

impl<Current, Expected> SameSchema<Vector<Expected>> for Vector<Current> where
    Current: SameSchema<Expected>
{
}

impl<Current, Expected, const LENGTH: usize> SameSchema<FixedArray<Expected, LENGTH>>
    for FixedArray<Current, LENGTH>
where
    Current: SameSchema<Expected>,
{
}

impl SameSchema<End> for End {}

impl<CurrentHead, ExpectedHead, CurrentTail, ExpectedTail>
    SameSchema<Item<ExpectedHead, ExpectedTail>> for Item<CurrentHead, CurrentTail>
where
    CurrentHead: SameSchema<ExpectedHead>,
    CurrentTail: SameSchema<ExpectedTail>,
{
}

impl<Name, Current, Expected> SameSchema<Field<Name, Expected>> for Field<Name, Current> where
    Current: SameSchema<Expected>
{
}

impl<CurrentFields, ExpectedFields> SameSchema<Record<ExpectedFields>> for Record<CurrentFields> where
    CurrentFields: SameSchema<ExpectedFields>
{
}

impl SameSchema<UnitVariant> for UnitVariant {}

impl<Current, Expected> SameSchema<NewtypeVariant<Expected>> for NewtypeVariant<Current> where
    Current: SameSchema<Expected>
{
}

impl<CurrentItems, ExpectedItems> SameSchema<TupleVariant<ExpectedItems>>
    for TupleVariant<CurrentItems>
where
    CurrentItems: SameSchema<ExpectedItems>,
{
}

impl<CurrentFields, ExpectedFields> SameSchema<RecordVariant<ExpectedFields>>
    for RecordVariant<CurrentFields>
where
    CurrentFields: SameSchema<ExpectedFields>,
{
}

impl<Name, Current, Expected> SameSchema<Variant<Name, Expected>> for Variant<Name, Current> where
    Current: SameSchema<Expected>
{
}

impl<CurrentVariants, ExpectedVariants> SameSchema<Enumeration<ExpectedVariants>>
    for Enumeration<CurrentVariants>
where
    CurrentVariants: SameSchema<ExpectedVariants>,
{
}

#[cfg(test)]
mod tests {
    use super::*;

    type Name = NameByte<b'i', NameByte<b'd', NameEnd>>;
    type Fields = Item<Field<Name, U64>, End>;

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
