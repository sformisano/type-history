//! Standard owned containers preserve element contracts and field admission.

use super::{FieldSchema, JsonSchemaField, Map, ResolvedSchema, Set, SetMembership};
use schemars::{Schema, SchemaGenerator};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::hash::{BuildHasher, Hash};
#[cfg(feature = "rc")]
use std::{rc::Rc, sync::Arc};

impl<Value: ResolvedSchema, Hasher> ResolvedSchema for HashMap<String, Value, Hasher> {
    type Wire = Map<Value::Wire>;
}
impl<Value: ResolvedSchema> ResolvedSchema for BTreeMap<String, Value> {
    type Wire = Map<Value::Wire>;
}

impl<Value: ResolvedSchema + SetMembership + Eq + Hash, Hasher: BuildHasher> ResolvedSchema
    for HashSet<Value, Hasher>
{
    type Wire = Set<Value::Wire, Value>;
}
impl<Value: ResolvedSchema + SetMembership + Ord> ResolvedSchema for BTreeSet<Value> {
    type Wire = Set<Value::Wire, Value>;
}

impl<Value: JsonSchemaField, Hasher> JsonSchemaField for HashMap<String, Value, Hasher> {
    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        map_schema::<Value>(generator)
    }
}
impl<Value: JsonSchemaField> JsonSchemaField for BTreeMap<String, Value> {
    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        map_schema::<Value>(generator)
    }
}

impl<Value: JsonSchemaField + SetMembership + Eq + Hash, Hasher: BuildHasher> JsonSchemaField
    for HashSet<Value, Hasher>
{
    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        set_schema::<Value>(generator)
    }
}
impl<Value: JsonSchemaField + SetMembership + Ord> JsonSchemaField for BTreeSet<Value> {
    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        set_schema::<Value>(generator)
    }
}

fn map_schema<Value: JsonSchemaField>(generator: &mut SchemaGenerator) -> Schema {
    let value = generator.subschema_for::<FieldSchema<Value>>();
    schemars::json_schema!({"type": "object", "additionalProperties": value})
}

fn set_schema<Value: JsonSchemaField + SetMembership>(generator: &mut SchemaGenerator) -> Schema {
    let value = generator.subschema_for::<FieldSchema<Value>>();
    schemars::json_schema!({
        "type": "array", "items": value,
        "x-type-history-membership": Value::MEMBERSHIP.to_json(),
    })
}

macro_rules! owned_wrapper {
    ($wrapper:ident) => {
        impl<Value: ResolvedSchema> ResolvedSchema for $wrapper<Value> {
            type Wire = Value::Wire;
        }
        impl<Value: JsonSchemaField> JsonSchemaField for $wrapper<Value> {
            fn json_schema(generator: &mut SchemaGenerator) -> Schema {
                Value::json_schema(generator)
            }
        }
    };
}

owned_wrapper!(Box);
#[cfg(feature = "rc")]
owned_wrapper!(Rc);
#[cfg(feature = "rc")]
owned_wrapper!(Arc);

#[cfg(test)]
mod tests;
