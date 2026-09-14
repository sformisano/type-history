//! Normalization into the one closed JSON Schema representation.
use super::{JsonSchemaError, JsonSchemaErrorReason, NULL_TYPE};
use crate::canonical;
use serde_json::{Map, Value};

/// Normalize one document in place. Idempotent.
pub(super) fn normalize(value: &mut Value) -> Result<(), JsonSchemaError> {
    normalize_node(value, "", NodeContext::Value)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum NodeContext {
    /// An ordinary value node.
    Value,
    /// One entry of an enum `oneOf`: a unit `const` or a variant wrapper.
    Variant,
}

fn normalize_node(
    value: &mut Value,
    path: &str,
    context: NodeContext,
) -> Result<(), JsonSchemaError> {
    let Some(object) = value.as_object_mut() else {
        return Err(JsonSchemaError::at(
            path,
            JsonSchemaErrorReason::NotAnObject,
        ));
    };
    object.remove("title");
    object.remove("description");
    for keyword in ["$ref", "$defs", "definitions"] {
        if object.contains_key(keyword) {
            return Err(JsonSchemaError::at(path, JsonSchemaErrorReason::Reference));
        }
    }
    if context == NodeContext::Value {
        if let Some(mut inner) = strip_null(object, path)? {
            normalize_node(&mut inner, path, NodeContext::Value)?;
            *value = optional_wrapper(inner);
            return Ok(());
        }
    }
    let object = value.as_object_mut().expect("still an object");
    if object.get("type") == Some(&Value::String("string".to_owned())) {
        if let Some(constant) = object.remove("const") {
            object.remove("type");
            object.insert("const".to_owned(), constant);
        } else if let Some(Value::Array(names)) = object.get_mut("enum") {
            if names.len() == 1 {
                let name = names.remove(0);
                object.remove("type");
                object.remove("enum");
                object.insert("const".to_owned(), name);
            } else {
                sort_by_canonical_text(names)?;
            }
        }
    }
    if let Some(Value::Object(properties)) = object.get_mut("properties") {
        for (name, property) in properties.iter_mut() {
            normalize_node(
                property,
                &format!("{path}/properties/{name}"),
                NodeContext::Value,
            )?;
        }
    }
    if let Some(items) = object.get_mut("items") {
        normalize_node(items, &format!("{path}/items"), NodeContext::Value)?;
    }
    if let Some(Value::Array(items)) = object.get_mut("prefixItems") {
        for (index, item) in items.iter_mut().enumerate() {
            normalize_node(
                item,
                &format!("{path}/prefixItems/{index}"),
                NodeContext::Value,
            )?;
        }
    }
    if let Some(Value::Array(entries)) = object.get_mut("oneOf") {
        let mut expanded = Vec::with_capacity(entries.len());
        for (index, entry) in entries.iter_mut().enumerate() {
            normalize_node(
                entry,
                &format!("{path}/oneOf/{index}"),
                NodeContext::Variant,
            )?;
            match unit_names(entry) {
                Some(names) => expanded.extend(names.into_iter().map(unit_constant)),
                None => expanded.push(entry.take()),
            }
        }
        sort_by_canonical_text(&mut expanded)?;
        *entries = expanded;
    }
    if let Some(Value::Array(entries)) = object.get_mut("anyOf") {
        for (index, entry) in entries.iter_mut().enumerate() {
            normalize_node(entry, &format!("{path}/anyOf/{index}"), NodeContext::Value)?;
        }
    }
    if context == NodeContext::Value
        && object.get("type") == Some(&Value::String("object".to_owned()))
    {
        let properties = match object
            .entry("properties")
            .or_insert_with(|| Value::Object(Map::new()))
        {
            Value::Object(properties) => properties,
            _ => {
                return Err(JsonSchemaError::at(
                    path,
                    JsonSchemaErrorReason::InvalidKeyword("properties".to_owned()),
                ))
            }
        };
        let required = properties
            .iter()
            .filter(|(_, schema)| !is_optional_wrapper(schema))
            .map(|(name, _)| Value::String(name.clone()))
            .collect::<Vec<_>>();
        object.insert("required".to_owned(), Value::Array(required));
    }
    if let Some(Value::Array(required)) = object.get_mut("required") {
        sort_by_canonical_text(required)?;
    }
    Ok(())
}

/// Remove `null` admission from a raw `schemars` node, returning the inner node
/// when the node was optional.
fn strip_null(
    object: &mut Map<String, Value>,
    path: &str,
) -> Result<Option<Value>, JsonSchemaError> {
    if object.len() == 1 {
        if let Some(Value::Array(alternatives)) = object.get("anyOf") {
            let null_count = alternatives
                .iter()
                .filter(|value| is_null_schema(value))
                .count();
            if null_count == 1 && alternatives.len() == 2 {
                let inner = alternatives
                    .iter()
                    .find(|value| !is_null_schema(value))
                    .cloned()
                    .expect("one non-null alternative");
                return Ok(Some(inner));
            }
            if null_count > 0 {
                return Err(JsonSchemaError::at(path, JsonSchemaErrorReason::Option));
            }
        }
    }
    let nullable_type = match object.get("type") {
        Some(Value::Array(types)) => types.iter().any(|value| value == NULL_TYPE),
        _ => false,
    };
    if !nullable_type {
        return Ok(None);
    }
    let mut inner = object.clone();
    if let Some(Value::Array(types)) = inner.get_mut("type") {
        types.retain(|value| value != NULL_TYPE);
        if types.len() == 1 {
            let only = types.remove(0);
            inner.insert("type".to_owned(), only);
        }
    }
    if let Some(Value::Array(names)) = inner.get_mut("enum") {
        names.retain(|value| !value.is_null());
    }
    Ok(Some(Value::Object(inner)))
}

fn is_null_schema(value: &Value) -> bool {
    value.as_object().is_some_and(|object| {
        object.len() == 1 && object.get("type") == Some(&Value::String(NULL_TYPE.to_owned()))
    })
}

pub(super) fn optional_wrapper(inner: Value) -> Value {
    let mut null = Map::new();
    null.insert("type".to_owned(), Value::String(NULL_TYPE.to_owned()));
    let mut wrapper = Map::new();
    wrapper.insert(
        "anyOf".to_owned(),
        Value::Array(vec![inner, Value::Object(null)]),
    );
    Value::Object(wrapper)
}

/// Whether a normalized node is the optional wrapper.
fn is_optional_wrapper(value: &Value) -> bool {
    optional_inner(value).is_some()
}

pub(super) fn optional_inner(value: &Value) -> Option<&Value> {
    let object = value.as_object()?;
    if object.len() != 1 {
        return None;
    }
    let alternatives = object.get("anyOf")?.as_array()?;
    if alternatives.len() != 2 || !is_null_schema(&alternatives[1]) {
        return None;
    }
    Some(&alternatives[0])
}

/// Names of a normalized unit-only enumeration node, if the node is one.
fn unit_names(value: &Value) -> Option<Vec<Value>> {
    let object = value.as_object()?;
    if object.len() == 2 && object.get("type") == Some(&Value::String("string".to_owned())) {
        if let Some(Value::Array(names)) = object.get("enum") {
            return Some(names.clone());
        }
    }
    None
}

pub(super) fn unit_constant(name: Value) -> Value {
    let mut object = Map::new();
    object.insert("const".to_owned(), name);
    Value::Object(object)
}

pub(super) fn sort_by_canonical_text(values: &mut [Value]) -> Result<(), JsonSchemaError> {
    let mut keyed = values
        .iter()
        .map(|value| canonical::value_to_vec(value).map(|bytes| (bytes, value.clone())))
        .collect::<Result<Vec<_>, _>>()?;
    keyed.sort_by(|left, right| left.0.cmp(&right.0));
    for (slot, (_, value)) in values.iter_mut().zip(keyed) {
        *slot = value;
    }
    Ok(())
}
