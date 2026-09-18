//! Equality declarations for standard owned values and containers.

use super::{rules, ConstantMembership, SetMembership};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::hash::{BuildHasher, Hash};
#[cfg(feature = "rc")]
use std::{rc::Rc, sync::Arc};

macro_rules! primitive_membership {
    ($($rust:ty => $rule:ident),+ $(,)?) => {
        $(impl SetMembership for $rust {
            const MEMBERSHIP: ConstantMembership = rules::$rule;
        })+
    };
}

primitive_membership! {
    bool => BOOL, i8 => I8, i16 => I16, i32 => I32, i64 => I64, i128 => I128,
    u8 => U8, u16 => U16, u32 => U32, u64 => U64, u128 => U128, String => STRING,
}

impl<Value: SetMembership + Eq> SetMembership for Option<Value> {
    const MEMBERSHIP: ConstantMembership = ConstantMembership {
        id: rules::OPTION.id,
        parameters: &[Value::MEMBERSHIP],
    };
}
impl<Value: SetMembership + Eq> SetMembership for Vec<Value> {
    const MEMBERSHIP: ConstantMembership = ConstantMembership {
        id: rules::SEQUENCE.id,
        parameters: &[Value::MEMBERSHIP],
    };
}
impl<Value: SetMembership + Eq, const LENGTH: usize> SetMembership for [Value; LENGTH] {
    const MEMBERSHIP: ConstantMembership = ConstantMembership {
        id: rules::ARRAY.id,
        parameters: &[Value::MEMBERSHIP],
    };
}
impl<Value: SetMembership + Eq, Hasher: BuildHasher> SetMembership
    for HashMap<String, Value, Hasher>
{
    const MEMBERSHIP: ConstantMembership = ConstantMembership {
        id: rules::MAP.id,
        parameters: &[Value::MEMBERSHIP],
    };
}
impl<Value: SetMembership + Eq> SetMembership for BTreeMap<String, Value> {
    const MEMBERSHIP: ConstantMembership = ConstantMembership {
        id: rules::MAP.id,
        parameters: &[Value::MEMBERSHIP],
    };
}
impl<Value: SetMembership + Eq + Hash, Hasher: BuildHasher> SetMembership
    for HashSet<Value, Hasher>
{
    const MEMBERSHIP: ConstantMembership = ConstantMembership {
        id: rules::SET.id,
        parameters: &[Value::MEMBERSHIP],
    };
}
impl<Value: SetMembership + Ord> SetMembership for BTreeSet<Value> {
    const MEMBERSHIP: ConstantMembership = ConstantMembership {
        id: rules::SET.id,
        parameters: &[Value::MEMBERSHIP],
    };
}

macro_rules! owned_membership {
    ($wrapper:ident) => {
        impl<Value: SetMembership + Eq> SetMembership for $wrapper<Value> {
            const MEMBERSHIP: ConstantMembership = Value::MEMBERSHIP;
        }
    };
}
owned_membership!(Box);
#[cfg(feature = "rc")]
owned_membership!(Rc);
#[cfg(feature = "rc")]
owned_membership!(Arc);
