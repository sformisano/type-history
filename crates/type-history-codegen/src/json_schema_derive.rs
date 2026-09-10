//! `schemars` derive emission for generated persisted records.
//!
//! Every generated state, payload, and named value record derives
//! `schemars::JsonSchema` with each field routed through
//! `FieldSchema<FieldType>`, so the exported document follows the wire
//! type that `ResolvedSchema` selects rather than the field's own
//! `JsonSchema` implementation. Authors never write these attributes.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{Ident, Type};

fn crate_path(support: &TokenStream) -> String {
    format!("{support}::schemars").replace(' ', "")
}

/// Container attributes: the derive, its crate path, and `deny_unknown_fields`.
pub fn container_attributes(support: &TokenStream) -> TokenStream {
    let crate_path = crate_path(support);
    quote! {
        #[derive(#support::schemars::JsonSchema)]
        #[schemars(crate = #crate_path, deny_unknown_fields)]
    }
}

/// Field attribute routing one authored field type through `FieldSchema`.
pub fn field_attribute(support: &TokenStream, ty: &Type) -> TokenStream {
    // Keep token boundaries: removing spaces turns `<T as Trait>::Item`
    // into a different type (`<TasTrait>::Item`).
    let with = quote!(#support::FieldSchema<#ty>).to_string();
    quote!(#[schemars(with = #with)])
}

/// `JsonSchemaField` implementation delegating to another field type's form.
pub fn field_impl_delegating_to(name: &Ident, inner: &Type, support: &TokenStream) -> TokenStream {
    quote! {
        impl #support::JsonSchemaField for #name {
            fn json_schema(
                generator: &mut #support::schemars::SchemaGenerator,
            ) -> #support::schemars::Schema {
                <#inner as #support::JsonSchemaField>::json_schema(generator)
            }
        }
    }
}

/// `JsonSchemaField` implementation for a record that derives `JsonSchema`.
pub fn field_impl_for_derived(name: &Ident, support: &TokenStream) -> TokenStream {
    quote! {
        impl #support::JsonSchemaField for #name {
            fn json_schema(
                generator: &mut #support::schemars::SchemaGenerator,
            ) -> #support::schemars::Schema {
                <#name as #support::schemars::JsonSchema>::json_schema(generator)
            }
        }
    }
}
