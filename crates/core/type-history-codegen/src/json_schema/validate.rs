//! Checks that must run before writing a shape into JSON objects.

use std::collections::BTreeSet;

use type_history_core::resolved::{SchemaField, SchemaShape, SchemaVariantShape};

use super::{JsonSchemaError, JsonSchemaErrorReason};

pub(super) fn shape(value: &SchemaShape, path: &str) -> Result<(), JsonSchemaError> {
    match value {
        SchemaShape::Option { value } => shape(value, &format!("{path}/anyOf/0")),
        SchemaShape::Sequence { value }
        | SchemaShape::Array { value, .. }
        | SchemaShape::Set { value, .. } => shape(value, &format!("{path}/items")),
        SchemaShape::Map { value } => shape(value, &format!("{path}/additionalProperties")),
        SchemaShape::Tuple { items } => tuple(items, path),
        SchemaShape::Record { fields } => record(fields, path),
        SchemaShape::Enum { variants } => {
            let mut names = BTreeSet::new();
            for (index, variant) in variants.iter().enumerate() {
                if !names.insert(&variant.name) {
                    return Err(JsonSchemaError::at(path, JsonSchemaErrorReason::Enum));
                }
                let path = format!("{path}/oneOf/{index}/properties/{}", escape(&variant.name));
                match &variant.shape {
                    SchemaVariantShape::Unit => (),
                    SchemaVariantShape::Newtype { schema } => shape(schema, &path)?,
                    SchemaVariantShape::Tuple { items } => tuple(items, &path)?,
                    SchemaVariantShape::Record { fields } => record(fields, &path)?,
                }
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn tuple(items: &[SchemaShape], path: &str) -> Result<(), JsonSchemaError> {
    if items.is_empty() {
        return Err(JsonSchemaError::at(path, JsonSchemaErrorReason::Tuple));
    }
    for (index, item) in items.iter().enumerate() {
        shape(item, &format!("{path}/prefixItems/{index}"))?;
    }
    Ok(())
}

fn record(fields: &[SchemaField], path: &str) -> Result<(), JsonSchemaError> {
    let mut names = BTreeSet::new();
    for field in fields {
        let field_path = format!("{path}/properties/{}", escape(&field.name));
        if !names.insert(&field.name) {
            return Err(JsonSchemaError::at(
                &field_path,
                JsonSchemaErrorReason::DuplicateField,
            ));
        }
        shape(&field.schema, &field_path)?;
    }
    Ok(())
}

fn escape(name: &str) -> String {
    name.replace('~', "~0").replace('/', "~1")
}

#[cfg(test)]
mod tests;
