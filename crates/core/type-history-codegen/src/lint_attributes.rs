//! Preserve authored lint scope across generated copies of one declaration.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::spanned::Spanned;
use syn::visit_mut::{self, VisitMut};
use syn::{Attribute, File, Ident, Path, Result, Type};

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
    scoped_field_type_for_owner(record, record, index, ty, attributes)
}

// Historical helper names remain distinct while `Self` keeps its authored owner.
pub(crate) fn scoped_field_type_for_owner(
    record: &Ident,
    owner: &Ident,
    index: usize,
    ty: &Type,
    attributes: &[Attribute],
) -> (Type, TokenStream) {
    // Helpers have a different `Self`; preserve the authored field's owner.
    let authored_span = ty.span();
    let mut ty = ty.clone();
    FieldOwner(owner).visit_type_mut(&mut ty);
    let lints = lint_attributes(attributes).collect::<Vec<_>>();
    if lints.is_empty() {
        return (ty, TokenStream::new());
    }
    let name = format_ident!(
        "__TypeHistory{}Field{}Type",
        record,
        index,
        span = authored_span
    );
    (
        syn::parse_quote!(#name),
        quote!(#(#lints)* type #name = #ty;),
    )
}

struct FieldOwner<'a>(&'a Ident);

impl VisitMut for FieldOwner<'_> {
    fn visit_path_mut(&mut self, path: &mut Path) {
        if path.leading_colon.is_none() {
            if let Some(first) = path.segments.first_mut() {
                if first.ident == "Self" {
                    first.ident = self.0.clone();
                }
            }
        }
        visit_mut::visit_path_mut(self, path);
    }
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
