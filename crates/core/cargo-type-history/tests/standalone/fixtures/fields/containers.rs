use history_api::{ConstantMembership, Schema, SetMembership, versioned};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::cmp::Ordering;
use std::hash::{BuildHasherDefault, DefaultHasher, Hash, Hasher};
use std::rc::Rc;
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Schema)]
pub struct Exact(pub String);
impl SetMembership for Exact {
    const MEMBERSHIP: ConstantMembership = ConstantMembership::custom("consumer:exact:v1");
}

#[derive(Clone, Debug, Serialize, Deserialize, Schema)]
pub struct Folded(pub String);
impl PartialEq for Folded {
    fn eq(&self, other: &Self) -> bool { self.0.eq_ignore_ascii_case(&other.0) }
}
impl Eq for Folded {}
impl Ord for Folded {
    fn cmp(&self, other: &Self) -> Ordering { self.0.to_ascii_lowercase().cmp(&other.0.to_ascii_lowercase()) }
}
impl PartialOrd for Folded {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> { Some(self.cmp(other)) }
}
impl Hash for Folded {
    fn hash<H: Hasher>(&self, state: &mut H) { self.0.to_ascii_lowercase().hash(state); }
}
impl SetMembership for Folded {
    const MEMBERSHIP: ConstantMembership = ConstantMembership::custom("consumer:folded:v1");
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Schema)]
pub struct Pair(pub u32, pub Option<String>);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Schema)]
pub enum Event {
    Pair(Pair),
    Named { map: BTreeMap<String, (u32,)> },
}

pub type Map = HashMap<String, Option<Pair>>;
pub type Members = HashSet<Exact>;
pub type Values = Vec<String>;
pub type Position = (u32, String);

#[versioned(stable_name = "fields.collections")]
pub struct Collections {
    pub map: Map,
    pub members: Members,
    pub repetitions: Values,
    pub position: Position,
    pub one: (u32,),
    pub pair: Pair,
    pub event: Event,
    pub boxed: Box<Option<u32>>,
    pub shared: Rc<String>,
    pub atomic_shared: Arc<(u32, String)>,
    pub nested: BTreeSet<(u32, String)>,
    pub byte_set: BTreeSet<u8>,
    pub bytes: Vec<u8>,
}

#[cfg(test)]
mod common;

#[cfg(test)]
mod tests {
    use super::{common, Collections, Event, Exact, Folded, Pair};
    use history_api::Versioned;
    use std::collections::{BTreeSet, HashSet};
    use std::rc::Rc;
    use std::sync::Arc;

    fn value() -> Collections {
        Collections {
            map: [("x".into(), Some(Pair(7, Some("seven".into())))), ("none".into(), None)].into_iter().collect(),
            members: [Exact("A".into()), Exact("a".into())].into(),
            repetitions: ["a".into(), "a".into()].into_iter().collect(),
            position: (9, "nine".into()),
            one: (5,),
            pair: Pair(8, None),
            event: Event::Named { map: [("one".into(), (1,))].into() },
            boxed: Box::new(None),
            shared: Rc::new("shared".into()),
            atomic_shared: Arc::new((3, "three".into())),
            nested: [(1, "one".into()), (2, "two".into())].into(),
            byte_set: [0, 255].into(),
            bytes: vec![0, 0, 255],
        }
    }

    #[test]
    fn field_support_collections_read_retained_bytes_and_owned_values() {
        assert_eq!(HashSet::from([Folded("A".into()), Folded("a".into())]).len(), 1);
        assert_eq!(BTreeSet::from([Folded("A".into()), Folded("a".into())]).len(), 1);
        let expected = value();
        let (json, binary) = common::retain("collections", &expected.clone().into_versioned());
        let json: Versioned<Collections> = serde_json::from_slice(&json).unwrap();
        let binary: Versioned<Collections> = rmp_serde::from_slice(&binary).unwrap();
        for stored in [json, binary] {
            assert_eq!(stored.source_version().get(), 1);
            let actual = Collections::from_versioned(stored).unwrap();
            assert_eq!(actual, expected);
            assert_eq!(actual.members.len(), 2, "A and a are distinct members");
            assert_eq!(actual.repetitions.len(), 2);
            assert_eq!(actual.bytes, [0, 0, 255]);
            assert_eq!(*actual.shared, "shared");
            assert_eq!(actual.atomic_shared.0, 3);
        }
    }
}
