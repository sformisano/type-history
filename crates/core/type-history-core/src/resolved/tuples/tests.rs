use crate::resolved::{
    export_json_schema, membership::rules, FieldSchema, ResolvedSchema, SchemaShape, SetMembership,
    WireNode,
};
use schemars::JsonSchema;
use serde::{de::DeserializeOwned, Serialize};

fn round_trip<T: ResolvedSchema + Serialize + DeserializeOwned>(value: T, length: usize)
where
    FieldSchema<T>: JsonSchema,
{
    let saved = serde_json::to_vec(&value).unwrap();
    let restored: T = serde_json::from_slice(&saved).unwrap();
    // Arity13..16 lacks std PartialEq, so compare the exact JSON value positions.
    assert_eq!(
        serde_json::to_value(&value).unwrap(),
        serde_json::to_value(restored).unwrap()
    );
    let schema = T::resolved_wire_schema();
    let SchemaShape::Tuple { items } = schema else {
        panic!("tuple contract expected");
    };
    assert_eq!(items.len(), length);
    let document = export_json_schema::<FieldSchema<T>>();
    assert_eq!(document["prefixItems"].as_array().unwrap().len(), length);
    assert_eq!(document["minItems"], length);
    assert_eq!(document["maxItems"], length);
}

#[test]
fn every_supported_tuple_arity_preserves_its_positions() {
    round_trip((1u32,), 1);
    round_trip((1u32, 2u32), 2);
    round_trip((1u32, 2u32, 3u32), 3);
    round_trip((1u32, 2u32, 3u32, 4u32), 4);
    round_trip((1u32, 2u32, 3u32, 4u32, 5u32), 5);
    round_trip((1u32, 2u32, 3u32, 4u32, 5u32, 6u32), 6);
    round_trip((1u32, 2u32, 3u32, 4u32, 5u32, 6u32, 7u32), 7);
    round_trip((1u32, 2u32, 3u32, 4u32, 5u32, 6u32, 7u32, 8u32), 8);
    round_trip((1u32, 2u32, 3u32, 4u32, 5u32, 6u32, 7u32, 8u32, 9u32), 9);
    round_trip(
        (1u32, 2u32, 3u32, 4u32, 5u32, 6u32, 7u32, 8u32, 9u32, 10u32),
        10,
    );
    round_trip(
        (
            1u32, 2u32, 3u32, 4u32, 5u32, 6u32, 7u32, 8u32, 9u32, 10u32, 11u32,
        ),
        11,
    );
    round_trip(
        (
            1u32, 2u32, 3u32, 4u32, 5u32, 6u32, 7u32, 8u32, 9u32, 10u32, 11u32, 12u32,
        ),
        12,
    );
    round_trip(
        (
            1u32, 2u32, 3u32, 4u32, 5u32, 6u32, 7u32, 8u32, 9u32, 10u32, 11u32, 12u32, 13u32,
        ),
        13,
    );
    round_trip(
        (
            1u32, 2u32, 3u32, 4u32, 5u32, 6u32, 7u32, 8u32, 9u32, 10u32, 11u32, 12u32, 13u32, 14u32,
        ),
        14,
    );
    round_trip(
        (
            1u32, 2u32, 3u32, 4u32, 5u32, 6u32, 7u32, 8u32, 9u32, 10u32, 11u32, 12u32, 13u32,
            14u32, 15u32,
        ),
        15,
    );
    round_trip(
        (
            1u32, 2u32, 3u32, 4u32, 5u32, 6u32, 7u32, 8u32, 9u32, 10u32, 11u32, 12u32, 13u32,
            14u32, 15u32, 16u32,
        ),
        16,
    );
}

#[test]
fn tuple_contracts_keep_order_arity_nullable_positions_and_membership() {
    type First = (u32, String);
    type Second = (String, u32);
    assert!(!<First as ResolvedSchema>::Wire::SHAPE.same(&<Second as ResolvedSchema>::Wire::SHAPE));
    assert_ne!(
        <(u32,)>::resolved_wire_schema(),
        u32::resolved_wire_schema()
    );
    assert_ne!(
        <(u32,)>::resolved_wire_schema(),
        <(u32, u32)>::resolved_wire_schema()
    );
    assert!(!First::MEMBERSHIP.same(&Second::MEMBERSHIP));
    assert_eq!(First::MEMBERSHIP.id, rules::TUPLE.id);
    assert!(First::MEMBERSHIP.parameters[0].same(&rules::U32));
    assert!(First::MEMBERSHIP.parameters[1].same(&rules::STRING));
    round_trip((None::<u32>, Some("value".to_owned())), 2);
    assert!(serde_json::from_str::<(Option<u32>,)>("[]").is_err());
    assert!(serde_json::from_str::<(u32,)>("[1,2]").is_err());
    assert_eq!(
        serde_json::from_str::<(Option<u32>,)>("[null]").unwrap(),
        (None,)
    );
    let value = Some((None::<u32>,));
    let restored: Option<(Option<u32>,)> =
        serde_json::from_slice(&serde_json::to_vec(&value).unwrap()).unwrap();
    assert_eq!(value, restored);
}
