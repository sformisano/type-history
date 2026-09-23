//! Structural markers for maps, sets, tuples, profiles, and transparent newtypes.

use super::{ConstantShape, NonOptionalNode, ProfileMarker, SetMembership, WireItems, WireNode};
use std::marker::PhantomData;

pub struct Map<Value>(PhantomData<Value>);
pub struct Set<Value, Member>(PhantomData<(Value, Member)>);
pub struct Tuple<Items>(PhantomData<Items>);
pub struct Profile<Marker>(PhantomData<Marker>);
pub struct Newtype<Value>(PhantomData<Value>);

impl<Value: WireNode> WireNode for Map<Value> {
    const SHAPE: ConstantShape = ConstantShape::Map(&Value::SHAPE);
}
impl<Value: WireNode> NonOptionalNode for Map<Value> {}

impl<Value: WireNode, Member: SetMembership> WireNode for Set<Value, Member> {
    const SHAPE: ConstantShape = ConstantShape::Set(&Value::SHAPE, Member::MEMBERSHIP);
}
impl<Value: WireNode, Member: SetMembership> NonOptionalNode for Set<Value, Member> {}

impl<Items: WireItems> WireNode for Tuple<Items> {
    const SHAPE: ConstantShape = ConstantShape::Tuple(Items::ITEMS);
}
impl<Items: WireItems> NonOptionalNode for Tuple<Items> {}

impl<Marker: ProfileMarker> WireNode for Profile<Marker> {
    const SHAPE: ConstantShape = ConstantShape::Profile(Marker::PROFILE);
}
impl<Marker: ProfileMarker> NonOptionalNode for Profile<Marker> {}

impl<Value: WireNode> WireNode for Newtype<Value> {
    const SHAPE: ConstantShape = Value::SHAPE;
}
impl<Value: NonOptionalNode> NonOptionalNode for Newtype<Value> {}
