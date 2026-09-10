//! Supporting structural schema derive for concrete named records.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{Attribute, Data, DeriveInput, Error, Fields, Ident, Result};
use typehistory_codegen::{
    lint_attributes::{lint_attributes, scoped_field_type, with_lints},
    NamedField,
};

pub(super) fn expand(input: DeriveInput) -> Result<TokenStream> {
    super::input::reject_generics(&input.generics)?;
    let Data::Struct(data) = &input.data else {
        return Err(Error::new_spanned(&input, "Schema supports named records"));
    };
    let Fields::Named(fields) = &data.fields else {
        return Err(Error::new_spanned(&input, "Schema supports named records"));
    };
    for attribute in &input.attrs {
        validate_attribute(attribute, true)?;
    }
    for attribute in fields.named.iter().flat_map(|field| &field.attrs) {
        validate_attribute(attribute, false)?;
    }
    let facade = super::facade();
    let support = quote!(#facade::__private);
    let name = &input.ident;
    let visibility = &input.vis;
    let helper = format_ident!("__Typehistory{}Schema", name);
    let marker = format_ident!("__Typehistory{}SchemaWire", name);
    let mut field_aliases = Vec::new();
    let schema_fields = fields
        .named
        .iter()
        .enumerate()
        .map(|(index, field)| {
            let (ty, declaration) = scoped_field_type(name, index, &field.ty, &field.attrs);
            field_aliases.push(declaration);
            NamedField {
                name: field.ident.clone().expect("named field"),
                ty,
                attributes: field.attrs.clone(),
            }
        })
        .collect::<Vec<_>>();
    let wire = typehistory_codegen::resolved_schema::record_wire_type(
        schema_fields.iter().map(|field| (&field.name, &field.ty)),
        &support,
    )?;
    let schema_derive = typehistory_codegen::json_schema_derive::container_attributes(&support);
    let declarations = schema_fields.iter().map(|field| {
        let name = &field.name;
        let ty = &field.ty;
        let lints = lint_attributes(&field.attributes);
        let schema = typehistory_codegen::json_schema_derive::field_attribute(&support, ty);
        quote!(#(#lints)* #schema #name: #ty)
    });
    let field_contract =
        typehistory_codegen::json_schema_derive::field_impl_for_derived(name, &support);
    with_lints(
        &input.attrs,
        quote! {
            #(#field_aliases)*
            #[allow(dead_code)]
            #schema_derive
            struct #helper { #(#declarations),* }
            #[doc(hidden)]
            #visibility struct #marker;
            impl #support::WireNode for #marker {
                const SHAPE: #support::ConstantShape = <#wire as #support::WireNode>::SHAPE;
                fn schema() -> #support::SchemaShape { <#wire as #support::WireNode>::schema() }
            }
            impl #support::NonOptionalNode for #marker {}
            impl #support::ResolvedSchema for #name { type Wire = #marker; }
            impl #support::schemars::JsonSchema for #name {
                fn schema_name() -> ::std::borrow::Cow<'static, str> { stringify!(#name).into() }
                fn json_schema(generator: &mut #support::schemars::SchemaGenerator) -> #support::schemars::Schema {
                    <#helper as #support::schemars::JsonSchema>::json_schema(generator)
                }
            }
            #field_contract
        },
    )
}

fn validate_attribute(attribute: &Attribute, container: bool) -> Result<()> {
    // Closing an authored Serde record agrees with the generated wire contract.
    if container
        && attribute.path().is_ident("serde")
        && attribute
            .parse_args::<Ident>()
            .is_ok_and(|argument| argument == "deny_unknown_fields")
    {
        return Ok(());
    }
    if ["serde", "schemars", "cfg", "cfg_attr"]
        .iter()
        .any(|name| attribute.path().is_ident(name))
    {
        return Err(Error::new_spanned(
            attribute,
            "Schema supports only the container's #[serde(deny_unknown_fields)] wire attribute",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::expand;
    use syn::parse_quote;

    #[test]
    fn schema_accepts_closed_serde_records_and_rejects_changed_wire_fields() {
        assert!(expand(parse_quote!(
            #[serde(deny_unknown_fields)]
            struct Details {
                quantity: u32,
            }
        ))
        .is_ok());
        for input in [
            parse_quote!(
                #[serde(rename_all = "camelCase")]
                struct Details {
                    quantity: u32,
                }
            ),
            parse_quote!(
                #[serde(deny_unknown_fields, default)]
                struct Details {
                    quantity: u32,
                }
            ),
            parse_quote!(
                struct Details {
                    #[serde(rename = "other")]
                    quantity: u32,
                }
            ),
            parse_quote!(
                struct Details {
                    #[serde(deny_unknown_fields)]
                    quantity: u32,
                }
            ),
        ] {
            assert!(expand(input).is_err());
        }
    }
}
