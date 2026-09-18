use history_api::{versioned, Schema};
use serde::{Deserialize, Serialize};
use std::fmt::{Debug, Formatter, Result as FmtResult};

#[derive(Clone, Serialize, Deserialize, Schema)]
pub struct Secret(pub String);
impl PartialEq for Secret {
    fn eq(&self, other: &Self) -> bool { self.0.eq_ignore_ascii_case(&other.0) }
}
impl Debug for Secret {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult { f.write_str("[redacted]") }
}

#[derive(Schema)]
pub struct SchemaOnly { pub value: u32 }

#[versioned(stable_name = "fields.native")]
pub struct Native {
    pub direct: Secret,
    pub nested: Vec<Secret>,
}

#[cfg(test)]
mod common;
#[cfg(test)]
mod tests {
    use super::{common, Native, SchemaOnly, Secret};
    use history_api::{ResolvedSchema, Versioned};

    // NATIVE_CHECK_BEGIN
    #[test]
    fn default_traits_delegate_native_custom_semantics() {
        let first = Native { direct: Secret("private".into()), nested: vec![Secret("value".into())] };
        let second = Native { direct: Secret("PRIVATE".into()), nested: vec![Secret("VALUE".into())] };
        assert_eq!(first, second);
        let debug = format!("{first:?}");
        assert!(!debug.contains("private")); assert!(!debug.contains("value"));
        assert_eq!(debug.matches("[redacted]").count(), 2);
    }
    // NATIVE_CHECK_END

    #[test]
    fn storage_and_schema_are_independent_of_native_traits() {
        let _ = SchemaOnly::resolved_wire_schema();
        let value = Native { direct: Secret("private".into()), nested: vec![Secret("value".into())] };
        let current = value.clone().into_versioned();
        let (json, binary) = common::retain("native-traits", &current);
        assert_eq!(json, serde_json::to_vec(&current).unwrap());
        assert_eq!(binary, rmp_serde::to_vec_named(&current).unwrap());
        let json: Versioned<Native> = serde_json::from_slice(&json).unwrap();
        let binary: Versioned<Native> = rmp_serde::from_slice(&binary).unwrap();
        for stored in [json, binary] {
            let actual = Native::from_versioned(stored).unwrap();
            assert_eq!(actual.direct.0, "private");
            assert_eq!(actual.nested[0].0, "value");
        }
    }
}
