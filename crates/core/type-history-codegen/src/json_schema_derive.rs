//! `schemars` derive emission for generated persisted records.
//!
//! Private generic helpers derive `schemars::JsonSchema` with each field routed through
//! `FieldSchema<FieldType>`, so the exported document follows the wire
//! type that `ResolvedSchema` selects rather than the field's own
//! `JsonSchema` implementation. Authors never write these attributes.

use crate::{identifier::helper_name, model::NamedField};
use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};
use syn::{Attribute, Ident, Type};

/// Generic field parameter used only inside a third-party derive helper.
pub fn parameter(index: usize) -> Ident {
    format_ident!("__TypeHistoryField{index}", span = Span::mixed_site())
}

/// Derive in a scope containing only generic fields and a dedicated support alias.
/// Actual authored types are instantiated in their original lexical scope.
pub fn isolated(
    name: &Ident,
    helper: &Ident,
    types: &[Type],
    body: &TokenStream,
    support: &TokenStream,
) -> TokenStream {
    let module = helper_name("schema", name);
    let namespace = helper_name("schema_support", name);
    let params = (0..types.len()).map(parameter).collect::<Vec<_>>();
    let bound = params
        .iter()
        .map(|param| quote!(#param: __support::JsonSchemaField).to_string())
        .collect::<Vec<_>>()
        .join(",");
    let title = name.to_string();
    let generator = format_ident!("__type_history_generator", span = Span::mixed_site());
    quote! {
        use #support as #namespace;
        #[allow(dead_code)]
        mod #module {
            use super::#namespace as __support;
            // Nested derive expansion may otherwise fall back to the outer
            // derive invocation's primitive aliases (rustc issue 83583).
            use ::core::primitive::{bool, str};
            fn __type_history_field_schema<T: __support::JsonSchemaField>(generator: &mut __support::schemars::SchemaGenerator) -> __support::schemars::Schema {
                <__support::FieldSchema<T> as __support::schemars::JsonSchema>::json_schema(generator)
            }
            #[derive(__support::schemars::JsonSchema)]
            #[schemars(crate = "__support::schemars", deny_unknown_fields, rename = #title, bound = #bound)]
            #body
            // Check authored types at the call site, without exposing the private
            // derive helper's trait bounds as advice to implement JsonSchema.
            pub(super) fn __type_history_record_schema<#(#params: __support::JsonSchemaField),*>(generator: &mut __support::schemars::SchemaGenerator, shape: &__support::ConstantShape) -> __support::schemars::Schema {
                let mut schema = <#helper<#(#params),*> as __support::schemars::JsonSchema>::json_schema(generator);
                __support::apply_named_presence(&mut schema, shape);
                schema
            }
        }
        impl #support::schemars::JsonSchema for #name {
            fn schema_name() -> ::std::borrow::Cow<'static, ::core::primitive::str> { ::std::borrow::Cow::Borrowed(#title) }
            fn schema_id() -> ::std::borrow::Cow<'static, ::core::primitive::str> {
                ::std::borrow::Cow::Borrowed(::core::concat!(::core::module_path!(), "::", #title))
            }
            fn json_schema(#generator: &mut #support::schemars::SchemaGenerator) -> #support::schemars::Schema {
                #module::__type_history_record_schema::<#(#types),*>(
                    #generator,
                    &<<#name as #support::ResolvedSchema>::Wire as #support::WireNode>::SHAPE,
                )
            }
        }
    }
}

/// Isolated schema implementation for a named record.
pub fn record(name: &Ident, fields: &[NamedField], support: &TokenStream) -> TokenStream {
    record_with_docs(name, fields, support, &[], false)
}

/// Isolated schema implementation for a supporting tuple struct.
pub fn tuple(name: &Ident, helper: &Ident, types: &[Type], support: &TokenStream) -> TokenStream {
    let params = (0..types.len()).map(parameter).collect::<Vec<_>>();
    let fields = params.iter().map(|param| {
        let schema = isolated_field_attribute(param);
        quote!(#schema #param)
    });
    isolated(
        name,
        helper,
        types,
        &quote!(pub(super) struct #helper<#(#params),*>(#(#fields),*);),
        support,
    )
}

/// Isolated payload schema retaining its public container and field descriptions.
pub fn record_with_docs(
    name: &Ident,
    fields: &[NamedField],
    support: &TokenStream,
    attributes: &[Attribute],
    field_docs: bool,
) -> TokenStream {
    let helper = format_ident!("__TypeHistory{}Schema", name);
    let types = fields
        .iter()
        .map(|field| field.ty.clone())
        .collect::<Vec<_>>();
    let params = (0..fields.len()).map(parameter).collect::<Vec<_>>();
    let declarations = fields.iter().zip(&params).map(|(field, param)| {
        let name = &field.name;
        let docs = field
            .attributes
            .iter()
            .filter(|attribute| field_docs && attribute.path().is_ident("doc"));
        let schema = isolated_field_attribute(param);
        quote!(#(#docs)* #schema #name: #param)
    });
    let docs = attributes
        .iter()
        .filter(|attribute| attribute.path().is_ident("doc"));
    isolated(
        name,
        &helper,
        &types,
        &quote!(#(#docs)* pub(super) struct #helper<#(#params),*> { #(#declarations),* }),
        support,
    )
}

fn crate_path(support: &TokenStream) -> String {
    format!("{support}::schemars").replace(' ', "")
}

/// Route a generic helper field without making its private schema identity generic.
pub fn isolated_field_attribute(parameter: &Ident) -> TokenStream {
    let function = quote!(__type_history_field_schema::<#parameter>).to_string();
    quote!(#[schemars(schema_with = #function)])
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
    let generator = format_ident!("__type_history_generator", span = Span::mixed_site());
    quote! {
        impl #support::JsonSchemaField for #name {
            fn json_schema(
                #generator: &mut #support::schemars::SchemaGenerator,
            ) -> #support::schemars::Schema {
                <#inner as #support::JsonSchemaField>::json_schema(#generator)
            }
        }
    }
}

/// `JsonSchemaField` implementation for a record that derives `JsonSchema`.
pub fn field_impl_for_derived(name: &Ident, support: &TokenStream) -> TokenStream {
    let generator = format_ident!("__type_history_generator", span = Span::mixed_site());
    quote! {
        impl #support::JsonSchemaField for #name {
            fn json_schema(
                #generator: &mut #support::schemars::SchemaGenerator,
            ) -> #support::schemars::Schema {
                <#name as #support::schemars::JsonSchema>::json_schema(#generator)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{field_impl_delegating_to, tuple};
    use quote::{format_ident, quote};
    use syn::{parse_quote, Ident, Type};

    #[test]
    fn tuple_schema_routes_each_position_through_its_field_contract() {
        let name: Ident = parse_quote!(Pair);
        let helper = format_ident!("__TypeHistoryPairSchema");
        let types = vec![parse_quote!(u32), parse_quote!(String)];
        let tokens = tuple(&name, &helper, &types, &quote!(support)).to_string();

        assert!(tokens.contains("struct __TypeHistoryPairSchema"));
        assert!(tokens.contains("__type_history_field_schema :: < __TypeHistoryField0 >"));
        assert!(tokens.contains("__type_history_field_schema :: < __TypeHistoryField1 >"));
        assert!(tokens.contains("apply_named_presence"));
    }

    #[test]
    fn newtype_field_contract_delegates_to_the_inner_value() {
        let name: Ident = parse_quote!(Identifier);
        let inner: Type = parse_quote!(String);
        let tokens = field_impl_delegating_to(&name, &inner, &quote!(support)).to_string();

        assert!(tokens.contains("impl support :: JsonSchemaField for Identifier"));
        assert!(tokens.contains("< String as support :: JsonSchemaField > :: json_schema"));
    }
}
