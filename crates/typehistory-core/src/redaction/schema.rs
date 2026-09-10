//! Transparent wrapper schemas belong beside the locally owned schema traits.

use crate::{JsonSchemaField, ResolvedSchema};
use redactable::{BypassDebugRedaction, BypassDisplayRedaction, BypassRedaction, SensitiveValue};
use schemars::{Schema, SchemaGenerator};

impl<T: ResolvedSchema, P> ResolvedSchema for SensitiveValue<T, P> {
    type Wire = T::Wire;
}
impl<T: JsonSchemaField, P> JsonSchemaField for SensitiveValue<T, P> {
    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        T::json_schema(generator)
    }
}
macro_rules! transparent_schema {
    ($($wrapper:ident),+ $(,)?) => { $(
        impl<T: ResolvedSchema> ResolvedSchema for $wrapper<T> { type Wire = T::Wire; }
        impl<T: JsonSchemaField> JsonSchemaField for $wrapper<T> {
            fn json_schema(generator: &mut SchemaGenerator) -> Schema { T::json_schema(generator) }
        }
    )+ };
}
transparent_schema!(
    BypassRedaction,
    BypassDebugRedaction,
    BypassDisplayRedaction
);
