//! Exact static representation of the closed wire language.

use super::{FieldPresence, SchemaField, SchemaShape, SchemaVariant, SchemaVariantShape};

#[derive(Clone, Copy)]
pub enum ConstantShape {
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
    Option(&'static Self),
    Sequence(&'static Self),
    Array(&'static Self, usize),
    Record(ConstantFields),
    Enum(ConstantVariants),
}
#[derive(Clone, Copy)]
pub enum ConstantName {
    End,
    Byte(u8, &'static Self),
}
#[derive(Clone, Copy)]
pub enum ConstantItems {
    End,
    Item(&'static ConstantShape, &'static Self),
}
#[derive(Clone, Copy)]
pub enum ConstantFields {
    End,
    Field(ConstantName, &'static ConstantShape, &'static Self),
}
#[derive(Clone, Copy)]
pub enum ConstantVariants {
    End,
    Variant(ConstantName, ConstantVariant, &'static Self),
}
#[derive(Clone, Copy)]
pub enum ConstantVariant {
    Unit,
    Newtype(&'static ConstantShape),
    Tuple(ConstantItems),
    Record(ConstantFields),
}

impl ConstantShape {
    pub const fn same(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Bool, Self::Bool)
            | (Self::I8, Self::I8)
            | (Self::I16, Self::I16)
            | (Self::I32, Self::I32)
            | (Self::I64, Self::I64)
            | (Self::I128, Self::I128)
            | (Self::U8, Self::U8)
            | (Self::U16, Self::U16)
            | (Self::U32, Self::U32)
            | (Self::U64, Self::U64)
            | (Self::U128, Self::U128)
            | (Self::String, Self::String)
            | (Self::Bytes, Self::Bytes) => true,
            (Self::Option(a), Self::Option(b)) | (Self::Sequence(a), Self::Sequence(b)) => {
                a.same(b)
            }
            (Self::Array(a, an), Self::Array(b, bn)) => *an == *bn && a.same(b),
            (Self::Record(a), Self::Record(b)) => a.same(b),
            (Self::Enum(a), Self::Enum(b)) => a.same(b),
            _ => false,
        }
    }

    pub fn schema(&self) -> SchemaShape {
        match self {
            Self::Bool => SchemaShape::Bool,
            Self::I8 => SchemaShape::I8,
            Self::I16 => SchemaShape::I16,
            Self::I32 => SchemaShape::I32,
            Self::I64 => SchemaShape::I64,
            Self::I128 => SchemaShape::I128,
            Self::U8 => SchemaShape::U8,
            Self::U16 => SchemaShape::U16,
            Self::U32 => SchemaShape::U32,
            Self::U64 => SchemaShape::U64,
            Self::U128 => SchemaShape::U128,
            Self::String => SchemaShape::String,
            Self::Bytes => SchemaShape::Bytes,
            Self::Option(value) => SchemaShape::Option {
                value: Box::new(value.schema()),
            },
            Self::Sequence(value) => SchemaShape::Sequence {
                value: Box::new(value.schema()),
            },
            Self::Array(value, length) => SchemaShape::Array {
                value: Box::new(value.schema()),
                length: *length,
            },
            Self::Record(fields) => SchemaShape::Record {
                fields: fields.fields(),
            },
            Self::Enum(variants) => SchemaShape::Enum {
                variants: variants.variants(),
            },
        }
    }
}
impl ConstantName {
    const fn same(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::End, Self::End) => true,
            (Self::Byte(a, at), Self::Byte(b, bt)) => *a == *b && at.same(bt),
            _ => false,
        }
    }
    fn name(&self) -> String {
        let mut bytes = Vec::new();
        let mut name = self;
        while let Self::Byte(byte, tail) = name {
            bytes.push(*byte);
            name = tail;
        }
        String::from_utf8(bytes).expect("Type History generated a UTF-8 wire name")
    }
}
impl ConstantItems {
    const fn same(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::End, Self::End) => true,
            (Self::Item(a, at), Self::Item(b, bt)) => a.same(b) && at.same(bt),
            _ => false,
        }
    }
    fn items(&self) -> Vec<SchemaShape> {
        let mut values = Vec::new();
        let mut next = self;
        while let Self::Item(value, tail) = next {
            values.push(value.schema());
            next = tail;
        }
        values
    }
}
impl ConstantFields {
    const fn same(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::End, Self::End) => true,
            (Self::Field(an, a, at), Self::Field(bn, b, bt)) => {
                an.same(bn) && a.same(b) && at.same(bt)
            }
            _ => false,
        }
    }
    fn fields(&self) -> Vec<SchemaField> {
        let mut values = Vec::new();
        let mut next = self;
        while let Self::Field(name, value, tail) = next {
            values.push(SchemaField {
                name: name.name(),
                presence: if matches!(value, ConstantShape::Option(_)) {
                    FieldPresence::Optional
                } else {
                    FieldPresence::Required
                },
                schema: value.schema(),
            });
            next = tail;
        }
        values
    }
}
impl ConstantVariant {
    /// Canonicalize an externally tagged newtype containing a named record.
    pub const fn newtype(value: &'static ConstantShape) -> Self {
        match value {
            ConstantShape::Record(fields) => Self::Record(*fields),
            value => Self::Newtype(value),
        }
    }

    const fn same(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Unit, Self::Unit) => true,
            (Self::Newtype(a), Self::Newtype(b)) => a.same(b),
            (Self::Tuple(a), Self::Tuple(b)) => a.same(b),
            (Self::Record(a), Self::Record(b)) => a.same(b),
            (Self::Newtype(ConstantShape::Record(a)), Self::Record(b))
            | (Self::Record(a), Self::Newtype(ConstantShape::Record(b))) => a.same(b),
            _ => false,
        }
    }
    fn shape(&self) -> SchemaVariantShape {
        match self {
            Self::Unit => SchemaVariantShape::Unit,
            Self::Newtype(ConstantShape::Record(fields)) => SchemaVariantShape::Record {
                fields: fields.fields(),
            },
            Self::Newtype(value) => SchemaVariantShape::Newtype {
                schema: Box::new(value.schema()),
            },
            Self::Tuple(items) => SchemaVariantShape::Tuple {
                items: items.items(),
            },
            Self::Record(fields) => SchemaVariantShape::Record {
                fields: fields.fields(),
            },
        }
    }
}
impl ConstantVariants {
    const fn same(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::End, Self::End) => true,
            (Self::Variant(an, a, at), Self::Variant(bn, b, bt)) => {
                an.same(bn) && a.same(b) && at.same(bt)
            }
            _ => false,
        }
    }
    fn variants(&self) -> Vec<SchemaVariant> {
        let mut values = Vec::new();
        let mut next = self;
        while let Self::Variant(name, value, tail) = next {
            values.push(SchemaVariant {
                name: name.name(),
                shape: value.shape(),
            });
            next = tail;
        }
        values
    }
}

#[cfg(test)]
mod tests {
    use super::super::{
        Bool, End, Enumeration, Field, FixedArray, Item, NameByte, NameEnd, NewtypeVariant,
        Optional, Record, RecordVariant, StringValue, TupleVariant, UnitVariant, Variant, Vector,
        WireNode, I128, I16, I32, I64, I8, U128, U16, U32, U64, U8,
    };
    use super::ConstantShape;
    use crate::resolved::SchemaShape;

    fn add<T: WireNode>(values: &mut Vec<(ConstantShape, SchemaShape)>) {
        values.push((T::SHAPE, T::schema()));
    }

    #[test]
    fn exact_constant_comparison_matches_every_runtime_constructor() {
        type A = NameByte<b'a', NameEnd>;
        type B = NameByte<b'b', NameEnd>;
        type Fields = Item<Field<A, Optional<U32>>, Item<Field<B, Vector<U8>>, End>>;
        let mut values = Vec::new();
        macro_rules! nodes { ($($node:ty),* $(,)?) => { $(add::<$node>(&mut values);)* }; }
        nodes!(Bool,I8,I16,I32,I64,I128,U8,U16,U32,U64,U128,StringValue,
            Optional<U32>,Optional<U64>,Vector<U8>,Vector<U32>,Vector<U64>,
            FixedArray<U8,1>,FixedArray<U8,2>,FixedArray<U32,2>,Record<End>,Record<Fields>,
            Record<Item<Field<A,U32>,End>>,Record<Item<Field<B,U32>,End>>,
            Enumeration<Item<Variant<A,UnitVariant>,End>>,
            Enumeration<Item<Variant<B,UnitVariant>,End>>,
            Enumeration<Item<Variant<A,NewtypeVariant<U32>>,End>>,
            Enumeration<Item<Variant<A,TupleVariant<Item<U32,End>>>,End>>,
            Enumeration<Item<Variant<A,RecordVariant<Fields>>,End>>,
            Enumeration<Item<Variant<A,NewtypeVariant<Record<Fields>>>,End>>,
            Optional<Record<Fields>>,Vector<Record<Fields>>);
        for (constant, runtime) in &values {
            assert_eq!(constant.schema(), *runtime);
        }
        for (a, ar) in &values {
            for (b, br) in &values {
                assert_eq!(a.same(b), ar == br);
            }
        }
    }
}
