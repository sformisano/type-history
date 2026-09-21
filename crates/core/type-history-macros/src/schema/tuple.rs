//! Supporting tuple-struct schema helpers.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{Error, FieldsUnnamed, Ident, Result, Visibility};
use type_history_codegen::lint_attributes::scoped_field_type;

use super::validate_attribute;

pub(super) fn expand(
    fields: &FieldsUnnamed,
    name: &Ident,
    visibility: &Visibility,
    helper: &Ident,
    support: &TokenStream,
) -> Result<TokenStream> {
    if fields.unnamed.is_empty() {
        return Err(Error::new_spanned(
            fields,
            "Schema tuple structs require at least one field",
        ));
    }
    for attribute in fields.unnamed.iter().flat_map(|field| &field.attrs) {
        validate_attribute(attribute, false)?;
    }
    let mut aliases = Vec::new();
    let types = fields
        .unnamed
        .iter()
        .enumerate()
        .map(|(index, field)| {
            let (ty, declaration) = scoped_field_type(name, index, &field.ty, &field.attrs);
            aliases.push(declaration);
            ty
        })
        .collect::<Vec<_>>();
    let wire = if types.len() == 1 {
        type_history_codegen::resolved_schema::newtype_wire_type(&types[0], support)
    } else {
        type_history_codegen::resolved_schema::tuple_wire_type(&types, support)
    };
    let schema = type_history_codegen::json_schema_derive::tuple(name, helper, &types, support);
    let field_contract = if types.len() == 1 {
        type_history_codegen::json_schema_derive::field_impl_delegating_to(name, &types[0], support)
    } else {
        type_history_codegen::json_schema_derive::field_impl_for_derived(name, support)
    };
    let marker = format_ident!("__TypeHistory{}SchemaWire", name);
    let wire_alias = format_ident!("__TypeHistory{}TupleWire", name);
    // A public associated type must not expose private field types. Keep their
    // shape behind a nominal marker, but retain nullability for Option<T>'s
    // nested-option rejection, including through aliases and other newtypes.
    Ok(quote! {
        #(#aliases)*
        #schema
        // Resolve authored field constants outside the generated generic scope.
        type #wire_alias = #wire;
        #[doc(hidden)]
        #visibility struct #marker<const NULLABLE: ::core::primitive::bool>;
        impl<const NULLABLE: ::core::primitive::bool> #support::WireNode for #marker<NULLABLE> {
            const SHAPE: #support::ConstantShape = <#wire_alias as #support::WireNode>::SHAPE;
            fn schema() -> #support::SchemaShape { <#wire_alias as #support::WireNode>::schema() }
        }
        impl #support::NonOptionalNode for #marker<false> {}
        impl #support::ResolvedSchema for #name {
            type Wire = #marker<{
                ::core::matches!(<#wire_alias as #support::WireNode>::SHAPE, #support::ConstantShape::Option(_))
            }>;
        }
        #field_contract
    })
}
