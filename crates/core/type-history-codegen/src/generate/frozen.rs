//! Unconditional compiler-resolved comparisons with committed wire shapes.

use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote, quote_spanned};
use type_history_core::resolved::SchemaShape;

use super::Context;

mod contracts;
use contracts::{presence_tokens, shape_tokens};

pub(super) fn generate(context: &Context<'_>) -> TokenStream {
    let support = &context.paths.support;
    let resolved = quote!(#support);
    let value = format_ident!("__type_history_value", span = Span::mixed_site());
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
                const _: () = ::core::assert!(
                    <<#ty as #resolved::ResolvedSchema>::Wire as #resolved::WireNode>::FIELD_PRESENCE.same(#expected_presence)
                        && <<#ty as #resolved::ResolvedSchema>::Wire as #resolved::WireNode>::SHAPE.same(&#expected_shape),
                    #message,
                );
            });
        }
        let payload = context.payload(retained.version);
        let expected = shape_tokens(shape, &resolved);
        let stable_name = context.stable_name;
        let version = retained.version;
        assertions.push(quote_spanned! {context.input.name.span()=>
            const _: () = {
                const EXPECTED: #resolved::ConstantShape = #expected;
                const ACTUAL: #resolved::ConstantShape =
                    <<#payload as #resolved::ResolvedSchema>::Wire as #resolved::WireNode>::SHAPE;
                const LENGTH: ::core::primitive::usize = #resolved::schema_diagnostic::encoded_len(
                    #stable_name, #version, &EXPECTED, &ACTUAL,
                );
                const BYTES: [::core::primitive::u8; LENGTH] = #resolved::schema_diagnostic::encode(
                    #stable_name, #version, &EXPECTED, &ACTUAL,
                );
                const MESSAGE: &::core::primitive::str = match ::core::str::from_utf8(&BYTES) {
                    ::core::result::Result::Ok(#value) => #value,
                    ::core::result::Result::Err(_) =>
                        ::core::panic!("invalid frozen schema diagnostic UTF-8"),
                };
                ::core::assert!(ACTUAL.same(&EXPECTED), "{}", MESSAGE);
            };
        });
    }
    quote!(#(#assertions)*)
}
