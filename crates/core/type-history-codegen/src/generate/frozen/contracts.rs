//! Token generation for every frozen storage-contract descriptor.

use proc_macro2::TokenStream;
use quote::quote;
use type_history_core::resolved::{
    FieldPresence, Membership, SchemaField, SchemaShape, SchemaVariantShape, StorageProfile,
};

pub(super) fn presence_tokens(presence: FieldPresence, resolved: &TokenStream) -> TokenStream {
    match presence {
        FieldPresence::Required => quote!(#resolved::FieldPresence::Required),
        FieldPresence::Optional => quote!(#resolved::FieldPresence::Optional),
    }
}

pub(super) fn shape_tokens(shape: &SchemaShape, resolved: &TokenStream) -> TokenStream {
    let node = quote!(#resolved::ConstantShape);
    match shape {
        SchemaShape::Bool => quote!(#node::Bool),
        SchemaShape::I8 => quote!(#node::I8),
        SchemaShape::I16 => quote!(#node::I16),
        SchemaShape::I32 => quote!(#node::I32),
        SchemaShape::I64 => quote!(#node::I64),
        SchemaShape::I128 => quote!(#node::I128),
        SchemaShape::U8 => quote!(#node::U8),
        SchemaShape::U16 => quote!(#node::U16),
        SchemaShape::U32 => quote!(#node::U32),
        SchemaShape::U64 => quote!(#node::U64),
        SchemaShape::U128 => quote!(#node::U128),
        SchemaShape::String => quote!(#node::String),
        SchemaShape::Bytes => quote!(#node::Bytes),
        SchemaShape::Option { value } => {
            let value = shape_tokens(value, resolved);
            quote!(#node::Option(&#value))
        }
        SchemaShape::Sequence { value } => {
            let value = shape_tokens(value, resolved);
            quote!(#node::Sequence(&#value))
        }
        SchemaShape::Map { value } => {
            let value = shape_tokens(value, resolved);
            quote!(#node::Map(&#value))
        }
        SchemaShape::Array { value, length } => {
            let value = shape_tokens(value, resolved);
            quote!(#node::Array(&#value, #length))
        }
        SchemaShape::Set { value, membership } => {
            let value = shape_tokens(value, resolved);
            let membership = membership_tokens(membership, resolved);
            quote!(#node::Set(&#value, #membership))
        }
        SchemaShape::Tuple { items } => {
            let items = items_tokens(items, resolved);
            quote!(#node::Tuple(#items))
        }
        SchemaShape::Profile { profile } => {
            let profile = profile_tokens(*profile, resolved);
            quote!(#node::Profile(#profile))
        }
        SchemaShape::Record { fields } => {
            let fields = fields_tokens(fields, resolved);
            quote!(#node::Record(#fields))
        }
        SchemaShape::Enum { variants } => {
            let variants = variants.iter().rev().fold(
                quote!(#resolved::ConstantVariants::End),
                |tail, variant| {
                    let name = name_tokens(&variant.name, resolved);
                    let shape = match &variant.shape {
                        SchemaVariantShape::Unit => quote!(#resolved::ConstantVariant::Unit),
                        SchemaVariantShape::Newtype { schema } => {
                            let shape = shape_tokens(schema, resolved);
                            quote!(#resolved::ConstantVariant::Newtype(&#shape))
                        }
                        SchemaVariantShape::Tuple { items } => {
                            let items = items_tokens(items, resolved);
                            quote!(#resolved::ConstantVariant::Tuple(#items))
                        }
                        SchemaVariantShape::Record { fields } => {
                            let fields = fields_tokens(fields, resolved);
                            quote!(#resolved::ConstantVariant::Record(#fields))
                        }
                    };
                    quote!(#resolved::ConstantVariants::Variant(#name, #shape, &#tail))
                },
            );
            quote!(#node::Enum(#variants))
        }
    }
}

fn fields_tokens(fields: &[SchemaField], resolved: &TokenStream) -> TokenStream {
    fields
        .iter()
        .rev()
        .fold(quote!(#resolved::ConstantFields::End), |tail, field| {
            let name = name_tokens(&field.name, resolved);
            let presence = presence_tokens(field.presence, resolved);
            let shape = shape_tokens(&field.schema, resolved);
            quote!(#resolved::ConstantFields::Field(#name, #presence, &#shape, &#tail))
        })
}

fn items_tokens(items: &[SchemaShape], resolved: &TokenStream) -> TokenStream {
    items
        .iter()
        .rev()
        .fold(quote!(#resolved::ConstantItems::End), |tail, item| {
            let shape = shape_tokens(item, resolved);
            quote!(#resolved::ConstantItems::Item(&#shape, &#tail))
        })
}

fn membership_tokens(membership: &Membership, resolved: &TokenStream) -> TokenStream {
    let id = &membership.id;
    let parameters = membership
        .parameters
        .iter()
        .map(|parameter| membership_tokens(parameter, resolved));
    quote!(#resolved::ConstantMembership { id: #id, parameters: &[#(#parameters),*] })
}

fn profile_tokens(profile: StorageProfile, resolved: &TokenStream) -> TokenStream {
    match profile {
        StorageProfile::Finite32 => quote!(#resolved::StorageProfile::Finite32),
        StorageProfile::Finite64 => quote!(#resolved::StorageProfile::Finite64),
        StorageProfile::UuidText => quote!(#resolved::StorageProfile::UuidText),
        StorageProfile::DecimalText => quote!(#resolved::StorageProfile::DecimalText),
        StorageProfile::Date => quote!(#resolved::StorageProfile::Date),
        StorageProfile::LocalTime => quote!(#resolved::StorageProfile::LocalTime),
        StorageProfile::LocalDateTime => quote!(#resolved::StorageProfile::LocalDateTime),
        StorageProfile::UtcInstant => quote!(#resolved::StorageProfile::UtcInstant),
        StorageProfile::OffsetDateTime => quote!(#resolved::StorageProfile::OffsetDateTime),
    }
}

fn name_tokens(name: &str, resolved: &TokenStream) -> TokenStream {
    name.as_bytes().iter().rev().fold(
        quote!(#resolved::ConstantName::End),
        |tail, byte| quote!(#resolved::ConstantName::Byte(#byte, &#tail)),
    )
}

#[cfg(test)]
mod tests {
    use quote::quote;
    use type_history_core::resolved::{
        FieldPresence, Membership, SchemaField, SchemaShape, StorageProfile,
    };

    use super::{presence_tokens, shape_tokens};

    #[test]
    fn emits_every_new_frozen_contract_component() {
        let resolved = quote!(support);
        let shape = SchemaShape::Record {
            fields: vec![SchemaField {
                name: "value".to_owned(),
                presence: FieldPresence::Optional,
                schema: SchemaShape::Map {
                    value: Box::new(SchemaShape::Set {
                        value: Box::new(SchemaShape::Tuple {
                            items: vec![SchemaShape::Profile {
                                profile: StorageProfile::DecimalText,
                            }],
                        }),
                        membership: Membership {
                            id: "example:membership:v1".to_owned(),
                            parameters: vec![Membership {
                                id: "example:parameter:v1".to_owned(),
                                parameters: vec![],
                            }],
                        },
                    }),
                },
            }],
        };
        let rendered = shape_tokens(&shape, &resolved).to_string();
        for expected in [
            "ConstantFields :: Field",
            "FieldPresence :: Optional",
            "ConstantShape :: Map",
            "ConstantShape :: Set",
            "ConstantShape :: Tuple",
            "StorageProfile :: DecimalText",
            "example:membership:v1",
            "example:parameter:v1",
        ] {
            assert!(
                rendered.contains(expected),
                "missing `{expected}` in {rendered}"
            );
        }
    }

    #[test]
    fn emits_both_presence_tokens() {
        let resolved = quote!(support);
        assert!(presence_tokens(FieldPresence::Required, &resolved)
            .to_string()
            .contains("Required"));
        assert!(presence_tokens(FieldPresence::Optional, &resolved)
            .to_string()
            .contains("Optional"));
    }
}
