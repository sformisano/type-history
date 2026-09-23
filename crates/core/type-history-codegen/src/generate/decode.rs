//! Exact raw JSON decoding and a linear-size adjacent conversion dispatcher.

use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};

use super::{historical, Context};

pub(super) fn generate(context: &Context<'_>) -> TokenStream {
    let support = &context.paths.support;
    let error_type = &context.paths.error;
    let function = context.helper("decode_history");
    let latest = context.payload(context.history.head());
    let stable_name = context.stable_name();
    let version = format_ident!("__type_history_source_version", span = Span::mixed_site());
    let bytes = format_ident!("__type_history_bytes", span = Span::mixed_site());
    let error = format_ident!("__type_history_error", span = Span::mixed_site());
    let upgrade = context.helper("upgrade_history");
    let state = format_ident!("__type_history_state", span = Span::mixed_site());
    let decoding = historical::decode_arms(context, |payload, variant| {
        quote! {
            #variant(
                #support::decode_json_payload::<#payload>(#bytes)
                    .map_err(|#error| #error_type::decode(#stable_name, #version, #error))?
            )
        }
    });
    quote! {
        fn #function(#version: #support::PayloadVersion, #bytes: &[::core::primitive::u8])
            -> ::core::result::Result<#latest, #error_type>
        {
            let #state = match #version.get() {
                #decoding
                _ => return ::core::result::Result::Err(#error_type::unsupported(#stable_name, #version)),
            };
            #upgrade(#state)
        }
    }
}
