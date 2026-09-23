//! Versioned value dispatch over the existing retained payload codecs.

use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};

use super::{historical, Context};

pub(super) fn generate(context: &Context<'_>) -> TokenStream {
    let support = &context.paths.support;
    let error_type = &context.paths.error;
    let historical = historical::state_type(context);
    let latest = context.payload(context.history.head());
    let final_variant = format_ident!("V{}", context.history.head());
    let stable_name = context.stable_name();
    let current_version = context.version(context.history.head());
    let value = format_ident!("__type_history_value", span = Span::mixed_site());
    let source_version = format_ident!("__type_history_source_version", span = Span::mixed_site());
    let deserializer = format_ident!("__type_history_deserializer", span = Span::mixed_site());
    let serializer = format_ident!("__type_history_serializer", span = Span::mixed_site());
    let serialization = context.history.versions().iter().map(|retained| {
        let variant = format_ident!("V{}", retained.version);
        quote!(Self::#variant(#value) => #support::serde::Serialize::serialize(#value, #serializer))
    });
    let source_function = context.helper("source_version");
    let upgrade_function = context.helper("upgrade_history");
    let decoding = historical::decode_arms(context, |payload, variant| {
        quote! {
            <#payload as #support::serde::Deserialize<'de>>::deserialize(#deserializer)
                .map(#variant)
        }
    });
    quote! {
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
                #source_function(#value)
            }

            fn deserialize_historical<'de, __TypeHistoryDeserializer: #support::serde::Deserializer<'de>>(
                #source_version: #support::PayloadVersion,
                #deserializer: __TypeHistoryDeserializer,
            ) -> ::core::result::Result<Self::Historical, __TypeHistoryDeserializer::Error> {
                match #source_version.get() {
                    #decoding
                    _ => ::core::result::Result::Err(
                        <__TypeHistoryDeserializer::Error as #support::serde::de::Error>::custom(
                            #error_type::unsupported(#stable_name, #source_version),
                        ),
                    ),
                }
            }

            fn upgrade(#value: Self::Historical) -> ::core::result::Result<Self, Self::Error> {
                #upgrade_function(#value)
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
