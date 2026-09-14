//! Versioned value dispatch over the existing retained payload codecs.

use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};

use super::Context;

pub(super) fn generate(context: &Context<'_>) -> TokenStream {
    let support = &context.paths.support;
    let error_type = &context.paths.error;
    let visibility = &context.input.visibility;
    let historical = format_ident!("__TypeHistory{}VersionedPayload", context.input.name);
    let latest = context.payload(context.history.head());
    let final_variant = format_ident!("V{}", context.history.head());
    let stable_name = context.stable_name();
    let current_version = context.version(context.history.head());
    let value = format_ident!("__type_history_value", span = Span::mixed_site());
    let source_version = format_ident!("__type_history_source_version", span = Span::mixed_site());
    let deserializer = format_ident!("__type_history_deserializer", span = Span::mixed_site());
    let serializer = format_ident!("__type_history_serializer", span = Span::mixed_site());
    let variants = context.history.versions().iter().map(|retained| {
        let variant = format_ident!("V{}", retained.version);
        let payload = context.payload(retained.version);
        quote!(#variant(#payload))
    });
    let serialization = context.history.versions().iter().map(|retained| {
        let variant = format_ident!("V{}", retained.version);
        quote!(Self::#variant(#value) => #support::serde::Serialize::serialize(#value, #serializer))
    });
    let source_versions = context.history.versions().iter().map(|retained| {
        let variant = format_ident!("V{}", retained.version);
        let version = context.version(retained.version);
        quote!(#historical::#variant(_) => #version)
    });
    let decoding = context.history.versions().iter().map(|retained| {
        let number = retained.version;
        let variant = format_ident!("V{number}");
        let payload = context.payload(number);
        quote! {
            #number => <#payload as #support::serde::Deserialize<'de>>::deserialize(#deserializer)
                .map(#historical::#variant)
        }
    });
    let upgrade = if context.history.head() == 1 {
        quote! {
            let #historical::#final_variant(#value) = #value;
            ::core::result::Result::Ok(#value)
        }
    } else {
        let state = format_ident!("__type_history_state", span = Span::mixed_site());
        let advances = context.history.transitions().iter().map(|transition| {
            let from = format_ident!("V{}", transition.from);
            let to = format_ident!("V{}", transition.to);
            let function = context.helper(&format!("upcast_v{}_v{}", transition.from, transition.to));
            quote!(#historical::#from(#value) => #historical::#to(#function(#value, #source_version)?))
        });
        quote! {
            let #source_version = <Self as #support::VersionedHistory>::source_version(&#value);
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
        #[derive(::core::clone::Clone)]
        #[allow(clippy::large_enum_variant)]
        #visibility enum #historical { #(#variants),* }

        impl #support::serde::Serialize for #historical {
            fn serialize<__TypeHistorySerializer: #support::serde::Serializer>(
                &self,
                #serializer: __TypeHistorySerializer,
            ) -> ::core::result::Result<__TypeHistorySerializer::Ok, __TypeHistorySerializer::Error> {
                match self { #(#serialization),* }
            }
        }

        impl #support::VersionedHistory for #latest {
            type Historical = #historical;
            type Error = #error_type;
            const STABLE_NAME: #support::StableName = #stable_name;
            const CURRENT_VERSION: #support::PayloadVersion = #current_version;

            fn into_historical(self) -> Self::Historical {
                #historical::#final_variant(self)
            }

            fn source_version(#value: &Self::Historical) -> #support::PayloadVersion {
                match #value { #(#source_versions),* }
            }

            fn deserialize_historical<'de, __TypeHistoryDeserializer: #support::serde::Deserializer<'de>>(
                #source_version: #support::PayloadVersion,
                #deserializer: __TypeHistoryDeserializer,
            ) -> ::core::result::Result<Self::Historical, __TypeHistoryDeserializer::Error> {
                match #source_version.get() {
                    #(#decoding,)*
                    _ => ::core::result::Result::Err(
                        <__TypeHistoryDeserializer::Error as #support::serde::de::Error>::custom(
                            #error_type::unsupported(#stable_name, #source_version),
                        ),
                    ),
                }
            }

            fn upgrade(#value: Self::Historical) -> ::core::result::Result<Self, Self::Error> {
                #upgrade
            }
        }

        impl #latest {
            /// Wrap this current value with its stable name and payload version.
            pub fn into_versioned(self) -> #support::Versioned<Self> {
                #support::Versioned::new(self)
            }

            /// Convert a versioned value into the current type.
            ///
            /// Failures retain the stored version, failing step, and concrete callback source.
            pub fn from_versioned(
                #value: #support::Versioned<Self>,
            ) -> ::core::result::Result<Self, #error_type> {
                <Self as #support::VersionedHistory>::from_versioned(#value)
            }
        }
    }
}
