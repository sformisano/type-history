//! Canonical closed-shape JSON Schema writer.
use super::collections::MEMBERSHIP_KEY;
use super::normalize::{optional_wrapper, sort_by_canonical_text, unit_constant};
use super::profiles::PROFILE_KEY;
use serde_json::{Map, Value};
use type_history_core::resolved::{
    FieldPresence, SchemaField, SchemaShape, SchemaVariant, SchemaVariantShape,
};

pub(super) fn write_node(shape: &SchemaShape) -> Value {
    let mut object = Map::new();
    match shape {
        SchemaShape::Bool => {
            object.insert("type".to_owned(), "boolean".into());
        }
        SchemaShape::String => {
            object.insert("type".to_owned(), "string".into());
        }
        SchemaShape::I8 => return write_integer("int8", Some((-128, 127))),
        SchemaShape::I16 => return write_integer("int16", Some((-32_768, 32_767))),
        SchemaShape::I32 => return write_integer("int32", None),
        SchemaShape::I64 => return write_integer("int64", None),
        SchemaShape::I128 => return write_integer("int128", None),
        SchemaShape::U8 => return write_integer("uint8", Some((0, 255))),
        SchemaShape::U16 => return write_integer("uint16", Some((0, 65_535))),
        SchemaShape::U32 => return write_unsigned("uint32"),
        SchemaShape::U64 => return write_unsigned("uint64"),
        SchemaShape::U128 => return write_unsigned("uint128"),
        SchemaShape::Bytes => {
            object.insert("type".to_owned(), "array".into());
            object.insert("items".to_owned(), write_node(&SchemaShape::U8));
        }
        SchemaShape::Option { value } => return optional_wrapper(write_node(value)),
        SchemaShape::Sequence { value } => {
            object.insert("type".to_owned(), "array".into());
            object.insert("items".to_owned(), write_node(value));
        }
        SchemaShape::Array { value, length } => {
            object.insert("type".to_owned(), "array".into());
            object.insert("items".to_owned(), write_node(value));
            object.insert("minItems".to_owned(), (*length).into());
            object.insert("maxItems".to_owned(), (*length).into());
        }
        SchemaShape::Map { value } => {
            object.insert("type".to_owned(), "object".into());
            object.insert("additionalProperties".to_owned(), write_node(value));
        }
        SchemaShape::Set { value, membership } => {
            object.insert("type".to_owned(), "array".into());
            object.insert("items".to_owned(), write_node(value));
            object.insert(MEMBERSHIP_KEY.to_owned(), membership.to_json());
        }
        SchemaShape::Tuple { items } => return write_tuple(items),
        SchemaShape::Profile { profile } => {
            object.insert("type".to_owned(), profile.json_type().into());
            object.insert(PROFILE_KEY.to_owned(), profile.id().into());
        }
        SchemaShape::Record { fields } => return write_record(fields),
        SchemaShape::Enum { variants } => return write_enum(variants),
    }
    Value::Object(object)
}

fn write_tuple(items: &[SchemaShape]) -> Value {
    assert!(
        !items.is_empty(),
        "Type History tuples require at least one item"
    );
    let mut tuple = Map::new();
    tuple.insert("type".to_owned(), "array".into());
    tuple.insert(
        "prefixItems".to_owned(),
        Value::Array(items.iter().map(write_node).collect()),
    );
    tuple.insert("minItems".to_owned(), items.len().into());
    tuple.insert("maxItems".to_owned(), items.len().into());
    Value::Object(tuple)
}

fn write_integer(format: &str, bounds: Option<(i64, i64)>) -> Value {
    let mut object = Map::new();
    object.insert("type".to_owned(), "integer".into());
    object.insert("format".to_owned(), format.into());
    if let Some((minimum, maximum)) = bounds {
        object.insert("minimum".to_owned(), minimum.into());
        object.insert("maximum".to_owned(), maximum.into());
    }
    Value::Object(object)
}

fn write_unsigned(format: &str) -> Value {
    let mut object = Map::new();
    object.insert("type".to_owned(), "integer".into());
    object.insert("format".to_owned(), format.into());
    object.insert("minimum".to_owned(), 0.into());
    Value::Object(object)
}

fn write_record(fields: &[SchemaField]) -> Value {
    let mut properties = Map::new();
    let mut required = Vec::new();
    let mut sorted = fields.iter().collect::<Vec<_>>();
    sorted.sort_by(|left, right| left.name.cmp(&right.name));
    for field in sorted {
        properties.insert(field.name.clone(), write_node(&field.schema));
        if field.presence == FieldPresence::Required {
            required.push(Value::String(field.name.clone()));
        }
    }
    sort_by_canonical_text(&mut required).expect("field names are strings");
    let mut object = Map::new();
    object.insert("type".to_owned(), "object".into());
    object.insert("properties".to_owned(), Value::Object(properties));
    object.insert("required".to_owned(), Value::Array(required));
    object.insert("additionalProperties".to_owned(), Value::Bool(false));
    Value::Object(object)
}

fn write_enum(variants: &[SchemaVariant]) -> Value {
    if variants
        .iter()
        .all(|variant| matches!(variant.shape, SchemaVariantShape::Unit))
    {
        let mut names = variants
            .iter()
            .map(|variant| Value::String(variant.name.clone()))
            .collect::<Vec<_>>();
        if names.len() == 1 {
            return unit_constant(names.remove(0));
        }
        sort_by_canonical_text(&mut names).expect("variant names are strings");
        let mut object = Map::new();
        object.insert("type".to_owned(), "string".into());
        object.insert("enum".to_owned(), Value::Array(names));
        return Value::Object(object);
    }
    let mut entries = variants
        .iter()
        .map(|variant| match &variant.shape {
            SchemaVariantShape::Unit => unit_constant(Value::String(variant.name.clone())),
            SchemaVariantShape::Newtype { schema } => {
                variant_wrapper(&variant.name, write_node(schema))
            }
            SchemaVariantShape::Tuple { items } => {
                variant_wrapper(&variant.name, write_tuple(items))
            }
            SchemaVariantShape::Record { fields } => {
                variant_wrapper(&variant.name, write_record(fields))
            }
        })
        .collect::<Vec<_>>();
    sort_by_canonical_text(&mut entries).expect("written entries are canonical");
    let mut object = Map::new();
    object.insert("oneOf".to_owned(), Value::Array(entries));
    Value::Object(object)
}

fn variant_wrapper(name: &str, inner: Value) -> Value {
    let mut properties = Map::new();
    properties.insert(name.to_owned(), inner);
    let mut object = Map::new();
    object.insert("type".to_owned(), "object".into());
    object.insert("properties".to_owned(), Value::Object(properties));
    object.insert(
        "required".to_owned(),
        Value::Array(vec![Value::String(name.to_owned())]),
    );
    object.insert("additionalProperties".to_owned(), Value::Bool(false));
    Value::Object(object)
}

// ---------------------------------------------------------------------------
// Reader
