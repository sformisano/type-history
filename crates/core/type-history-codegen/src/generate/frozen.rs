//! Compiler-resolved comparisons with committed wire shapes.

use proc_macro2::TokenStream;
use quote::{quote, quote_spanned};
use type_history_core::resolved::SchemaShape;

use super::{Context, GenerationOptions};

mod contracts;
use contracts::{presence_tokens, shape_tokens};

pub(super) fn generate(context: &Context<'_>, options: &GenerationOptions) -> TokenStream {
    let support = &context.paths.support;
    let resolved = quote!(#support);
    let guard = options
        .observation_cfg
        .as_ref()
        .map(|cfg| quote!(#[cfg(not(all(test, #cfg)))]));
    let mut assertions = Vec::new();
    for retained in context.history.versions() {
        let Some(shape) = context.history.frozen_shapes().get(&retained.version) else {
            continue;
        };
        let SchemaShape::Record { fields } = shape else {
            unreachable!("authorized record history has a record schema");
        };
        for expected in fields {
            let field = retained
                .fields
                .iter()
                .find(|field| field.name.to_string().trim_start_matches("r#") == expected.name)
                .expect("authorized history contains every frozen field");
            let ty = context.field_type(retained.version, &field.name);
            let expected_shape = shape_tokens(&expected.schema, &resolved);
            let expected_presence = presence_tokens(expected.presence, &resolved);
            let message = format!(
                "{} V{} field `{}` changed its frozen wire shape; restore its type/history boundaries and add the next version",
                context.stable_name, retained.version, expected.name
            );
            assertions.push(quote_spanned! {field.name.span()=>
                #guard
                const _: () = ::core::assert!(
                    <<#ty as #resolved::ResolvedSchema>::Wire as #resolved::WireNode>::FIELD_PRESENCE.same(#expected_presence)
                        && <<#ty as #resolved::ResolvedSchema>::Wire as #resolved::WireNode>::SHAPE.same(&#expected_shape),
                    #message,
                );
            });
        }
        let payload = context.payload(retained.version);
        let expected = shape_tokens(shape, &resolved);
        let message = format!(
            "{} V{} changed its frozen wire shape; run the frontend history check for details, restore its type/history boundaries and add the next version",
            context.stable_name, retained.version,
        );
        assertions.push(quote_spanned! {context.input.name.span()=>
            #guard
            const _: () = ::core::assert!(
                <<#payload as #resolved::ResolvedSchema>::Wire as #resolved::WireNode>::SHAPE.same(&#expected),
                #message,
            );
        });
    }
    quote!(#(#assertions)*)
}
