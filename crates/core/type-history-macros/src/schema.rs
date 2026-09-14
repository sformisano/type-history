//! Supporting structural schemas for concrete named records and enums.

mod enumeration;

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{Attribute, Data, DeriveInput, Error, Fields, FieldsNamed, Ident, Result};
use type_history_codegen::{
    lint_attributes::{scoped_field_type, with_lints},
    NamedField,
};

pub(super) fn expand(input: DeriveInput) -> Result<TokenStream> {
    super::input::reject_generics(&input.generics)?;
    for attribute in &input.attrs {
        validate_attribute(attribute, true)?;
    }
    let facade = super::facade();
    let support = quote!(#facade::__private);
    let name = &input.ident;
    let visibility = &input.vis;
    let helper = format_ident!("__TypeHistory{}Schema", name);
    let marker = format_ident!("__TypeHistory{}SchemaWire", name);
    let (wire, declaration) = match &input.data {
        Data::Struct(data) => {
            let Fields::Named(fields) = &data.fields else {
                return Err(Error::new_spanned(
                    &input,
                    "Schema supports named records and enums",
                ));
            };
            record(fields, name, &support)?
        }
        Data::Enum(data) => enumeration::expand(data, name, &helper, &support)?,
        Data::Union(_) => {
            return Err(Error::new_spanned(
                &input,
                "Schema supports named records and enums",
            ));
        }
    };
    let field_contract =
        type_history_codegen::json_schema_derive::field_impl_for_derived(name, &support);
    with_lints(
        &input.attrs,
        quote! {
            #declaration
            #[doc(hidden)]
            #visibility struct #marker;
            impl #support::WireNode for #marker {
                const SHAPE: #support::ConstantShape = <#wire as #support::WireNode>::SHAPE;
                fn schema() -> #support::SchemaShape { <#wire as #support::WireNode>::schema() }
            }
            impl #support::NonOptionalNode for #marker {}
            impl #support::ResolvedSchema for #name { type Wire = #marker; }
            #field_contract
        },
    )
}

fn record(
    fields: &FieldsNamed,
    name: &Ident,
    support: &TokenStream,
) -> Result<(TokenStream, TokenStream)> {
    for attribute in fields.named.iter().flat_map(|field| &field.attrs) {
        validate_attribute(attribute, false)?;
    }
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
    let wire = type_history_codegen::resolved_schema::record_wire_type(
        schema_fields.iter().map(|field| (&field.name, &field.ty)),
        support,
    )?;
    let schema = type_history_codegen::json_schema_derive::record(name, &schema_fields, support);
    Ok((
        wire,
        quote! {
            #(#field_aliases)*
            #schema
        },
    ))
}

fn validate_attribute(attribute: &Attribute, container: bool) -> Result<()> {
    if attribute.path().is_ident("history") {
        return Err(Error::new_spanned(
            attribute,
            "Schema does not support history attributes; evolve the containing versioned record",
        ));
    }
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
    fn schema_enums_reject_variant_history_and_wire_overrides() {
        for input in [
            parse_quote!(
                enum E {
                    #[history(added_in = v2)]
                    A,
                }
            ),
            parse_quote!(
                enum E {
                    #[serde(alias = "old")]
                    A,
                }
            ),
            parse_quote!(
                enum E {
                    A {
                        #[serde(flatten)]
                        value: u32,
                    },
                }
            ),
            parse_quote!(
                enum E {
                    #[cfg(feature = "other")]
                    A,
                }
            ),
            parse_quote!(
                #[serde(tag = "kind")]
                enum E {
                    A,
                }
            ),
            parse_quote!(
                enum E {
                    A(),
                }
            ),
            parse_quote!(
                enum E {}
            ),
        ] {
            assert!(expand(input).is_err());
        }
        assert!(expand(parse_quote!(
            enum E {
                A,
                B(u32),
                C(u32, String),
                D { value: u32 },
            }
        ))
        .is_ok());
    }

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
