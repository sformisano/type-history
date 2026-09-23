//! Retained records, their current alias, and schema expressions.
//!
//! Native mode derives Clone and the selected PartialEq and Debug traits.

use crate::{
    lint_attributes::{lint_attributes, scoped_field_type_for_owner},
    model::NamedField,
};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{parse_quote, DeriveInput, Result};

use super::{Context, GeneratedVersion, GenerationOptions, PayloadTraits};

pub(super) fn generate(
    context: &Context<'_>,
    options: &GenerationOptions,
) -> Result<(TokenStream, Vec<GeneratedVersion>)> {
    let support_path = &context.paths.support;
    let support = quote!(#support_path);
    let mut declarations = Vec::new();
    let mut versions = Vec::new();
    let visibility = &context.input.visibility;
    let mut derives = vec![quote!(::core::clone::Clone)];
    if context.input.derives.partial_eq {
        derives.push(quote!(::core::cmp::PartialEq));
    }
    if context.input.derives.debug {
        derives.push(quote!(::core::fmt::Debug));
    }
    let derives = match options.payload_traits {
        PayloadTraits::Native => quote!(#[derive(#(#derives),*)]),
        PayloadTraits::Frontend => TokenStream::new(),
    };
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
            quote!(#(#docs)* #(#lints)* #field_visibility #name: #ty)
        });
        let codecs = crate::record::record(&name, &fields, &support);
        let schema_docs = if version.version == context.history.head() {
            context.input.attributes.as_slice()
        } else {
            &[]
        };
        let schema_impl = crate::json_schema_derive::record_with_docs(
            &name,
            &fields,
            &support,
            schema_docs,
            true,
        );
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
            #derives
            #visibility struct #name { #(#field_declarations),* }
            #schema_impl
            #codecs
            #[doc(hidden)]
            #visibility struct #wire_marker;
            impl #support_path::WireNode for #wire_marker {
                const SHAPE: #support_path::ConstantShape = <#wire_type as #support_path::WireNode>::SHAPE;
            }
            impl #support_path::NonOptionalNode for #wire_marker {}
            impl #support_path::ResolvedSchema for #name { type Wire = #wire_marker; }
            #field_contract
        });
        let contract_fields = fields.iter().map(|field| {
            let name = &field.name;
            let ty = &field.ty;
            let attributes = &field.attributes;
            let key = name.to_string().trim_start_matches("r#").to_owned();
            let visibility = &context.input.field_visibility[&key];
            quote!(#(#attributes)* #visibility #name: #ty)
        });
        let contract: DeriveInput = parse_quote! {
            #visibility struct #name { #(#contract_fields),* }
        };
        versions.push(GeneratedVersion {
            number: version.version,
            schema_expression: quote!(#support_path::export_json_schema::<#name>()),
            shape_expression: quote!(<#name as #support_path::ResolvedSchema>::resolved_wire_schema()),
            contract,
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
