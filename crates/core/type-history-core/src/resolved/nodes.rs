//! Structural markers for maps, sets, tuples, profiles, and transparent newtypes.

use super::{
    ConstantShape, NonOptionalNode, ProfileMarker, SameSchema, SchemaShape, SetMembership,
    WireItems, WireNode,
};
use std::marker::PhantomData;

pub struct Map<Value>(PhantomData<Value>);
pub struct Set<Value, Member>(PhantomData<(Value, Member)>);
pub struct Tuple<Items>(PhantomData<Items>);
pub struct Profile<Marker>(PhantomData<Marker>);
pub struct Newtype<Value>(PhantomData<Value>);

impl<Value: WireNode> WireNode for Map<Value> {
    const SHAPE: ConstantShape = ConstantShape::Map(&Value::SHAPE);
    fn schema() -> SchemaShape {
        Self::SHAPE.schema()
    }
}
impl<Value: WireNode> NonOptionalNode for Map<Value> {}

impl<Value: WireNode, Member: SetMembership> WireNode for Set<Value, Member> {
    const SHAPE: ConstantShape = ConstantShape::Set(&Value::SHAPE, Member::MEMBERSHIP);
    fn schema() -> SchemaShape {
        Self::SHAPE.schema()
    }
}
impl<Value: WireNode, Member: SetMembership> NonOptionalNode for Set<Value, Member> {}

impl<Items: WireItems> WireNode for Tuple<Items> {
    const SHAPE: ConstantShape = ConstantShape::Tuple(Items::ITEMS);
    fn schema() -> SchemaShape {
        Self::SHAPE.schema()
    }
}
impl<Items: WireItems> NonOptionalNode for Tuple<Items> {}

impl<Marker: ProfileMarker> WireNode for Profile<Marker> {
    const SHAPE: ConstantShape = ConstantShape::Profile(Marker::PROFILE);
    fn schema() -> SchemaShape {
        Self::SHAPE.schema()
    }
}
impl<Marker: ProfileMarker> NonOptionalNode for Profile<Marker> {}

impl<Value: WireNode> WireNode for Newtype<Value> {
    const SHAPE: ConstantShape = Value::SHAPE;
    fn schema() -> SchemaShape {
        Value::schema()
    }
}
impl<Value: NonOptionalNode> NonOptionalNode for Newtype<Value> {}

impl<Current, Expected> SameSchema<Map<Expected>> for Map<Current> where
    Current: SameSchema<Expected>
{
}
impl<Current, Expected, Member> SameSchema<Set<Expected, Member>> for Set<Current, Member> where
    Current: SameSchema<Expected>
{
}
impl<Current, Expected> SameSchema<Tuple<Expected>> for Tuple<Current> where
    Current: SameSchema<Expected>
{
}
impl<Marker> SameSchema<Profile<Marker>> for Profile<Marker> {}
impl<Current, Expected> SameSchema<Newtype<Expected>> for Newtype<Current> where
    Current: SameSchema<Expected>
{
}
