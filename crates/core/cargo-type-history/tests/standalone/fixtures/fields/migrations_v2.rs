use history_api::{versioned, Schema};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::convert::Infallible;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Schema)]
pub struct Nullable(pub Option<u32>);

#[versioned(stable_name = "fields.migration")]
pub struct Record {
    #[history(updated_in = v2, previous_type = Vec<String>, backfill_fn = deduplicate)]
    pub items: BTreeSet<String>,
    #[history(updated_in = v2, previous_type = Option<u32>, backfill_fn = require)]
    pub value: Nullable,
    pub required: Nullable,
}

fn deduplicate(old: &RecordV1) -> Result<BTreeSet<String>, Infallible> {
    Ok(old.items.iter().cloned().collect())
}
fn require(old: &RecordV1) -> Result<Nullable, Infallible> { Ok(Nullable(old.value)) }

#[cfg(test)]
mod tests {
    use super::Record;
    use history_api::Versioned;
    use serde_json::Value;
    use std::fs;
    #[test]
    fn field_support_explicit_migration_preserves_old_version_until_conversion() {
        let json: Versioned<Record> = serde_json::from_slice(&fs::read("old.json").unwrap()).unwrap();
        let binary: Versioned<Record> = rmp_serde::from_slice(&fs::read("old.msgpack").unwrap()).unwrap();
        for stored in [json, binary] {
            assert_eq!(stored.source_version().get(), 1);
            let old: Value = serde_json::to_value(&stored).unwrap();
            assert_eq!(old["payload"]["items"].as_array().unwrap().len(), 3);
            let current = Record::from_versioned(stored).unwrap();
            assert_eq!(current.items.into_iter().collect::<Vec<_>>(), ["a", "b"]);
            assert_eq!(current.value.0, None);
            assert_eq!(current.required.0, None);
        }
    }
}
