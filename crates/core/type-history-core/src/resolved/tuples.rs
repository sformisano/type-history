//! Ordered tuple field contracts for arities one through sixteen.

use super::{
    membership::rules, ConstantMembership, End, FieldSchema, Item, JsonSchemaField, ResolvedSchema,
    SetMembership, Tuple,
};
use schemars::{Schema, SchemaGenerator};

macro_rules! wire_items {
    () => { End };
    ($head:ident $(, $tail:ident)*) => {
        Item<<$head as ResolvedSchema>::Wire, wire_items!($($tail),*)>
    };
}

macro_rules! tuple {
    ($($value:ident),+ $(,)?) => {
        impl<$($value: ResolvedSchema),+> ResolvedSchema for ($($value,)+) {
            type Wire = Tuple<wire_items!($($value),+)>;
        }
        impl<$($value: JsonSchemaField),+> JsonSchemaField for ($($value,)+) {
            fn json_schema(generator: &mut SchemaGenerator) -> Schema {
                let items = vec![$(generator.subschema_for::<FieldSchema<$value>>()),+];
                let length = items.len();
                schemars::json_schema!({
                    "type": "array", "prefixItems": items,
                    "minItems": length, "maxItems": length,
                })
            }
        }
        // Rust's tuple Eq implementation is arity-limited. Do not certify a
        // tuple as a set member when its actual standard equality is absent.
        impl<$($value: SetMembership),+> SetMembership for ($($value,)+)
        where ($($value,)+): Eq
        {
            const MEMBERSHIP: ConstantMembership = ConstantMembership {
                id: rules::TUPLE.id, parameters: &[$($value::MEMBERSHIP),+],
            };
        }
    };
}

tuple!(A);
tuple!(A, B);
tuple!(A, B, C);
tuple!(A, B, C, D);
tuple!(A, B, C, D, E);
tuple!(A, B, C, D, E, F);
tuple!(A, B, C, D, E, F, G);
tuple!(A, B, C, D, E, F, G, H);
tuple!(A, B, C, D, E, F, G, H, I);
tuple!(A, B, C, D, E, F, G, H, I, J);
tuple!(A, B, C, D, E, F, G, H, I, J, K);
tuple!(A, B, C, D, E, F, G, H, I, J, K, L);
tuple!(A, B, C, D, E, F, G, H, I, J, K, L, M);
tuple!(A, B, C, D, E, F, G, H, I, J, K, L, M, N);
tuple!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O);
tuple!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P);

#[cfg(test)]
mod tests;
