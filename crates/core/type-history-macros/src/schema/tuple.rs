//! Supporting tuple-struct schema helpers.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{Error, FieldsUnnamed, Ident, Result};
use type_history_codegen::lint_attributes::scoped_field_type;

use super::validate_attribute;

pub(super) fn expand(
    fields: &FieldsUnnamed,
    name: &Ident,
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
    Ok(quote! {
        #(#aliases)*
        #schema
        impl #support::ResolvedSchema for #name { type Wire = #wire; }
        #field_contract
    })
}
