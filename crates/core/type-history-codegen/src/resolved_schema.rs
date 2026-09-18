//! Type-level wire-schema token generation.

use proc_macro2::TokenStream;
use quote::{quote, ToTokens};
use syn::{Error, Fields, Ident, Result, Type, Variant};

/// Build an externally tagged enum from its concrete, validated variants.
pub fn enum_wire_type<'a>(
    variants: impl IntoIterator<Item = &'a Variant>,
    support: &TokenStream,
) -> Result<TokenStream> {
    let mut variants = variants.into_iter().collect::<Vec<_>>();
    variants.sort_by_key(|variant| wire_ident(&variant.ident));
    reject_duplicate_names(
        variants.iter().map(|variant| wire_ident(&variant.ident)),
        support,
    )?;
    let variants = variants
        .into_iter()
        .map(|variant| {
            let name = name_type(&wire_ident(&variant.ident), support);
            let shape = match &variant.fields {
                Fields::Unit => quote!(#support::UnitVariant),
                Fields::Unnamed(fields) if fields.unnamed.len() == 1 => {
                    let ty = type_reference(&fields.unnamed[0].ty, support);
                    quote!(#support::NewtypeVariant<#ty>)
                }
                Fields::Unnamed(fields) => {
                    let items = list_type(
                        fields
                            .unnamed
                            .iter()
                            .map(|field| type_reference(&field.ty, support))
                            .collect(),
                        support,
                    );
                    quote!(#support::TupleVariant<#items>)
                }
                Fields::Named(fields) => {
                    let fields = record_fields_type(
                        fields
                            .named
                            .iter()
                            .map(|field| (field.ident.as_ref().expect("named field"), &field.ty)),
                        support,
                    )?;
                    quote!(#support::RecordVariant<#fields>)
                }
            };
            Ok(quote!(#support::Variant<#name, #shape>))
        })
        .collect::<Result<Vec<_>>>()?;
    let variants = list_type(variants, support);
    Ok(quote!(#support::Enumeration<#variants>))
}

/// Build the structural record type after sorting and validating durable field names.
pub fn record_wire_type<'a>(
    fields: impl IntoIterator<Item = (&'a Ident, &'a Type)>,
    support: &TokenStream,
) -> Result<TokenStream> {
    let fields = record_fields_type(fields, support)?;
    Ok(quote!(#support::Record<#fields>))
}

/// Build the transparent wire marker for a supporting one-field tuple struct.
pub fn newtype_wire_type(ty: &Type, support: &TokenStream) -> TokenStream {
    let value = type_reference(ty, support);
    quote!(#support::Newtype<#value>)
}

/// Build an ordered tuple wire marker from its concrete field types.
pub fn tuple_wire_type<'a>(
    types: impl IntoIterator<Item = &'a Type>,
    support: &TokenStream,
) -> TokenStream {
    let items = types
        .into_iter()
        .map(|ty| type_reference(ty, support))
        .collect();
    let items = list_type(items, support);
    quote!(#support::Tuple<#items>)
}

fn record_fields_type<'a>(
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
    Ok(fields)
}

fn type_reference(ty: &Type, resolved: &TokenStream) -> TokenStream {
    quote!(<#ty as #resolved::ResolvedSchema>::Wire)
}

fn name_type(name: &str, resolved: &TokenStream) -> TokenStream {
    let chunks = name.as_bytes().chunks_exact(8);
    let tail = chunks.remainder().iter().rev().fold(
        quote!(#resolved::NameEnd),
        |tail, byte| quote!(#resolved::NameByte<#byte, #tail>),
    );
    chunks.rev().fold(tail, |tail, chunk| {
        let packed = u64::from_be_bytes(chunk.try_into().expect("eight-byte chunk"));
        quote!(#resolved::NameChunk<#packed, #tail>)
    })
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
