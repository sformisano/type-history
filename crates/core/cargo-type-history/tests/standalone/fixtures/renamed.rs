use history_api::Schema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Schema)]
#[serde(deny_unknown_fields)]
pub struct Nested {
    pub label: String,
}

#[history_api::versioned(stable_name = "example.nested.child")]
pub struct Child {
    pub nested: Nested,
}

#[history_api::versioned(stable_name = "example.nested.parent")]
pub struct Parent {
    pub child: Option<Child>,
}

#[cfg(test)]
mod tests {
    use super::{Child, Nested, Parent};
    use history_api::{decode, HasHistory, PayloadVersion, Versioned};
    use serde_json::{from_slice, json, to_value, to_vec};

    #[test]
    fn nested_histories_keep_values_in_raw_and_versioned_json() {
        let parent = Parent {
            child: Some(Child {
                nested: Nested { label: "nested value".to_owned() },
            }),
        };
        assert_eq!(
            to_value(&parent).unwrap(),
            json!({"child": {"nested": {"label": "nested value"}}})
        );
        assert!(format!("{parent:?}").contains("nested value"));
        for parent in [parent, Parent { child: None }] {
            let raw = to_vec(&parent).unwrap();
            let decoded = decode::<Parent>(&Parent::STABLE_NAME, PayloadVersion::INITIAL, &raw)
                .unwrap();
            assert_eq!(decoded, parent);
            let wire = to_vec(&parent.clone().into_versioned()).unwrap();
            let stored: Versioned<Parent> = from_slice(&wire).unwrap();
            assert_eq!(stored.source_version(), PayloadVersion::INITIAL);
            assert_eq!(Parent::from_versioned(stored).unwrap(), parent);
        }
    }
}
