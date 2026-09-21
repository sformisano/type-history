//! Supporting structural schemas for concrete records, tuple structs, and enums.

mod enumeration;
mod tuple;

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{Attribute, Data, DeriveInput, Error, Fields, FieldsNamed, Ident, Result, Visibility};
use type_history_codegen::{
    lint_attributes::{scoped_field_type, with_lints},
    NamedField,
};

pub(super) fn expand(input: DeriveInput) -> Result<TokenStream> {
    super::input::reject_generics(&input.generics)?;
    let allows_closed_container = matches!(
        &input.data,
        Data::Struct(data) if matches!(&data.fields, Fields::Named(_))
    ) || matches!(&input.data, Data::Enum(_));
    for attribute in &input.attrs {
        validate_attribute(attribute, allows_closed_container)?;
    }
    let facade = super::facade();
    let support = quote!(#facade::__private);
    let name = &input.ident;
    let visibility = &input.vis;
    let helper = format_ident!("__TypeHistory{}Schema", name);
    let implementation = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => {
                let (wire, declaration) = record(fields, name, &support)?;
                nominal(name, visibility, &wire, &declaration, &support)
            }
            Fields::Unnamed(fields) => tuple::expand(fields, name, visibility, &helper, &support)?,
            Fields::Unit => {
                return Err(Error::new_spanned(
                    &input,
                    "Schema does not support unit structs",
                ));
            }
        },
        Data::Enum(data) => {
            let (wire, declaration) = enumeration::expand(data, name, &helper, &support)?;
            nominal(name, visibility, &wire, &declaration, &support)
        }
        Data::Union(_) => {
            return Err(Error::new_spanned(
                &input,
                "Schema supports records, nonempty tuple structs, and enums",
            ));
        }
    };
    with_lints(&input.attrs, implementation)
}

fn nominal(
    name: &Ident,
    visibility: &Visibility,
    wire: &TokenStream,
    declaration: &TokenStream,
    support: &TokenStream,
) -> TokenStream {
    let marker = format_ident!("__TypeHistory{}SchemaWire", name);
    let field_contract =
        type_history_codegen::json_schema_derive::field_impl_for_derived(name, support);
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
    }
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
    use syn::{parse_quote, File};

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

    #[test]
    fn schema_accepts_nonempty_tuple_structs_as_valid_rust() {
        for input in [
            parse_quote!(
                struct Identifier(String);
            ),
            parse_quote!(
                struct Position(u32, String);
            ),
        ] {
            let output = expand(input).expect("nonempty tuple struct schema");
            syn::parse2::<File>(output).expect("generated declarations are valid Rust syntax");
        }
    }

    #[test]
    fn schema_rejects_unit_and_empty_tuple_structs() {
        for input in [
            parse_quote!(
                struct Unit;
            ),
            parse_quote!(
                struct Empty();
            ),
        ] {
            assert!(expand(input).is_err());
        }
    }
}
