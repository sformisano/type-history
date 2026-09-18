use history_api::{versioned, Schema};
use serde::{Deserialize, Serialize};
use std::rc::Rc;
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Schema)]
pub struct Nullable(pub Option<u32>);
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Schema)]
pub struct Nested(pub Nullable);
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Schema)]
pub struct Required(pub u32);
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Schema)]
pub struct Pair(pub Option<u32>, pub u32);
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Schema)]
pub enum Choice {
    Named { optional: Option<u32>, required: Nullable },
    Nullable(Nullable),
    Pair(Pair),
}

#[versioned(stable_name = "fields.presence")]
pub struct Presence {
    pub direct: Option<u32>,
    pub boxed_optional: Box<Option<u32>>,
    pub rc_optional: Rc<Option<u32>>,
    pub arc_optional: Arc<Option<u32>>,
    pub required: Nullable,
    pub nested: Nested,
    pub boxed_required: Box<Nullable>,
    pub rc_required: Rc<Nullable>,
    pub arc_required: Arc<Nullable>,
    pub optional_newtype: Option<Required>,
    pub positional: (Option<u32>,),
    pub pair: Pair,
    pub choice: Choice,
}

#[cfg(test)]
mod common;
#[cfg(test)]
mod tests {
    use super::common::{MetadataFirst, PayloadFirst};
    use super::{common, Presence};
    use history_api::Versioned;
    use serde_json::{json, Value};

    fn payload() -> Value {
        json!({"direct":null,"boxed_optional":null,"rc_optional":null,"arc_optional":null,
            "required":null,"nested":null,"boxed_required":null,"rc_required":null,"arc_required":null,
            "optional_newtype":null,"positional":[null],"pair":[null,1],
            "choice":{"Named":{"required":null}}})
    }

    fn accepted(payload: &Value, expected: bool) {
        let stable_name = "fields.presence";
        let first = PayloadFirst { payload, version: 1, stable_name };
        let last = MetadataFirst { payload, version: 1, stable_name };
        for bytes in [serde_json::to_vec(&first).unwrap(), serde_json::to_vec(&last).unwrap()] {
            assert_eq!(serde_json::from_slice::<Versioned<Presence>>(&bytes).is_ok(), expected, "{}", String::from_utf8_lossy(&bytes));
        }
        let payload = json!({"stable_name":stable_name,"version":1,"payload":payload});
        assert_eq!(rmp_serde::from_slice::<Versioned<Presence>>(&common::messagepack(&payload)).is_ok(), expected, "{payload}");
        // Reordering uses ordinary small fixture integer serialization.
        assert_eq!(rmp_serde::from_slice::<Versioned<Presence>>(&common::payload_first_binary(stable_name, &payload["payload"])).is_ok(), expected, "{payload}");
    }

    #[test]
    fn field_support_presence_missing_is_distinct_from_null() {
        accepted(&payload(), true);
        for name in ["direct", "boxed_optional", "rc_optional", "arc_optional", "optional_newtype"] {
            let mut value = payload();
            value.as_object_mut().unwrap().remove(name);
            accepted(&value, true);
        }
        for name in ["required", "nested", "boxed_required", "rc_required", "arc_required", "positional", "pair", "choice"] {
            let mut value = payload();
            value.as_object_mut().unwrap().remove(name);
            accepted(&value, false);
        }
        for choice in [json!({"Nullable":null}), json!({"Pair":[null,1]}), json!({"Named":{"required":null}})] {
            let mut value = payload(); value["choice"] = choice; accepted(&value, true);
        }
        for choice in [json!({}), json!({"Named":{}}), json!({"Pair":[null]})] {
            let mut value = payload(); value["choice"] = choice; accepted(&value, false);
        }
        let mut value = payload(); value["positional"] = json!([]); accepted(&value, false);
    }
}
