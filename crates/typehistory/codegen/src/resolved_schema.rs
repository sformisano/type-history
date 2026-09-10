//! Type-level wire-schema token generation.

use proc_macro2::TokenStream;
use quote::{quote, ToTokens};
use syn::{Error, Ident, Result, Type};

/// Build the structural record type after sorting and validating durable field names.
pub fn record_wire_type<'a>(
    fields: impl IntoIterator<Item = (&'a Ident, &'a Type)>,
    support: &TokenStream,
) -> Result<TokenStream> {
    let resolved = quote!(#support);
    let mut fields = fields
        .into_iter()
        .map(|(name, ty)| (wire_ident(name), type_reference(ty, &resolved)))
        .collect::<Vec<_>>();
    fields.sort_by(|left, right| left.0.cmp(&right.0));
    reject_duplicate_names(fields.iter().map(|(name, _)| name.clone()), support)?;
    let fields = fields
        .into_iter()
        .map(|(name, ty)| {
            let name = name_type(&name, &resolved);
            quote!(#resolved::Field<#name, #ty>)
        })
        .collect::<Vec<_>>();
    let fields = list_type(fields, &resolved);
    Ok(quote!(#resolved::Record<#fields>))
}

fn type_reference(ty: &Type, resolved: &TokenStream) -> TokenStream {
    quote!(<#ty as #resolved::ResolvedSchema>::Wire)
}

fn name_type(name: &str, resolved: &TokenStream) -> TokenStream {
    name.as_bytes().iter().rev().fold(
        quote!(#resolved::NameEnd),
        |tail, byte| quote!(#resolved::NameByte<#byte, #tail>),
    )
}

fn list_type(items: Vec<TokenStream>, resolved: &TokenStream) -> TokenStream {
    items.into_iter().rev().fold(
        quote!(#resolved::End),
        |tail, head| quote!(#resolved::Item<#head, #tail>),
    )
}

fn wire_ident(identifier: &Ident) -> String {
    let value = identifier.to_string();
    value.strip_prefix("r#").unwrap_or(&value).to_owned()
}

fn reject_duplicate_names(
    names: impl IntoIterator<Item = String>,
    span: impl ToTokens,
) -> Result<()> {
    let mut previous = None;
    for name in names {
        if previous.as_ref() == Some(&name) {
            return Err(Error::new_spanned(
                &span,
                format!("duplicate durable wire name `{name}`"),
            ));
        }
        previous = Some(name);
    }
    Ok(())
}
