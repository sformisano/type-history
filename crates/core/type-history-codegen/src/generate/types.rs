//! Retained records, their current alias, and schema expressions.
//!
//! Records derive Clone, PartialEq, and field-wise Debug.

use crate::{
    lint_attributes::{lint_attributes, scoped_field_type_for_owner},
    model::NamedField,
};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::Result;

use super::{Context, GeneratedVersion};

pub(super) fn generate(context: &Context<'_>) -> Result<(TokenStream, Vec<GeneratedVersion>)> {
    let support_path = &context.paths.support;
    let support = quote!(#support_path);
    let schema_derive = crate::json_schema_derive::container_attributes(&support);
    let mut declarations = Vec::new();
    let mut versions = Vec::new();
    let visibility = &context.input.visibility;
    for version in context.history.versions() {
        let name = context.payload(version.version);
        let mut field_aliases = Vec::new();
        let fields = version
            .fields
            .iter()
            .enumerate()
            .map(|(index, field)| {
                let (ty, declaration) = scoped_field_type_for_owner(
                    &name,
                    &context.input.name,
                    index,
                    &field.ty,
                    &field.attributes,
                );
                field_aliases.push(declaration);
                NamedField {
                    name: field.name.clone(),
                    ty,
                    attributes: field.attributes.clone(),
                }
            })
            .collect::<Vec<_>>();
        let field_declarations = fields.iter().map(|field| {
            let name = &field.name;
            let ty = &field.ty;
            let key = name.to_string().trim_start_matches("r#").to_owned();
            let field_visibility = &context.input.field_visibility[&key];
            let docs = field
                .attributes
                .iter()
                .filter(|attribute| attribute.path().is_ident("doc"));
            let lints = lint_attributes(&field.attributes);
            let schema = crate::json_schema_derive::field_attribute(&support, ty);
            quote!(#(#docs)* #(#lints)* #schema #field_visibility #name: #ty)
        });
        let codecs = crate::record::record(&name, &fields, &support);
        let wire_type = crate::resolved_schema::record_wire_type(
            fields.iter().map(|field| (&field.name, &field.ty)),
            &support,
        )?;
        let field_contract = crate::json_schema_derive::field_impl_for_derived(&name, &support);
        let wire_marker = format_ident!("__TypeHistory{}Wire", name);
        let docs = if version.version == context.history.head() {
            let attributes = context
                .input
                .attributes
                .iter()
                .filter(|attribute| attribute.path().is_ident("doc"));
            quote!(#(#attributes)*)
        } else {
            quote!(#[doc(hidden)])
        };
        declarations.push(quote! {
            #(#field_aliases)*
            #docs
            #[derive(::core::clone::Clone, ::core::cmp::PartialEq, ::core::fmt::Debug)]
            #schema_derive
            #visibility struct #name { #(#field_declarations),* }
            #codecs
            #[doc(hidden)]
            #visibility struct #wire_marker;
            impl #support_path::WireNode for #wire_marker {
                const SHAPE: #support_path::ConstantShape = <#wire_type as #support_path::WireNode>::SHAPE;
                fn schema() -> #support_path::SchemaShape { <#wire_type as #support_path::WireNode>::schema() }
            }
            impl #support_path::NonOptionalNode for #wire_marker {}
            impl #support_path::ResolvedSchema for #name { type Wire = #wire_marker; }
            #field_contract
        });
        versions.push(GeneratedVersion {
            number: version.version,
            schema_expression: quote!(#support_path::export_json_schema::<#name>()),
            name,
        });
    }
    let current = context.payload(context.history.head());
    let alias = &context.input.name;
    let attributes = &context.input.attributes;
    Ok((
        quote! {
            #(#declarations)*
            #(#attributes)*
            #visibility type #alias = #current;
        },
        versions,
    ))
}
