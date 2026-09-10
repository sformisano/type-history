//! Shared mechanical contracts and strict codecs for authored records and values.

use crate::{lint_attributes::lint_attributes, model::NamedField};
use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};
use syn::{DeriveInput, Ident, Result};

/// Emit mechanical record contracts and strict map-only serialization codecs.
pub fn record(
    name: &Ident,
    fields: &[NamedField],
    equality: bool,
    support: &TokenStream,
) -> Result<TokenStream> {
    let serde = quote!(#support::serde);
    let redactable = quote!(#support::redactable);
    let declarations = fields.iter().map(|field| {
        let name = &field.name;
        let ty = &field.ty;
        let attrs = &field.attributes;
        quote!(#(#attrs)* #name:#ty)
    });
    let input: DeriveInput = syn::parse2(quote!(struct #name {#(#declarations),*}))?;
    let contract = crate::binding_contract::binding_contract(&input, &redactable, equality, false)?;
    let names = fields.iter().map(|field| &field.name).collect::<Vec<_>>();
    let helper_fields = fields.iter().map(|field| {
        let name = &field.name;
        let ty = &field.ty;
        let lints = lint_attributes(&field.attributes);
        quote!(#(#lints)* #name: #ty)
    });
    let keys = names
        .iter()
        .map(|name| {
            let name = name.to_string();
            name.strip_prefix("r#").unwrap_or(&name).to_owned()
        })
        .collect::<Vec<_>>();
    let length = names.len();
    let serde_path = serde.to_string().replace(' ', "");
    let helper = format_ident!("__TypehistoryInternal{}DecodeRecord", name);
    let visitor = format_ident!("__TypehistoryInternal{}RecordVisitor", name);
    let decoder = format_ident!("__typehistory_deserializer", span = Span::mixed_site());
    let map = format_ident!("__typehistory_map", span = Span::mixed_site());
    let record = format_ident!("__typehistory_record", span = Span::mixed_site());
    Ok(quote! {
        #contract
        #[derive(#serde::Deserialize)]
        #[serde(crate = #serde_path, deny_unknown_fields)]
        struct #helper {#(#helper_fields),*}
        struct #visitor;
        impl<'de> #serde::de::Visitor<'de> for #visitor {
            type Value = #name;
            fn expecting(&self, formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                formatter.write_str("a history record object")
            }
            fn visit_map<__TypehistoryMap: #serde::de::MapAccess<'de>>(self, #map: __TypehistoryMap) -> ::core::result::Result<#name, __TypehistoryMap::Error> {
                let #record = <#helper as #serde::Deserialize>::deserialize(#serde::de::value::MapAccessDeserializer::new(#map))?;
                ::core::result::Result::Ok(#name {#(#names:#record.#names),*})
            }
        }
        impl #serde::Serialize for #name {
            fn serialize<__TypehistorySerializer: #serde::Serializer>(&self, serializer:__TypehistorySerializer) -> ::core::result::Result<__TypehistorySerializer::Ok,__TypehistorySerializer::Error> {
                let mut record = #serde::Serializer::serialize_struct(serializer,stringify!(#name),#length)?;
                #(#serde::ser::SerializeStruct::serialize_field(&mut record,#keys,&self.#names)?;)*
                #serde::ser::SerializeStruct::end(record)
            }
        }
        impl<'de> #serde::Deserialize<'de> for #name {
            fn deserialize<__TypehistoryDeserializer: #serde::Deserializer<'de>>(#decoder:__TypehistoryDeserializer) -> ::core::result::Result<Self,__TypehistoryDeserializer::Error> {
                #serde::Deserializer::deserialize_map(#decoder, #visitor)
            }
        }
    })
}
