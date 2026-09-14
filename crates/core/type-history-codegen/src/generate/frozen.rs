//! Unconditional compiler-resolved comparisons with committed wire shapes.

use proc_macro2::TokenStream;
use quote::{quote, quote_spanned};
use type_history_core::resolved::{SchemaField, SchemaShape, SchemaVariantShape};

use super::Context;

pub(super) fn generate(context: &Context<'_>) -> TokenStream {
    let support = &context.paths.support;
    let resolved = quote!(#support);
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
            let message = format!(
                "{} V{} field `{}` changed its frozen wire shape; restore its type/history boundaries and add the next version",
                context.stable_name, retained.version, expected.name
            );
            assertions.push(quote_spanned! {field.name.span()=>
                const _: () = ::core::assert!(
                    <<#ty as #resolved::ResolvedSchema>::Wire as #resolved::WireNode>::SHAPE.same(&#expected_shape),
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
                const LENGTH: usize = #resolved::schema_diagnostic::encoded_len(
                    #stable_name, #version, &EXPECTED, &ACTUAL,
                );
                const BYTES: [u8; LENGTH] = #resolved::schema_diagnostic::encode(
                    #stable_name, #version, &EXPECTED, &ACTUAL,
                );
                const MESSAGE: &str = match ::core::str::from_utf8(&BYTES) {
                    ::core::result::Result::Ok(value) => value,
                    ::core::result::Result::Err(_) =>
                        ::core::panic!("invalid frozen schema diagnostic UTF-8"),
                };
                ::core::assert!(ACTUAL.same(&EXPECTED), "{}", MESSAGE);
            };
        });
    }
    quote!(#(#assertions)*)
}

fn shape_tokens(shape: &SchemaShape, resolved: &TokenStream) -> TokenStream {
    let node = quote!(#resolved::ConstantShape);
    match shape {
        SchemaShape::Bool => quote!(#node::Bool),
        SchemaShape::I8 => quote!(#node::I8),
        SchemaShape::I16 => quote!(#node::I16),
        SchemaShape::I32 => quote!(#node::I32),
        SchemaShape::I64 => quote!(#node::I64),
        SchemaShape::I128 => quote!(#node::I128),
        SchemaShape::U8 => quote!(#node::U8),
        SchemaShape::U16 => quote!(#node::U16),
        SchemaShape::U32 => quote!(#node::U32),
        SchemaShape::U64 => quote!(#node::U64),
        SchemaShape::U128 => quote!(#node::U128),
        SchemaShape::String => quote!(#node::String),
        SchemaShape::Bytes => quote!(#node::Bytes),
        SchemaShape::Option { value } => {
            let value = shape_tokens(value, resolved);
            quote!(#node::Option(&#value))
        }
        SchemaShape::Sequence { value } => {
            let value = shape_tokens(value, resolved);
            quote!(#node::Sequence(&#value))
        }
        SchemaShape::Array { value, length } => {
            let value = shape_tokens(value, resolved);
            quote!(#node::Array(&#value, #length))
        }
        SchemaShape::Record { fields } => {
            let fields = fields_tokens(fields, resolved);
            quote!(#node::Record(#fields))
        }
        SchemaShape::Enum { variants } => {
            let variants = variants.iter().rev().fold(
                quote!(#resolved::ConstantVariants::End),
                |tail, variant| {
                    let name = name_tokens(&variant.name, resolved);
                    let shape = match &variant.shape {
                        SchemaVariantShape::Unit => quote!(#resolved::ConstantVariant::Unit),
                        SchemaVariantShape::Newtype { schema } => {
                            let shape = shape_tokens(schema, resolved);
                            quote!(#resolved::ConstantVariant::Newtype(&#shape))
                        }
                        SchemaVariantShape::Tuple { items } => {
                            let items = items.iter().rev().fold(
                                quote!(#resolved::ConstantItems::End),
                                |tail, item| {
                                    let shape = shape_tokens(item, resolved);
                                    quote!(#resolved::ConstantItems::Item(&#shape, &#tail))
                                },
                            );
                            quote!(#resolved::ConstantVariant::Tuple(#items))
                        }
                        SchemaVariantShape::Record { fields } => {
                            let fields = fields_tokens(fields, resolved);
                            quote!(#resolved::ConstantVariant::Record(#fields))
                        }
                    };
                    quote!(#resolved::ConstantVariants::Variant(#name, #shape, &#tail))
                },
            );
            quote!(#node::Enum(#variants))
        }
    }
}

fn fields_tokens(fields: &[SchemaField], resolved: &TokenStream) -> TokenStream {
    fields
        .iter()
        .rev()
        .fold(quote!(#resolved::ConstantFields::End), |tail, field| {
            let name = name_tokens(&field.name, resolved);
            let shape = shape_tokens(&field.schema, resolved);
            quote!(#resolved::ConstantFields::Field(#name, &#shape, &#tail))
        })
}

fn name_tokens(name: &str, resolved: &TokenStream) -> TokenStream {
    name.as_bytes().iter().rev().fold(
        quote!(#resolved::ConstantName::End),
        |tail, byte| quote!(#resolved::ConstantName::Byte(#byte, &#tail)),
    )
}
