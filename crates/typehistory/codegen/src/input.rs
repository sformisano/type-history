//! Pure standalone declaration parsing, independent of environment and ledger access.

use crate::{
    history::parse_retained_floor,
    model::{NamedField, RecordInput},
};
use std::collections::BTreeMap;
use syn::{
    parse::{Parse, ParseStream},
    Attribute, Error, Fields, Generics, Ident, ItemStruct, LitStr, Result, Token,
};
use typehistory_core::TypeIdentity;

/// Explicit identity and optional retained floor for the outer history attribute.
pub struct Arguments {
    id: LitStr,
    floor: Option<Ident>,
    redacted: bool,
}

impl Parse for Arguments {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let mut id = None;
        let mut floor = None;
        let mut redacted = false;
        while !input.is_empty() {
            let key: Ident = input.parse()?;
            if key != "redacted" {
                input.parse::<Token![=]>()?;
            }
            match key.to_string().as_str() {
                "id" if id.is_none() => id = Some(input.parse()?),
                "retained_from" if floor.is_none() => floor = Some(input.parse()?),
                "redacted" if !redacted => redacted = true,
                "id" | "retained_from" | "redacted" => {
                    return Err(Error::new_spanned(key, "duplicate history argument"))
                }
                _ => {
                    return Err(Error::new_spanned(
                        key,
                        "expected `id`, `retained_from`, or `redacted`",
                    ))
                }
            }
            if input.is_empty() {
                break;
            }
            input.parse::<Token![,]>()?;
        }
        Ok(Self {
            id: id.ok_or_else(|| input.error("history requires an explicit `id`"))?,
            floor,
            redacted,
        })
    }
}

/// Parse the authored record inventory identically for macro and source discovery.
pub fn parse(arguments: Arguments, item: ItemStruct) -> Result<(RecordInput, String)> {
    reject_generics(&item.generics)?;
    crate::redaction::require_capability(arguments.redacted, &item.ident)?;
    let identity = arguments.id.value();
    TypeIdentity::try_from(identity.clone())
        .map_err(|error| Error::new_spanned(&arguments.id, error))?;
    let Fields::Named(named) = &item.fields else {
        return Err(Error::new_spanned(
            &item,
            "history requires a concrete named record",
        ));
    };
    let mut attributes = item.attrs.clone();
    if let Some(floor) = arguments.floor {
        attributes.push(syn::parse_quote!(#[history(retained_from = #floor)]));
    }
    for attribute in &attributes {
        if !allowed(attribute) {
            return Err(Error::new_spanned(attribute, "history owns record derives and wire contracts; only documentation, lints, and retained history metadata are supported"));
        }
    }
    let retained_floor = parse_retained_floor(&attributes)?;
    attributes.retain(|attribute| !attribute.path().is_ident("history"));
    let mut field_visibility = BTreeMap::new();
    let mut fields = Vec::new();
    for field in &named.named {
        for attribute in &field.attrs {
            if crate::redaction::is_policy(attribute) {
                if !arguments.redacted {
                    return Err(Error::new_spanned(attribute, "field redaction requires `#[history(..., redacted)]` and the `redactable` feature"));
                }
            } else if !allowed(attribute) {
                return Err(Error::new_spanned(
                    attribute,
                    "record fields support only documentation, lints, and `#[history(...)]`",
                ));
            }
        }
        crate::redaction::parse_policy(&field.attrs)?;
        let name = field.ident.clone().expect("named field");
        field_visibility.insert(
            name.to_string().trim_start_matches("r#").to_owned(),
            field.vis.clone(),
        );
        fields.push(NamedField {
            name,
            ty: field.ty.clone(),
            attributes: field.attrs.clone(),
        });
    }
    let input = RecordInput {
        redacted: arguments.redacted,
        name: item.ident,
        visibility: item.vis,
        attributes,
        fields,
        retained_floor,
        field_visibility,
    };
    Ok((input, identity))
}

fn allowed(attribute: &Attribute) -> bool {
    ["doc", "allow", "warn", "deny", "history"]
        .iter()
        .any(|name| attribute.path().is_ident(name))
}

/// Reject generic records outside the supported structural history subset.
pub fn reject_generics(generics: &Generics) -> Result<()> {
    if !generics.params.is_empty() || generics.where_clause.is_some() {
        return Err(Error::new_spanned(
            generics,
            "history schema records must be concrete; generics are unsupported",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::parse;
    use syn::{parse_quote, Attribute};

    #[test]
    fn plain_fields_keep_documentation_lints_and_history() {
        let (input, identity) = parse(
            parse_quote!(id = "example.record.created"),
            parse_quote! {
                /// Current record.
                #[allow(dead_code)]
                pub struct Record {
                    pub value: String,
                    /// Added label.
                    #[allow(deprecated)]
                    #[history(added_at = v2, backfill_value = String::new())]
                    pub label: String,
                }
            },
        )
        .expect("ordinary record fields");
        assert_eq!(identity, "example.record.created");
        assert!(input.fields[0].attributes.is_empty());
        let attributes = &input.fields[1].attributes;
        for name in ["doc", "allow", "history"] {
            assert!(attributes
                .iter()
                .any(|attribute| attribute.path().is_ident(name)));
        }
    }

    #[test]
    fn redaction_markers_are_unsupported_field_attributes() {
        let attributes: [Attribute; 4] = [
            parse_quote!(#[sensitive]),
            parse_quote!(#[sensitive(crate::Policy)]),
            parse_quote!(#[not_sensitive]),
            parse_quote!(#[cfg_attr(all(), not_sensitive)]),
        ];
        for attribute in attributes {
            let result = parse(
                parse_quote!(id = "example.record.created"),
                parse_quote! {
                    struct Record {
                        #attribute
                        value: String,
                    }
                },
            );
            let Err(error) = result else {
                panic!("unsupported field attribute was accepted: {attribute:?}");
            };
            let expected = if attribute.path().is_ident("cfg_attr") {
                "record fields support only documentation, lints, and `#[history(...)]`"
            } else {
                "field redaction requires `#[history(..., redacted)]` and the `redactable` feature"
            };
            assert_eq!(error.to_string(), expected);
        }
    }
}
