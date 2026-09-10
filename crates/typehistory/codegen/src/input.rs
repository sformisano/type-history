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
}

impl Parse for Arguments {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let mut id = None;
        let mut floor = None;
        while !input.is_empty() {
            let key: Ident = input.parse()?;
            input.parse::<Token![=]>()?;
            match key.to_string().as_str() {
                "id" if id.is_none() => id = Some(input.parse()?),
                "retained_from" if floor.is_none() => floor = Some(input.parse()?),
                "id" | "retained_from" => {
                    return Err(Error::new_spanned(key, "duplicate history argument"))
                }
                _ => return Err(Error::new_spanned(key, "expected `id` or `retained_from`")),
            }
            if input.is_empty() {
                break;
            }
            input.parse::<Token![,]>()?;
        }
        Ok(Self {
            id: id.ok_or_else(|| input.error("history requires an explicit `id`"))?,
            floor,
        })
    }
}

/// Parse the authored record inventory identically for macro and source discovery.
pub fn parse(arguments: Arguments, item: ItemStruct) -> Result<(RecordInput, String)> {
    reject_generics(&item.generics)?;
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
        if !allowed(attribute, true) {
            return Err(Error::new_spanned(attribute, "history owns record derives and wire contracts; only documentation, lints, and retained history metadata are supported"));
        }
    }
    let retained_floor = parse_retained_floor(&attributes)?;
    attributes.retain(|attribute| !attribute.path().is_ident("history"));
    let mut field_visibility = BTreeMap::new();
    let mut fields = Vec::new();
    for field in &named.named {
        for attribute in &field.attrs {
            if !allowed(attribute, false) {
                return Err(Error::new_spanned(attribute, "record fields support documentation, lints, sensitivity, and exactly `#[history(...)]`"));
            }
        }
        crate::sensitivity::parse_sensitivity_metadata(&field.attrs)?;
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
        name: item.ident,
        visibility: item.vis,
        attributes,
        fields,
        retained_floor,
        field_visibility,
    };
    Ok((input, identity))
}

fn allowed(attribute: &Attribute, container: bool) -> bool {
    ["doc", "allow", "warn", "deny", "history"]
        .iter()
        .any(|name| attribute.path().is_ident(name))
        || (!container
            && ["sensitive", "not_sensitive"]
                .iter()
                .any(|name| attribute.path().is_ident(name)))
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
