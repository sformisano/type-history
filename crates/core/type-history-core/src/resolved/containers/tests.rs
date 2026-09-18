use crate::resolved::{
    membership::rules, ConstantMembership, FieldPresence, JsonSchemaField, Newtype, Optional,
    ResolvedSchema, SchemaShape, SetMembership, StringValue, WireNode, U32,
};
use schemars::{generate::SchemaSettings, Schema, SchemaGenerator};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::hash::{BuildHasherDefault, DefaultHasher};
#[cfg(feature = "rc")]
use std::{rc::Rc, sync::Arc};

type FixedHasher = BuildHasherDefault<DefaultHasher>;

fn schema<T: JsonSchemaField>() -> Value {
    let mut generator = SchemaSettings::draft2020_12()
        .with(|settings| settings.inline_subschemas = true)
        .into_generator();
    T::json_schema(&mut generator).to_value()
}

fn same_contract<A: ResolvedSchema, B: ResolvedSchema>() {
    assert!(A::Wire::SHAPE.same(&B::Wire::SHAPE));
    assert_eq!(A::resolved_wire_schema(), B::resolved_wire_schema());
    assert!(A::Wire::FIELD_PRESENCE.same(B::Wire::FIELD_PRESENCE));
}

#[test]
fn maps_ignore_implementation_and_hasher_but_preserve_nested_values() {
    type ValueType = Option<Vec<(u32, String)>>;
    type Hashed = HashMap<String, ValueType>;
    type Fixed = HashMap<String, ValueType, FixedHasher>;
    type Ordered = BTreeMap<String, ValueType>;
    same_contract::<Hashed, Ordered>();
    same_contract::<Fixed, Ordered>();
    assert_eq!(schema::<Hashed>(), schema::<Ordered>());
    assert_eq!(schema::<Fixed>(), schema::<Ordered>());
    let document = schema::<Hashed>();
    assert_eq!(document["type"], "object");
    assert!(document.get("properties").is_none());
    assert!(document.get("required").is_none());
    assert!(document.get("additionalProperties").is_some());
    assert_ne!(
        Hashed::resolved_wire_schema(),
        BTreeMap::<String, u64>::resolved_wire_schema()
    );
    let values: Hashed = [
        ("first".to_owned(), Some(vec![(42, "A".into())])),
        ("second".to_owned(), None),
    ]
    .into();
    let saved = serde_json::to_vec(&values).unwrap();
    let restored: Ordered = serde_json::from_slice(&saved).unwrap();
    assert_eq!(restored["first"], Some(vec![(42, "A".into())]));
    assert_eq!(restored["second"], None);
}

#[test]
fn sets_preserve_explicit_membership_and_never_become_bytes_or_sequences() {
    same_contract::<HashSet<String>, BTreeSet<String>>();
    same_contract::<HashSet<String, FixedHasher>, BTreeSet<String>>();
    assert_eq!(schema::<HashSet<String>>(), schema::<BTreeSet<String>>());
    let document = schema::<HashSet<u8>>();
    assert_eq!(document["type"], "array");
    assert_eq!(document["x-type-history-membership"], rules::U8.to_json());
    assert!(document.get("uniqueItems").is_none());
    assert_ne!(
        HashSet::<u8>::resolved_wire_schema(),
        Vec::<u8>::resolved_wire_schema()
    );
    assert_eq!(Vec::<u8>::resolved_wire_schema(), SchemaShape::Bytes);
    assert_ne!(
        HashSet::<String>::resolved_wire_schema(),
        Vec::<String>::resolved_wire_schema()
    );
    let exact: HashSet<String> = ["A".to_owned(), "a".to_owned()].into();
    let restored: BTreeSet<String> =
        serde_json::from_slice(&serde_json::to_vec(&exact).unwrap()).unwrap();
    assert_eq!(restored.into_iter().collect::<Vec<_>>(), ["A", "a"]);
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
struct Exact(String);
impl ResolvedSchema for Exact {
    type Wire = Newtype<StringValue>;
}
impl JsonSchemaField for Exact {
    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        String::json_schema(generator)
    }
}
impl SetMembership for Exact {
    const MEMBERSHIP: ConstantMembership = ConstantMembership::custom("example:exact:v1");
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Hash)]
struct Changed(String);
impl ResolvedSchema for Changed {
    type Wire = Newtype<StringValue>;
}
impl SetMembership for Changed {
    const MEMBERSHIP: ConstantMembership = ConstantMembership::custom("example:exact:v2");
}

#[test]
fn custom_membership_is_explicit_and_frozen_with_the_element_shape() {
    same_contract::<HashSet<Exact>, BTreeSet<Exact>>();
    assert_ne!(
        BTreeSet::<Exact>::resolved_wire_schema(),
        BTreeSet::<Changed>::resolved_wire_schema()
    );
    assert!(<Exact as ResolvedSchema>::Wire::SHAPE.same(&<Changed as ResolvedSchema>::Wire::SHAPE));
    assert_eq!(
        schema::<BTreeSet<Exact>>()["x-type-history-membership"],
        Exact::MEMBERSHIP.to_json()
    );
    let exact: HashSet<Exact> = [Exact("A".into()), Exact("a".into())].into();
    let restored: BTreeSet<Exact> =
        serde_json::from_slice(&serde_json::to_vec(&exact).unwrap()).unwrap();
    assert_eq!(restored.len(), 2);
}

#[test]
fn builtin_membership_composes_through_supported_containers() {
    assert!(HashMap::<String, u32>::MEMBERSHIP.same(&BTreeMap::<String, u32>::MEMBERSHIP));
    assert!(HashSet::<String>::MEMBERSHIP.same(&BTreeSet::<String>::MEMBERSHIP));
    assert!(HashSet::<String, FixedHasher>::MEMBERSHIP.same(&HashSet::<String>::MEMBERSHIP));
    assert!(Box::<Option<u32>>::MEMBERSHIP.same(&Option::<u32>::MEMBERSHIP));
    assert_eq!(
        Option::<Vec<[u32; 2]>>::MEMBERSHIP.to_json(),
        json!({
            "id": rules::OPTION.id, "parameters": [{
                "id": rules::SEQUENCE.id, "parameters": [{
                    "id": rules::ARRAY.id, "parameters": [{"id": rules::U32.id, "parameters": []}]
                }]
            }]
        })
    );
    type Element = BTreeMap<String, Option<(String, u32)>>;
    same_contract::<HashSet<Element>, BTreeSet<Element>>();
    assert_eq!(schema::<HashSet<Element>>(), schema::<BTreeSet<Element>>());
    assert_eq!(
        BTreeSet::<BTreeSet<String>>::MEMBERSHIP.parameters[0].id,
        rules::SET.id
    );
}

#[derive(Deserialize)]
struct Nullable(Option<u32>);
impl ResolvedSchema for Nullable {
    type Wire = Newtype<Optional<U32>>;
}
impl JsonSchemaField for Nullable {
    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        Option::<u32>::json_schema(generator)
    }
}

#[test]
fn owned_indirection_forwards_presence_without_hiding_nullable_newtypes() {
    same_contract::<Box<Option<u32>>, Option<u32>>();
    same_contract::<Box<Nullable>, Nullable>();
    assert_eq!(
        <Box<Option<u32>> as ResolvedSchema>::Wire::FIELD_PRESENCE,
        FieldPresence::Optional
    );
    assert_eq!(
        <Box<Nullable> as ResolvedSchema>::Wire::FIELD_PRESENCE,
        FieldPresence::Required
    );
    assert_eq!(schema::<Box<Nullable>>(), schema::<Option<u32>>());
    assert_eq!(
        schema::<Box<Vec<(u32, String)>>>(),
        schema::<Vec<(u32, String)>>()
    );
    same_contract::<Option<(Option<u32>,)>, Box<Option<(Option<u32>,)>>>();
    #[derive(Deserialize)]
    struct Omittable {
        value: Box<Option<u32>>,
    }
    #[derive(Deserialize)]
    struct Required {
        value: Box<Nullable>,
    }
    assert!(serde_json::from_str::<Omittable>("{}")
        .unwrap()
        .value
        .is_none());
    assert!(serde_json::from_str::<Required>("{}").is_err());
    assert!(serde_json::from_str::<Required>(r#"{"value":null}"#)
        .unwrap()
        .value
        .0
        .is_none());
}

#[cfg(feature = "rc")]
#[test]
fn shared_ownership_wrappers_preserve_by_value_schema_membership_and_presence() {
    same_contract::<Rc<Option<u32>>, Option<u32>>();
    same_contract::<Arc<Option<u32>>, Option<u32>>();
    same_contract::<Rc<Nullable>, Nullable>();
    same_contract::<Arc<Nullable>, Nullable>();
    assert!(Rc::<(u32, String)>::MEMBERSHIP.same(&<(u32, String)>::MEMBERSHIP));
    assert!(Arc::<(u32, String)>::MEMBERSHIP.same(&<(u32, String)>::MEMBERSHIP));
    assert_eq!(schema::<Rc<Nullable>>(), schema::<Option<u32>>());
    assert_eq!(schema::<Arc<Nullable>>(), schema::<Option<u32>>());
    #[derive(Deserialize)]
    struct Omittable {
        rc: Rc<Option<u32>>,
        arc: Arc<Option<u32>>,
    }
    #[derive(Deserialize)]
    struct Required {
        rc: Rc<Nullable>,
        arc: Arc<Nullable>,
    }
    let omitted: Omittable = serde_json::from_str("{}").unwrap();
    assert!(omitted.rc.is_none() && omitted.arc.is_none());
    assert!(serde_json::from_str::<Required>("{}").is_err());
    let explicit: Required = serde_json::from_str(r#"{"rc":null,"arc":null}"#).unwrap();
    assert!(explicit.rc.0.is_none() && explicit.arc.0.is_none());
    let saved = serde_json::to_vec(&Rc::new(vec![1u32, 2])).unwrap();
    let restored: Arc<Vec<u32>> = serde_json::from_slice(&saved).unwrap();
    assert_eq!(*restored, [1, 2]);
}
