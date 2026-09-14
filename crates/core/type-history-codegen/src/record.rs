//! Strict map-only codecs for generated named records.

use crate::{identifier::helper_name, json_schema_derive::parameter, model::NamedField};
use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};
use syn::Ident;

/// Emit strict map-only serialization codecs without choosing ordinary record traits.
pub fn record(name: &Ident, fields: &[NamedField], support: &TokenStream) -> TokenStream {
    let serde = quote!(#support::serde);
    let names = fields.iter().map(|field| &field.name).collect::<Vec<_>>();
    let params = (0..fields.len()).map(parameter).collect::<Vec<_>>();
    let types = fields.iter().map(|field| &field.ty).collect::<Vec<_>>();
    let helper_fields = fields.iter().zip(&params).map(|(field, ty)| {
        let name = &field.name;
        quote!(pub(super) #name: #ty)
    });
    let keys = names
        .iter()
        .map(|name| {
            let name = name.to_string();
            name.strip_prefix("r#").unwrap_or(&name).to_owned()
        })
        .collect::<Vec<_>>();
    let length = names.len();
    let module = helper_name("decode", name);
    let namespace = helper_name("decode_support", name);
    let helper = format_ident!("__TypeHistoryInternal{}DecodeRecord", name);
    let visitor = format_ident!("__TypeHistoryInternal{}RecordVisitor", name);
    let decoder = format_ident!("__type_history_deserializer", span = Span::mixed_site());
    let map = format_ident!("__type_history_map", span = Span::mixed_site());
    let record = format_ident!("__type_history_record", span = Span::mixed_site());
    let formatter = format_ident!("__type_history_formatter", span = Span::mixed_site());
    let serializer = format_ident!("__type_history_serializer", span = Span::mixed_site());
    quote! {
        use #support as #namespace;
        mod #module {
            use super::#namespace as __support;
            #[derive(__support::serde::Deserialize)]
            #[serde(crate = "__support::serde", deny_unknown_fields)]
            pub(super) struct #helper<#(#params),*> {#(#helper_fields),*}
        }
        struct #visitor;
        impl<'de> #serde::de::Visitor<'de> for #visitor {
            type Value = #name;
            fn expecting(&self, #formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                #formatter.write_str("a history record object")
            }
            fn visit_map<__TypeHistoryMap: #serde::de::MapAccess<'de>>(self, #map: __TypeHistoryMap) -> ::core::result::Result<#name, __TypeHistoryMap::Error> {
                let #record = <#module::#helper<#(#types),*> as #serde::Deserialize>::deserialize(#serde::de::value::MapAccessDeserializer::new(#map))?;
                ::core::result::Result::Ok(#name {#(#names:#record.#names),*})
            }
        }
        impl #serde::Serialize for #name {
            fn serialize<__TypeHistorySerializer: #serde::Serializer>(&self, #serializer:__TypeHistorySerializer) -> ::core::result::Result<__TypeHistorySerializer::Ok,__TypeHistorySerializer::Error> {
                let mut #record = #serde::Serializer::serialize_struct(#serializer,::core::stringify!(#name),#length)?;
                #(#serde::ser::SerializeStruct::serialize_field(&mut #record,#keys,&self.#names)?;)*
                #serde::ser::SerializeStruct::end(#record)
            }
        }
        impl<'de> #serde::Deserialize<'de> for #name {
            fn deserialize<__TypeHistoryDeserializer: #serde::Deserializer<'de>>(#decoder:__TypeHistoryDeserializer) -> ::core::result::Result<Self,__TypeHistoryDeserializer::Error> {
                #serde::Deserializer::deserialize_map(#decoder, #visitor)
            }
        }
    }
}
