//! Shared mechanical value contracts and safe redaction generation.
use crate::sensitivity::SensitivityMetadata;
use proc_macro2::{Span, TokenStream, TokenTree};
use quote::{format_ident, quote, quote_spanned};
use std::collections::BTreeSet;
use syn::spanned::Spanned;
use syn::{Data, DataEnum, DataStruct, DeriveInput, Error, Field, Fields, Ident, Result};

/// Emit Clone, selected equality traits, masked Debug, and declared field redaction.
pub fn binding_contract(
    input: &DeriveInput,
    redactable: &TokenStream,
    equality: bool,
    eq: bool,
) -> Result<TokenStream> {
    let name = &input.ident;
    let mapper_type = fresh_mapper_type(quote!(#input #redactable));
    let (clone_body, equality_body, redact_body) = match &input.data {
        Data::Struct(data) => struct_contract(name, data, redactable)?,
        Data::Enum(data) => enum_contract(data, redactable)?,
        Data::Union(union) => {
            return Err(Error::new_spanned(
                union.union_token,
                "unions cannot be persisted values",
            ));
        }
    };
    let equality = equality.then(|| {
        quote! {
            impl ::core::cmp::PartialEq for #name {
                fn eq(&self, other: &Self) -> bool { #equality_body }
            }
        }
    });
    let eq = eq.then(|| quote!(impl ::core::cmp::Eq for #name {}));
    let redaction = quote! {
        impl #redactable::RedactableWithMapper for #name {
            fn redact_with<#mapper_type: #redactable::RedactableMapper>(self, mapper: &#mapper_type) -> Self { #redact_body }
        }
        impl #redactable::Redactable for #name {}
    };
    Ok(quote! {
        impl ::core::clone::Clone for #name {
            fn clone(&self) -> Self { #clone_body }
        }
        #equality #eq #redaction
        impl ::core::fmt::Debug for #name {
            fn fmt(&self, formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                formatter.write_str(concat!(stringify!(#name), "(<redacted>)"))
            }
        }
        impl #redactable::ToRedacted for #name {
            fn to_redacted(&self) -> #redactable::RedactedValue {
                #redactable::ToRedacted::to_redacted(&#redactable::BypassTextRedaction(
                    concat!(stringify!(#name), "(<redacted>)").to_owned()
                ))
            }
        }
    })
}

fn fresh_mapper_type(tokens: TokenStream) -> Ident {
    fn collect_identifiers(tokens: TokenStream, identifiers: &mut BTreeSet<String>) {
        for token in tokens {
            match token {
                TokenTree::Ident(identifier) => {
                    identifiers.insert(identifier.to_string().trim_start_matches("r#").to_owned());
                }
                TokenTree::Group(group) => collect_identifiers(group.stream(), identifiers),
                TokenTree::Punct(_) | TokenTree::Literal(_) => {}
            }
        }
    }

    let mut identifiers = BTreeSet::new();
    collect_identifiers(tokens, &mut identifiers);
    let mut name = String::from("__TypehistoryMapper");
    while identifiers.contains(&name) {
        name.push('X');
    }
    Ident::new(&name, Span::mixed_site())
}

fn struct_contract(
    name: &Ident,
    data: &DataStruct,
    redactable: &TokenStream,
) -> Result<(TokenStream, TokenStream, TokenStream)> {
    match &data.fields {
        Fields::Named(fields) => {
            let names = fields
                .named
                .iter()
                .map(|field| field.ident.as_ref().expect("named field"))
                .collect::<Vec<_>>();
            let redacted = fields
                .named
                .iter()
                .map(|field| {
                    let name = field.ident.as_ref().expect("named field");
                    redact_expression(quote!(self.#name), field, redactable)
                        .map(|value| quote!(#name: #value))
                })
                .collect::<Result<Vec<_>>>()?;
            Ok((
                quote!(Self { #(#names: self.#names.clone()),* }),
                quote!(true #(&& self.#names == other.#names)*),
                quote!(#name { #(#redacted),* }),
            ))
        }
        Fields::Unnamed(fields) if fields.unnamed.len() == 1 => {
            let redacted = redact_expression(quote!(self.0), &fields.unnamed[0], redactable)?;
            Ok((
                quote!(Self(self.0.clone())),
                quote!(self.0 == other.0),
                quote!(#name(#redacted)),
            ))
        }
        other => Err(Error::new_spanned(
            other,
            "records require named fields, or one transparent value newtype field",
        )),
    }
}

fn enum_contract(
    data: &DataEnum,
    redactable: &TokenStream,
) -> Result<(TokenStream, TokenStream, TokenStream)> {
    if data.variants.is_empty() {
        return Ok((
            quote!(match *self {}),
            quote!({
                let _ = other;
                match *self {}
            }),
            quote!(match self {}),
        ));
    }
    let mut clone_arms = Vec::new();
    let mut equality_arms = Vec::new();
    let mut redact_arms = Vec::new();
    for variant in &data.variants {
        let variant_name = &variant.ident;
        match &variant.fields {
            Fields::Unit => {
                clone_arms.push(quote!(Self::#variant_name => Self::#variant_name));
                equality_arms.push(quote!((Self::#variant_name, Self::#variant_name) => true));
                redact_arms.push(quote!(Self::#variant_name => Self::#variant_name));
            }
            Fields::Unnamed(fields) => {
                let left = numbered("left", fields.unnamed.len());
                let right = numbered("right", fields.unnamed.len());
                let owned = numbered("field", fields.unnamed.len());
                let redacted = fields
                    .unnamed
                    .iter()
                    .zip(&owned)
                    .map(|(field, value)| redact_expression(quote!(#value), field, redactable))
                    .collect::<Result<Vec<_>>>()?;
                clone_arms.push(quote!(
                    Self::#variant_name(#(#left),*) => Self::#variant_name(#((*#left).clone()),*)
                ));
                equality_arms.push(quote!(
                    (Self::#variant_name(#(#left),*), Self::#variant_name(#(#right),*)) =>
                        true #(&& #left == #right)*
                ));
                redact_arms.push(quote!(
                    Self::#variant_name(#(#owned),*) => Self::#variant_name(#(#redacted),*)
                ));
            }
            Fields::Named(fields) => {
                let names = fields
                    .named
                    .iter()
                    .map(|field| field.ident.as_ref().expect("named field"))
                    .collect::<Vec<_>>();
                let left = names
                    .iter()
                    .map(|name| format_ident!("left_{name}"))
                    .collect::<Vec<_>>();
                let right = names
                    .iter()
                    .map(|name| format_ident!("right_{name}"))
                    .collect::<Vec<_>>();
                let redacted = fields
                    .named
                    .iter()
                    .map(|field| {
                        let field_name = field.ident.as_ref().expect("named field");
                        redact_expression(quote!(#field_name), field, redactable)
                            .map(|value| quote!(#field_name: #value))
                    })
                    .collect::<Result<Vec<_>>>()?;
                clone_arms.push(quote!(
                    Self::#variant_name { #(#names: #left),* } =>
                        Self::#variant_name { #(#names: (*#left).clone()),* }
                ));
                equality_arms.push(quote!(
                    (Self::#variant_name { #(#names: #left),* },
                     Self::#variant_name { #(#names: #right),* }) =>
                        true #(&& #left == #right)*
                ));
                redact_arms.push(quote!(
                    Self::#variant_name { #(#names),* } => Self::#variant_name { #(#redacted),* }
                ));
            }
        }
    }
    let fallback = (data.variants.len() > 1).then(|| quote!(_ => false,));
    Ok((
        quote!(match self { #(#clone_arms),* }),
        quote!(match (self, other) { #(#equality_arms,)* #fallback }),
        quote!(match self { #(#redact_arms),* }),
    ))
}

fn numbered(stem: &str, length: usize) -> Vec<Ident> {
    (0..length)
        .map(|index| format_ident!("{stem}_{index}"))
        .collect()
}

fn redact_expression(
    value: TokenStream,
    field: &Field,
    redactable: &TokenStream,
) -> Result<TokenStream> {
    match crate::sensitivity::parse_sensitivity_metadata(&field.attrs)? {
        SensitivityMetadata::Inherited => {
            Ok(quote!(#redactable::RedactableWithMapper::redact_with(#value, mapper)))
        }
        SensitivityMetadata::Sensitive => {
            Ok(quote!(#redactable::RedactableMapper::map_scalar(mapper, #value)))
        }
        SensitivityMetadata::SensitivePolicy(policy) => Ok(
            quote_spanned!(field.ty.span()=> <_ as #redactable::__private::PolicyField<#policy>>::apply_field(#value, mapper)),
        ),
        SensitivityMetadata::NotSensitive => Ok(value),
    }
}
