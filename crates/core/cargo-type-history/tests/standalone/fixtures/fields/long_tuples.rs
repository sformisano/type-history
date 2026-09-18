use history_api::{versioned as tracked, Schema};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::Arc;

pub type T1 = (u32,);
pub type T2 = (u32, u32,);
pub type T3 = (u32, u32, u32,);
pub type T4 = (u32, u32, u32, u32,);
pub type T5 = (u32, u32, u32, u32, u32,);
pub type T6 = (u32, u32, u32, u32, u32, u32,);
pub type T7 = (u32, u32, u32, u32, u32, u32, u32,);
pub type T8 = (u32, u32, u32, u32, u32, u32, u32, u32,);
pub type T9 = (u32, u32, u32, u32, u32, u32, u32, u32, u32,);
pub type T10 = (u32, u32, u32, u32, u32, u32, u32, u32, u32, u32,);
pub type T11 = (u32, u32, u32, u32, u32, u32, u32, u32, u32, u32, u32,);
pub type T12 = (u32, u32, u32, u32, u32, u32, u32, u32, u32, u32, u32, u32,);
pub type T13 = (u32, u32, u32, u32, u32, u32, u32, u32, u32, u32, u32, u32, u32,);
pub type T14 = (u32, u32, u32, u32, u32, u32, u32, u32, u32, u32, u32, u32, u32, u32,);
pub type T15 = (u32, u32, u32, u32, u32, u32, u32, u32, u32, u32, u32, u32, u32, u32, u32,);
pub type T16 = (u32, u32, u32, u32, u32, u32, u32, u32, u32, u32, u32, u32, u32, u32, u32, u32,);

#[derive(Clone, Serialize, Deserialize, Schema)]
pub struct Newtype(pub T13);
#[derive(Clone, Serialize, Deserialize, Schema)]
pub struct Multi(pub T14, pub u32);
#[derive(Clone, Serialize, Deserialize, Schema)]
pub struct Supporting { pub value: T15 }
#[derive(Clone, Serialize, Deserialize, Schema)]
pub enum Choice { One(Newtype), Many(T16, String) }

#[tracked(stable_name = "fields.tuples", derive_debug = false, derive_partial_eq = false)]
pub struct Tuples {
    pub t1: T1,
    pub t2: T2,
    pub t3: T3,
    pub t4: T4,
    pub t5: T5,
    pub t6: T6,
    pub t7: T7,
    pub t8: T8,
    pub t9: T9,
    pub t10: T10,
    pub t11: T11,
    pub t12: T12,
    pub t13: T13,
    pub t14: T14,
    pub t15: T15,
    pub t16: T16,
    pub nested: Option<Box<Vec<T16>>>,
    pub map: BTreeMap<String, (T13,)>,
    pub array: [T14; 1],
    pub rc: Rc<T15>,
    pub arc: Arc<T16>,
    pub newtype: Newtype,
    pub multi: Multi,
    pub supporting: Supporting,
    pub choice: Choice,
}

#[cfg(test)]
mod common;
#[cfg(test)]
mod tests {
    use super::{common, Choice, Tuples};
    use history_api::Versioned;
    use serde_json::{json, Map, Value};

    #[test]
    fn field_support_all_tuple_positions_and_recursive_aliases() {
        let values = |length: u32| json!((1..=length).collect::<Vec<_>>());
        let mut fields = Map::new();
        for length in 1..=16 { fields.insert(format!("t{length}"), values(length)); }
        for (name, value) in [
            ("nested", json!([values(16)])), ("map", json!({"key":[values(13)]})),
            ("array", json!([values(14)])), ("rc", values(15)), ("arc", values(16)),
            ("newtype", values(13)), ("multi", json!([values(14), 77])),
            ("supporting", json!({"value":values(15)})), ("choice", json!({"Many":[values(16),"sixteen"]})),
        ] { fields.insert(name.into(), value); }
        let expected = Value::Object(fields);
        let value: Tuples = serde_json::from_value(expected.clone()).unwrap();
        let (json, binary) = common::retain("long-tuples", &value.clone().into_versioned());
        let json: Versioned<Tuples> = serde_json::from_slice(&json).unwrap();
        let binary: Versioned<Tuples> = rmp_serde::from_slice(&binary).unwrap();
        for stored in [json, binary] {
            assert_eq!(stored.source_version().get(), 1);
            let actual = Tuples::from_versioned(stored).unwrap();
            assert_eq!([actual.t1.0], [1]);
            assert_eq!([actual.t2.0, actual.t2.1], [1, 2]);
            assert_eq!([actual.t3.0, actual.t3.1, actual.t3.2], [1, 2, 3]);
            assert_eq!([actual.t4.0, actual.t4.1, actual.t4.2, actual.t4.3], [1, 2, 3, 4]);
            assert_eq!([actual.t5.0, actual.t5.1, actual.t5.2, actual.t5.3, actual.t5.4], [1, 2, 3, 4, 5]);
            assert_eq!([actual.t6.0, actual.t6.1, actual.t6.2, actual.t6.3, actual.t6.4, actual.t6.5], [1, 2, 3, 4, 5, 6]);
            assert_eq!([actual.t7.0, actual.t7.1, actual.t7.2, actual.t7.3, actual.t7.4, actual.t7.5, actual.t7.6], [1, 2, 3, 4, 5, 6, 7]);
            assert_eq!([actual.t8.0, actual.t8.1, actual.t8.2, actual.t8.3, actual.t8.4, actual.t8.5, actual.t8.6, actual.t8.7], [1, 2, 3, 4, 5, 6, 7, 8]);
            assert_eq!([actual.t9.0, actual.t9.1, actual.t9.2, actual.t9.3, actual.t9.4, actual.t9.5, actual.t9.6, actual.t9.7, actual.t9.8], [1, 2, 3, 4, 5, 6, 7, 8, 9]);
            assert_eq!([actual.t10.0, actual.t10.1, actual.t10.2, actual.t10.3, actual.t10.4, actual.t10.5, actual.t10.6, actual.t10.7, actual.t10.8, actual.t10.9], [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
            assert_eq!([actual.t11.0, actual.t11.1, actual.t11.2, actual.t11.3, actual.t11.4, actual.t11.5, actual.t11.6, actual.t11.7, actual.t11.8, actual.t11.9, actual.t11.10], [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]);
            assert_eq!([actual.t12.0, actual.t12.1, actual.t12.2, actual.t12.3, actual.t12.4, actual.t12.5, actual.t12.6, actual.t12.7, actual.t12.8, actual.t12.9, actual.t12.10, actual.t12.11], [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]);
            assert_eq!([actual.t13.0, actual.t13.1, actual.t13.2, actual.t13.3, actual.t13.4, actual.t13.5, actual.t13.6, actual.t13.7, actual.t13.8, actual.t13.9, actual.t13.10, actual.t13.11, actual.t13.12], [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13]);
            assert_eq!([actual.t14.0, actual.t14.1, actual.t14.2, actual.t14.3, actual.t14.4, actual.t14.5, actual.t14.6, actual.t14.7, actual.t14.8, actual.t14.9, actual.t14.10, actual.t14.11, actual.t14.12, actual.t14.13], [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14]);
            assert_eq!([actual.t15.0, actual.t15.1, actual.t15.2, actual.t15.3, actual.t15.4, actual.t15.5, actual.t15.6, actual.t15.7, actual.t15.8, actual.t15.9, actual.t15.10, actual.t15.11, actual.t15.12, actual.t15.13, actual.t15.14], [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]);
            assert_eq!([actual.t16.0, actual.t16.1, actual.t16.2, actual.t16.3, actual.t16.4, actual.t16.5, actual.t16.6, actual.t16.7, actual.t16.8, actual.t16.9, actual.t16.10, actual.t16.11, actual.t16.12, actual.t16.13, actual.t16.14, actual.t16.15], [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]);
            assert_eq!(actual.nested.as_ref().unwrap()[0].15, 16);
            assert_eq!(actual.map["key"].0.12, 13);
            assert_eq!(actual.array[0].13, 14);
            assert_eq!(actual.rc.14, 15);
            assert_eq!(actual.arc.15, 16);
            assert_eq!(actual.newtype.0.12, 13);
            assert_eq!(actual.multi.0.13, 14);
            assert_eq!(actual.supporting.value.14, 15);
            match &actual.choice { Choice::Many(value, _) => assert_eq!(value.15, 16), _ => panic!("wrong variant") }
            assert_eq!(serde_json::to_value(actual).unwrap(), expected);
        }
    }
}
