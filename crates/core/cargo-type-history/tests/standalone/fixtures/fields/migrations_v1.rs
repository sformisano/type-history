use history_api::{versioned, Schema};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Schema)]
pub struct Nullable(pub Option<u32>);

#[versioned(stable_name = "fields.migration")]
pub struct Record {
    pub items: Vec<String>,
    pub value: Option<u32>,
    pub required: Nullable,
}

// RUNTIME_TESTS_BEGIN
#[cfg(test)]
mod common;
#[cfg(test)]
mod tests {
    use super::{common, Record};
    use history_api::Versioned;
    use serde_json::json;
    use std::fs;
    #[test]
    fn field_support_save_old_duplicates_and_omitted_field() {
        let value = json!({"stable_name":"fields.migration","version":1,"payload":{"items":["a","a","b"],"required":null}});
        let json = serde_json::to_vec(&value).unwrap();
        let binary = common::messagepack(&value);
        let stored: Versioned<Record> = serde_json::from_slice(&json).unwrap();
        assert_eq!(Record::from_versioned(stored).unwrap().items.len(), 3);
        let stored: Versioned<Record> = rmp_serde::from_slice(&binary).unwrap();
        assert_eq!(Record::from_versioned(stored).unwrap().items.len(), 3);
        fs::write("old.json", json).unwrap(); fs::write("old.msgpack", binary).unwrap();
    }
}
// RUNTIME_TESTS_END
