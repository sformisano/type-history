//! Concrete externally tagged enum helpers.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{DataEnum, Error, Fields, Ident, Result};
use type_history_codegen::{
    json_schema_derive::field_attribute,
    lint_attributes::{lint_attributes, scoped_field_type},
    resolved_schema::enum_wire_type,
};

use super::validate_attribute;

pub(super) fn expand(
    data: &DataEnum,
    name: &Ident,
    helper: &Ident,
    support: &TokenStream,
) -> Result<(TokenStream, TokenStream)> {
    if data.variants.is_empty() {
        return Err(Error::new_spanned(
            name,
            "Schema requires at least one enum variant",
        ));
    }
    let mut variants = data.variants.clone();
    let mut aliases = Vec::new();
    let mut declarations = Vec::new();
    let mut index = 0;
    for variant in &mut variants {
        for attribute in &variant.attrs {
            validate_attribute(attribute, false)?;
        }
        if matches!(&variant.fields, Fields::Unnamed(fields) if fields.unnamed.is_empty()) {
            return Err(Error::new_spanned(
                &variant.fields,
                "Schema tuple variants require at least one field",
            ));
        }
        let variant_name = &variant.ident;
        let variant_lints = lint_attributes(&variant.attrs).collect::<Vec<_>>();
        let mut fields = Vec::new();
        for field in &mut variant.fields {
            for attribute in &field.attrs {
                validate_attribute(attribute, false)?;
            }
            let mut attributes = variant_lints
                .iter()
                .map(|attribute| (*attribute).clone())
                .collect::<Vec<_>>();
            attributes.extend(field.attrs.clone());
            let (ty, alias) = scoped_field_type(name, index, &field.ty, &attributes);
            index += 1;
            aliases.push(alias);
            field.ty = ty;
            let ty = &field.ty;
            let schema = field_attribute(support, ty);
            let lints = lint_attributes(&field.attrs);
            let declaration = match &field.ident {
                Some(name) => quote!(#name: #ty),
                None => quote!(#ty),
            };
            fields.push(quote!(#(#lints)* #schema #declaration));
        }
        let payload = match &variant.fields {
            Fields::Unit => quote!(),
            Fields::Unnamed(_) => quote!((#(#fields),*)),
            Fields::Named(_) => quote!({#(#fields),*}),
        };
        declarations.push(quote!(#(#variant_lints)* #variant_name #payload));
    }
    let wire = enum_wire_type(&variants, support)?;
    let derive = type_history_codegen::json_schema_derive::container_attributes(support);
    Ok((
        wire,
        quote! {
            #(#aliases)*
            #[allow(dead_code)]
            #derive
            enum #helper { #(#declarations),* }
        },
    ))
}
