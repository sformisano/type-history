//! JSON Schema adapters for resolved field wire types.

mod presence;
pub use presence::apply_named_presence;

use schemars::{generate::SchemaSettings, JsonSchema, Schema, SchemaGenerator};
use serde_json::Value;
use std::{borrow::Cow, marker::PhantomData};

/// Routes a persisted field type to the `schemars` form of its wire type.
///
/// Generated payload and named value records derive
/// `JsonSchema` with every field routed through this type, so the
/// exported document follows [`ResolvedSchema`](super::ResolvedSchema) rather than the field's
/// own `JsonSchema` implementation. The indirection also lets generated histories
/// describe foreign wrapper types through their resolved inner wire type.
pub struct FieldSchema<T: ?Sized>(PhantomData<T>);

/// The `schemars` form of one persisted field type.
#[diagnostic::on_unimplemented(
    message = "`{Self}` is not supported as a persisted field",
    label = "this type has no supported JSON Schema field adapter",
    note = "For a supported named record or enum, derive `type_history::Schema` or use your framework's schema declaration.",
    note = "For other types, choose a supported serialized representation; wrapping an unsupported field alone is insufficient."
)]
pub trait JsonSchemaField {
    /// Generate the JSON Schema for the wire type of `Self`.
    fn json_schema(generator: &mut SchemaGenerator) -> Schema;
}

impl<T: JsonSchemaField> JsonSchema for FieldSchema<T> {
    fn inline_schema() -> bool {
        true
    }

    fn schema_name() -> Cow<'static, str> {
        "TypeHistoryFieldSchema".into()
    }

    fn schema_id() -> Cow<'static, str> {
        format!(
            "type_history::TypeHistoryFieldSchema<{}>",
            std::any::type_name::<T>()
        )
        .into()
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        T::json_schema(generator)
    }
}

macro_rules! primitive_field_schema {
    ($($rust:ty),+ $(,)?) => {
        $(impl JsonSchemaField for $rust {
            fn json_schema(generator: &mut SchemaGenerator) -> Schema {
                <$rust as JsonSchema>::json_schema(generator)
            }
        })+
    };
}

primitive_field_schema!(bool, i8, i16, i32, i64, i128, u8, u16, u32, u64, u128, String);

impl<T: JsonSchemaField> JsonSchemaField for Option<T> {
    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        <Option<FieldSchema<T>> as JsonSchema>::json_schema(generator)
    }
}

impl<T: JsonSchemaField> JsonSchemaField for Vec<T> {
    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        <Vec<FieldSchema<T>> as JsonSchema>::json_schema(generator)
    }
}

impl<T: JsonSchemaField, const LENGTH: usize> JsonSchemaField for [T; LENGTH] {
    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        let items = generator.subschema_for::<FieldSchema<T>>();
        schemars::json_schema!({
            "type": "array",
            "items": items,
            "minItems": LENGTH,
            "maxItems": LENGTH,
        })
    }
}

/// Generate the raw JSON Schema 2020-12 document that the schema export prints.
///
/// The planner normalizes and fingerprints this value; nothing else reads it.
pub fn export_json_schema<T: JsonSchema>() -> Value {
    SchemaSettings::draft2020_12()
        .with(|settings| settings.inline_subschemas = true)
        .into_generator()
        .into_root_schema_for::<T>()
        .to_value()
}
