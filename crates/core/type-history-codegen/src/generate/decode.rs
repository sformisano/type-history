//! Exact raw JSON decoding and a linear-size adjacent conversion dispatcher.

use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};

use super::Context;

pub(super) fn generate(context: &Context<'_>) -> TokenStream {
    let support = &context.paths.support;
    let error_type = &context.paths.error;
    let function = context.helper("decode_history");
    let latest = context.payload(context.history.head());
    let stable_name = context.stable_name();
    let version = format_ident!("__type_history_source_version", span = Span::mixed_site());
    let bytes = format_ident!("__type_history_bytes", span = Span::mixed_site());
    let error = format_ident!("__type_history_error", span = Span::mixed_site());
    if context.history.head() == 1 {
        let current = context.history.head();
        return quote! {
            fn #function(#version: #support::PayloadVersion, #bytes: &[::core::primitive::u8])
                -> ::core::result::Result<#latest, #error_type>
            {
                if #version.get() != #current {
                    return ::core::result::Result::Err(#error_type::unsupported(#stable_name, #version));
                }
                #support::decode_json_payload::<#latest>(#bytes)
                    .map_err(|#error| #error_type::decode(#stable_name, #version, #error))
            }
        };
    }
    let state = format_ident!("__type_history_state", span = Span::mixed_site());
    let value = format_ident!("__type_history_value", span = Span::mixed_site());
    let state_type = format_ident!("__TypeHistoryRetainedPayload", span = Span::mixed_site());
    let variants = context.history.versions().iter().map(|retained| {
        let variant = format_ident!("V{}", retained.version);
        let payload = context.payload(retained.version);
        quote!(#variant(#payload))
    });
    let decoding = context.history.versions().iter().map(|retained| {
        let number = retained.version;
        let variant = format_ident!("V{number}");
        let payload = context.payload(number);
        quote! {
            #number => #state_type::#variant(
                #support::decode_json_payload::<#payload>(#bytes)
                    .map_err(|#error| #error_type::decode(#stable_name, #version, #error))?
            )
        }
    });
    let advances = context.history.transitions().iter().map(|transition| {
        let from = format_ident!("V{}", transition.from);
        let to = format_ident!("V{}", transition.to);
        let function = context.helper(&format!("upcast_v{}_v{}", transition.from, transition.to));
        quote!(#state_type::#from(#value) => #state_type::#to(#function(#value, #version)?))
    });
    let final_variant = format_ident!("V{}", context.history.head());
    quote! {
        fn #function(#version: #support::PayloadVersion, #bytes: &[::core::primitive::u8])
            -> ::core::result::Result<#latest, #error_type>
        {
            // Keep historical values owned without adding per-transition heap allocations.
            #[allow(clippy::large_enum_variant)]
            enum #state_type { #(#variants),* }
            let mut #state = match #version.get() {
                #(#decoding,)*
                _ => return ::core::result::Result::Err(#error_type::unsupported(#stable_name, #version)),
            };
            loop {
                #state = match #state {
                    #(#advances,)*
                    #state_type::#final_variant(#value) => return ::core::result::Result::Ok(#value),
                };
            }
        }
    }
}
