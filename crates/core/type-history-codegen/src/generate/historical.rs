//! Shared retained state, version dispatch, and adjacent upgrades.

use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};
use syn::Ident;

use super::{Context, GenerationOptions, PayloadTraits};

pub(super) fn state_type(context: &Context<'_>) -> Ident {
    format_ident!("__TypeHistory{}VersionedPayload", context.input.name)
}

/// Keep selection of retained versions in one place for both transport adapters.
pub(super) fn decode_arms(
    context: &Context<'_>,
    decode: impl Fn(Ident, TokenStream) -> TokenStream,
) -> TokenStream {
    let historical = state_type(context);
    let arms = context.history.versions().iter().map(|retained| {
        let number = retained.version;
        let variant = format_ident!("V{number}");
        let expression = decode(context.payload(number), quote!(#historical::#variant));
        quote!(#number => #expression)
    });
    quote!(#(#arms,)*)
}

pub(super) fn generate(context: &Context<'_>, options: &GenerationOptions) -> TokenStream {
    let support = &context.paths.support;
    let error_type = &context.paths.error;
    let visibility = &context.input.visibility;
    let historical = state_type(context);
    let latest = context.payload(context.history.head());
    let final_variant = format_ident!("V{}", context.history.head());
    let source_function = context.helper("source_version");
    let upgrade_function = context.helper("upgrade_history");
    let value = format_ident!("__type_history_value", span = Span::mixed_site());
    let state = format_ident!("__type_history_state", span = Span::mixed_site());
    let source_version = format_ident!("__type_history_source_version", span = Span::mixed_site());
    let variants = context.history.versions().iter().map(|retained| {
        let variant = format_ident!("V{}", retained.version);
        let payload = context.payload(retained.version);
        quote!(#variant(#payload))
    });
    let source_versions = context.history.versions().iter().map(|retained| {
        let variant = format_ident!("V{}", retained.version);
        let version = context.version(retained.version);
        quote!(#historical::#variant(_) => #version)
    });
    let clone = match options.payload_traits {
        PayloadTraits::Native => quote!(#[derive(::core::clone::Clone)]),
        PayloadTraits::Frontend => TokenStream::new(),
    };
    let upgrade = if context.history.head() == 1 {
        quote! {
            let #historical::#final_variant(#value) = #value;
            ::core::result::Result::Ok(#value)
        }
    } else {
        let advances = context.history.transitions().iter().map(|transition| {
            let from = format_ident!("V{}", transition.from);
            let to = format_ident!("V{}", transition.to);
            let function = context.helper(&format!("upcast_v{}_v{}", transition.from, transition.to));
            quote!(#historical::#from(#value) => #historical::#to(#function(#value, #source_version)?))
        });
        quote! {
            let #source_version = #source_function(&#value);
            let mut #state = #value;
            loop {
                #state = match #state {
                    #(#advances,)*
                    #historical::#final_variant(#value) => return ::core::result::Result::Ok(#value),
                };
            }
        }
    };
    quote! {
        #[doc(hidden)]
        #clone
        #[allow(clippy::large_enum_variant)]
        #visibility enum #historical { #(#variants),* }

        // A single-version raw frontend needs no version extraction at runtime.
        #[allow(dead_code)]
        fn #source_function(#value: &#historical) -> #support::PayloadVersion {
            match #value { #(#source_versions),* }
        }

        fn #upgrade_function(#value: #historical) -> ::core::result::Result<#latest, #error_type> {
            #upgrade
        }
    }
}
