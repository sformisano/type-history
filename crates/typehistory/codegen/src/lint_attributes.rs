//! Preserve authored lint scope across generated copies of one declaration.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{Attribute, File, Ident, Result, Type};

/// The supported lint attributes, in their authored order.
pub fn lint_attributes(attributes: &[Attribute]) -> impl Iterator<Item = &Attribute> {
    attributes.iter().filter(|attribute| {
        ["allow", "warn", "deny"]
            .iter()
            .any(|name| attribute.path().is_ident(name))
    })
}

/// Reuse a field type without moving its lint allowance onto neighboring fields.
pub fn scoped_field_type(
    record: &Ident,
    index: usize,
    ty: &Type,
    attributes: &[Attribute],
) -> (Type, TokenStream) {
    let lints = lint_attributes(attributes).collect::<Vec<_>>();
    if lints.is_empty() {
        return (ty.clone(), TokenStream::new());
    }
    let name = format_ident!("__Typehistory{}Field{}Type", record, index);
    (
        syn::parse_quote!(#name),
        quote!(#(#lints)* type #name = #ty;),
    )
}

/// Apply a declaration's lint scope to every emitted item that replaces it.
pub fn with_lints(attributes: &[Attribute], tokens: TokenStream) -> Result<TokenStream> {
    let lints = lint_attributes(attributes).collect::<Vec<_>>();
    if lints.is_empty() {
        return Ok(tokens);
    }
    let file: File = syn::parse2(tokens)?;
    Ok(file
        .items
        .iter()
        .map(|item| quote!(#(#lints)* #item))
        .collect())
}
