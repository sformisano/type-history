//! Strict reading of normalized JSON Schema into structural shapes.
use super::normalize::optional_inner;
use super::{presence_of, JsonSchemaError, JsonSchemaErrorReason};
use serde_json::{Map, Value};
use std::collections::BTreeSet;
use type_history_core::resolved::{
    FieldPresence, SchemaField, SchemaShape, SchemaVariant, SchemaVariantShape,
};

/// Read a normalized document into its structural shape.
pub fn parse(value: &Value) -> Result<SchemaShape, JsonSchemaError> {
    let object = value
        .as_object()
        .ok_or_else(|| JsonSchemaError::at("", JsonSchemaErrorReason::NotAnObject))?;
    let mut node = object.clone();
    node.remove("$schema");
    node.remove("$id");
    parse_node(&node, "").map(SchemaShape::normalized)
}

const NODE_KEYWORDS: [&str; 14] = [
    "type",
    "format",
    "minimum",
    "maximum",
    "items",
    "minItems",
    "maxItems",
    "prefixItems",
    "properties",
    "required",
    "additionalProperties",
    "enum",
    "const",
    "oneOf",
];

fn parse_node(object: &Map<String, Value>, path: &str) -> Result<SchemaShape, JsonSchemaError> {
    let error = |reason| JsonSchemaError::at(path, reason);
    if object.contains_key("anyOf") {
        let whole = Value::Object(object.clone());
        let inner = optional_inner(&whole).ok_or_else(|| error(JsonSchemaErrorReason::Option))?;
        let inner = inner.as_object().ok_or_else(|| {
            JsonSchemaError::at(
                &format!("{path}/anyOf/0"),
                JsonSchemaErrorReason::NotAnObject,
            )
        })?;
        let value = parse_node(inner, &format!("{path}/anyOf/0"))?;
        if matches!(value, SchemaShape::Option { .. }) {
            return Err(error(JsonSchemaErrorReason::Option));
        }
        return Ok(SchemaShape::Option {
            value: Box::new(value),
        });
    }
    for keyword in object.keys() {
        if !NODE_KEYWORDS.contains(&keyword.as_str()) {
            return Err(error(JsonSchemaErrorReason::UnknownKeyword(
                keyword.clone(),
            )));
        }
    }
    let keys = |allowed: &[&str]| -> Result<(), JsonSchemaError> {
        object
            .keys()
            .find(|key| !allowed.contains(&key.as_str()))
            .map_or(Ok(()), |key| {
                Err(error(JsonSchemaErrorReason::UnknownKeyword(key.clone())))
            })
    };
    if let Some(constant) = object.get("const") {
        keys(&["const"])?;
        let name = constant
            .as_str()
            .ok_or_else(|| error(JsonSchemaErrorReason::Enum))?;
        return Ok(SchemaShape::Enum {
            variants: vec![SchemaVariant {
                name: name.to_owned(),
                shape: SchemaVariantShape::Unit,
            }],
        });
    }
    if let Some(entries) = object.get("oneOf") {
        keys(&["oneOf"])?;
        let entries = entries
            .as_array()
            .ok_or_else(|| error(JsonSchemaErrorReason::InvalidKeyword("oneOf".to_owned())))?;
        let variants = entries
            .iter()
            .enumerate()
            .map(|(index, entry)| parse_variant(entry, &format!("{path}/oneOf/{index}")))
            .collect::<Result<Vec<_>, _>>()?;
        reject_duplicate_names(
            variants.iter().map(|variant| variant.name.as_str()),
            path,
            JsonSchemaErrorReason::Enum,
        )?;
        return Ok(SchemaShape::Enum { variants });
    }
    let type_name = object
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| error(JsonSchemaErrorReason::Type))?;
    match type_name {
        "boolean" => {
            keys(&["type"])?;
            Ok(SchemaShape::Bool)
        }
        "string" => {
            keys(&["type", "enum"])?;
            match object.get("enum") {
                None => Ok(SchemaShape::String),
                Some(names) => {
                    let names = names
                        .as_array()
                        .ok_or_else(|| error(JsonSchemaErrorReason::Enum))?;
                    if names.len() == 1 {
                        return Err(error(JsonSchemaErrorReason::Enum));
                    }
                    let variants = names
                        .iter()
                        .map(|name| {
                            name.as_str()
                                .map(|name| SchemaVariant {
                                    name: name.to_owned(),
                                    shape: SchemaVariantShape::Unit,
                                })
                                .ok_or_else(|| error(JsonSchemaErrorReason::Enum))
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    reject_duplicate_names(
                        variants.iter().map(|variant| variant.name.as_str()),
                        path,
                        JsonSchemaErrorReason::Enum,
                    )?;
                    Ok(SchemaShape::Enum { variants })
                }
            }
        }
        "integer" => {
            keys(&["type", "format", "minimum", "maximum"])?;
            parse_integer(object, path)
        }
        "array" => {
            keys(&["type", "items", "minItems", "maxItems"])?;
            let items = object
                .get("items")
                .and_then(Value::as_object)
                .ok_or_else(|| error(JsonSchemaErrorReason::InvalidKeyword("items".to_owned())))?;
            let value = parse_node(items, &format!("{path}/items"))?;
            match (object.get("minItems"), object.get("maxItems")) {
                (None, None) => Ok(if value == SchemaShape::U8 {
                    SchemaShape::Bytes
                } else {
                    SchemaShape::Sequence {
                        value: Box::new(value),
                    }
                }),
                (Some(minimum), Some(maximum)) => {
                    let length = integer(minimum, path)?;
                    if integer(maximum, path)? != length {
                        return Err(error(JsonSchemaErrorReason::ArrayLength));
                    }
                    let length = usize::try_from(length)
                        .map_err(|_| error(JsonSchemaErrorReason::ArrayLength))?;
                    Ok(SchemaShape::Array {
                        value: Box::new(value),
                        length,
                    })
                }
                _ => Err(error(JsonSchemaErrorReason::ArrayLength)),
            }
        }
        "object" => {
            keys(&["type", "properties", "required", "additionalProperties"])?;
            Ok(SchemaShape::Record {
                fields: parse_fields(object, path)?,
            })
        }
        _ => Err(error(JsonSchemaErrorReason::Type)),
    }
}

fn parse_fields(
    object: &Map<String, Value>,
    path: &str,
) -> Result<Vec<SchemaField>, JsonSchemaError> {
    let error = |reason| JsonSchemaError::at(path, reason);
    if object.get("additionalProperties") != Some(&Value::Bool(false)) {
        return Err(error(JsonSchemaErrorReason::AdditionalProperties));
    }
    let properties = object
        .get("properties")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            error(JsonSchemaErrorReason::InvalidKeyword(
                "properties".to_owned(),
            ))
        })?;
    let required = object
        .get("required")
        .and_then(Value::as_array)
        .ok_or_else(|| error(JsonSchemaErrorReason::Required))?;
    let mut fields = Vec::with_capacity(properties.len());
    let mut expected_required = Vec::new();
    for (name, property) in properties {
        let property = property.as_object().ok_or_else(|| {
            JsonSchemaError::at(
                &format!("{path}/properties/{name}"),
                JsonSchemaErrorReason::NotAnObject,
            )
        })?;
        let schema = parse_node(property, &format!("{path}/properties/{name}"))?;
        let presence = presence_of(&schema);
        if presence == FieldPresence::Required {
            expected_required.push(Value::String(name.clone()));
        }
        fields.push(SchemaField {
            name: name.clone(),
            presence,
            schema,
        });
    }
    if required != &expected_required {
        return Err(error(JsonSchemaErrorReason::Required));
    }
    Ok(fields)
}

fn parse_variant(entry: &Value, path: &str) -> Result<SchemaVariant, JsonSchemaError> {
    let error = |reason| JsonSchemaError::at(path, reason);
    let object = entry
        .as_object()
        .ok_or_else(|| error(JsonSchemaErrorReason::NotAnObject))?;
    if let Some(constant) = object.get("const") {
        if object.len() != 1 {
            return Err(error(JsonSchemaErrorReason::Variant));
        }
        let name = constant
            .as_str()
            .ok_or_else(|| error(JsonSchemaErrorReason::Variant))?;
        return Ok(SchemaVariant {
            name: name.to_owned(),
            shape: SchemaVariantShape::Unit,
        });
    }
    let is_wrapper = object.len() == 4
        && object.get("type") == Some(&Value::String("object".to_owned()))
        && object.get("additionalProperties") == Some(&Value::Bool(false));
    if !is_wrapper {
        return Err(error(JsonSchemaErrorReason::Variant));
    }
    let properties = object
        .get("properties")
        .and_then(Value::as_object)
        .filter(|properties| properties.len() == 1)
        .ok_or_else(|| error(JsonSchemaErrorReason::Variant))?;
    let (name, payload) = properties.iter().next().expect("one property");
    if object.get("required") != Some(&Value::Array(vec![Value::String(name.clone())])) {
        return Err(error(JsonSchemaErrorReason::Variant));
    }
    let payload_path = format!("{path}/properties/{name}");
    let payload = payload
        .as_object()
        .ok_or_else(|| JsonSchemaError::at(&payload_path, JsonSchemaErrorReason::NotAnObject))?;
    let shape = if let Some(items) = payload.get("prefixItems") {
        for keyword in payload.keys() {
            if !["type", "prefixItems", "minItems", "maxItems"].contains(&keyword.as_str()) {
                return Err(JsonSchemaError::at(
                    &payload_path,
                    JsonSchemaErrorReason::UnknownKeyword(keyword.clone()),
                ));
            }
        }
        let items = items
            .as_array()
            .ok_or_else(|| JsonSchemaError::at(&payload_path, JsonSchemaErrorReason::Tuple))?;
        if payload.get("type") != Some(&Value::String("array".to_owned())) || items.len() < 2 {
            return Err(JsonSchemaError::at(
                &payload_path,
                JsonSchemaErrorReason::Tuple,
            ));
        }
        let length = u64::try_from(items.len()).expect("tuple length fits");
        if payload
            .get("minItems")
            .map(|value| integer(value, &payload_path))
            .transpose()?
            != Some(length)
            || payload
                .get("maxItems")
                .map(|value| integer(value, &payload_path))
                .transpose()?
                != Some(length)
        {
            return Err(JsonSchemaError::at(
                &payload_path,
                JsonSchemaErrorReason::Tuple,
            ));
        }
        let items = items
            .iter()
            .enumerate()
            .map(|(index, item)| {
                let item_path = format!("{payload_path}/prefixItems/{index}");
                item.as_object()
                    .ok_or_else(|| {
                        JsonSchemaError::at(&item_path, JsonSchemaErrorReason::NotAnObject)
                    })
                    .and_then(|item| parse_node(item, &item_path))
            })
            .collect::<Result<Vec<_>, _>>()?;
        SchemaVariantShape::Tuple { items }
    } else {
        match parse_node(payload, &payload_path)? {
            SchemaShape::Record { fields } => SchemaVariantShape::Record { fields },
            schema => SchemaVariantShape::Newtype {
                schema: Box::new(schema),
            },
        }
    };
    Ok(SchemaVariant {
        name: name.clone(),
        shape,
    })
}

fn parse_integer(object: &Map<String, Value>, path: &str) -> Result<SchemaShape, JsonSchemaError> {
    let error = |reason| JsonSchemaError::at(path, reason);
    let format = object
        .get("format")
        .and_then(Value::as_str)
        .ok_or_else(|| error(JsonSchemaErrorReason::IntegerFormat))?;
    let (shape, expected_minimum, expected_maximum) = match format {
        "int8" => (SchemaShape::I8, Some(-128), Some(127)),
        "int16" => (SchemaShape::I16, Some(-32_768), Some(32_767)),
        "int32" => (SchemaShape::I32, None, None),
        "int64" => (SchemaShape::I64, None, None),
        "int128" => (SchemaShape::I128, None, None),
        "uint8" => (SchemaShape::U8, Some(0), Some(255)),
        "uint16" => (SchemaShape::U16, Some(0), Some(65_535)),
        "uint32" => (SchemaShape::U32, Some(0), None),
        "uint64" => (SchemaShape::U64, Some(0), None),
        "uint128" => (SchemaShape::U128, Some(0), None),
        _ => return Err(error(JsonSchemaErrorReason::IntegerFormat)),
    };
    let minimum = object
        .get("minimum")
        .map(|value| signed(value, path))
        .transpose()?;
    let maximum = object
        .get("maximum")
        .map(|value| signed(value, path))
        .transpose()?;
    if minimum != expected_minimum || maximum != expected_maximum {
        return Err(error(JsonSchemaErrorReason::IntegerBounds));
    }
    Ok(shape)
}

fn integer(value: &Value, path: &str) -> Result<u64, JsonSchemaError> {
    value
        .as_u64()
        .ok_or_else(|| JsonSchemaError::at(path, JsonSchemaErrorReason::Number))
}

fn signed(value: &Value, path: &str) -> Result<i64, JsonSchemaError> {
    value
        .as_i64()
        .ok_or_else(|| JsonSchemaError::at(path, JsonSchemaErrorReason::Number))
}

fn reject_duplicate_names<'a>(
    names: impl Iterator<Item = &'a str>,
    path: &str,
    reason: JsonSchemaErrorReason,
) -> Result<(), JsonSchemaError> {
    let mut seen = BTreeSet::new();
    for name in names {
        if !seen.insert(name) {
            return Err(JsonSchemaError::at(path, reason));
        }
    }
    Ok(())
}
