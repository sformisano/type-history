//! Pure standalone declaration parsing, independent of environment and ledger access.

use crate::model::{NamedField, RecordDerives, RecordInput};
use std::collections::BTreeMap;
use syn::{
    parse::{Parse, ParseStream},
    Attribute, Error, Fields, Generics, Ident, ItemStruct, LitBool, LitStr, Result, Token,
};
use type_history_core::StableName;

/// Stable name and optional native trait choices for the versioned attribute.
pub struct Arguments {
    stable_name: LitStr,
    derives: RecordDerives,
}

impl Parse for Arguments {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let mut stable_name = None;
        let mut derive_debug: Option<LitBool> = None;
        let mut derive_partial_eq: Option<LitBool> = None;
        while !input.is_empty() {
            let key: Ident = input.parse()?;
            input.parse::<Token![=]>()?;
            match key.to_string().as_str() {
                "stable_name" if stable_name.is_none() => stable_name = Some(input.parse()?),
                "derive_debug" if derive_debug.is_none() => derive_debug = Some(input.parse()?),
                "derive_partial_eq" if derive_partial_eq.is_none() => {
                    derive_partial_eq = Some(input.parse()?)
                }
                "stable_name" | "derive_debug" | "derive_partial_eq" => {
                    return Err(Error::new_spanned(key, "duplicate history argument"));
                }
                _ => {
                    return Err(Error::new_spanned(
                        key,
                        "expected `stable_name`, `derive_debug`, or `derive_partial_eq`",
                    ));
                }
            }
            if input.is_empty() {
                break;
            }
            input.parse::<Token![,]>()?;
        }
        let defaults = RecordDerives::default();
        Ok(Self {
            stable_name: stable_name
                .ok_or_else(|| input.error("history requires an explicit `stable_name`"))?,
            derives: RecordDerives {
                debug: derive_debug.map_or(defaults.debug, |value| value.value),
                partial_eq: derive_partial_eq.map_or(defaults.partial_eq, |value| value.value),
            },
        })
    }
}

/// Parse the authored record inventory identically for macro and source discovery.
pub fn parse(arguments: Arguments, item: ItemStruct) -> Result<(RecordInput, String)> {
    reject_generics(&item.generics)?;
    let stable_name = arguments.stable_name.value();
    StableName::try_from(stable_name.clone())
        .map_err(|error| Error::new_spanned(&arguments.stable_name, error))?;
    let Fields::Named(named) = &item.fields else {
        return Err(Error::new_spanned(
            &item,
            "history requires a concrete named record",
        ));
    };
    let attributes = item.attrs.clone();
    for attribute in &attributes {
        if attribute.path().is_ident("history") {
            return Err(Error::new_spanned(
                attribute,
                "history records belong on fields",
            ));
        }
        if attribute.path().is_ident("derive") {
            attribute.parse_args_with(
                syn::punctuated::Punctuated::<syn::Path, Token![,]>::parse_terminated,
            )?;
        } else if !allowed(attribute) && !attribute.path().is_ident("error") {
            return Err(Error::new_spanned(
                attribute,
                "history owns wire contracts; record attributes support documentation, lints, derives, and Redactable helpers",
            ));
        }
    }
    let mut field_visibility = BTreeMap::new();
    let mut fields = Vec::new();
    for field in &named.named {
        for attribute in &field.attrs {
            if !allowed(attribute) {
                return Err(Error::new_spanned(
                    attribute,
                    "record fields support documentation, lints, history, and Redactable policies",
                ));
            }
        }
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
        derives: arguments.derives,
        fields,
        field_visibility,
    };
    Ok((input, stable_name))
}

fn allowed(attribute: &Attribute) -> bool {
    ["doc", "allow", "warn", "deny", "history"]
        .iter()
        .any(|name| attribute.path().is_ident(name))
        || redaction_field_attribute(attribute)
}

pub(crate) fn redaction_field_attribute(attribute: &Attribute) -> bool {
    ["sensitive", "not_sensitive", "redactable"]
        .iter()
        .any(|name| attribute.path().is_ident(name))
}

pub(crate) fn payload_attribute(attribute: &Attribute) -> bool {
    attribute.path().is_ident("derive")
        || attribute.path().is_ident("error")
        || redaction_field_attribute(attribute)
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
    use super::{parse, Arguments};
    use crate::model::RecordDerives;
    use syn::{parse_quote, Attribute, ItemStruct};

    fn argument_error(source: &str) -> String {
        let Err(error) = syn::parse_str::<Arguments>(source) else {
            panic!("invalid history arguments were accepted: {source}");
        };
        error.to_string()
    }

    #[test]
    fn stable_name_is_accepted_and_plain_fields_keep_supported_attributes() {
        let (input, stable_name) = parse(
            parse_quote!(stable_name = "example.record.created"),
            parse_quote! {
                /// Current record.
                #[allow(dead_code)]
                pub struct Record {
                    pub value: String,
                    /// Added label.
                    #[allow(deprecated)]
                    #[history(added_in = v2, backfill_value = String::new())]
                    pub label: String,
                }
            },
        )
        .expect("ordinary record fields");
        assert_eq!(stable_name, "example.record.created");
        assert_eq!(input.derives, RecordDerives::default());
        assert!(input.derives.debug && input.derives.partial_eq);
        assert!(input.fields[0].attributes.is_empty());
        let attributes = &input.fields[1].attributes;
        for name in ["doc", "allow", "history"] {
            assert!(attributes
                .iter()
                .any(|attribute| attribute.path().is_ident(name)));
        }
    }

    #[test]
    fn history_records_belong_on_fields() {
        let result = parse(
            parse_quote!(stable_name = "example.record.created"),
            parse_quote! {
                #[history(added_in = v2, backfill_value = 0)]
                struct Record { value: u32 }
            },
        );
        let Err(error) = result else {
            panic!("struct history was accepted");
        };
        assert_eq!(error.to_string(), "history records belong on fields");
    }

    #[test]
    fn stable_name_is_required() {
        assert!(argument_error("").ends_with("history requires an explicit `stable_name`"));
    }

    #[test]
    fn arguments_require_known_keys_and_values() {
        for (source, diagnostic) in [
            ("stable_name", "expected `=`"),
            (
                "stable_name = \"example.record.created\", unknown = true",
                "expected `stable_name`, `derive_debug`, or `derive_partial_eq`",
            ),
        ] {
            assert_eq!(argument_error(source), diagnostic);
        }
    }

    #[test]
    fn duplicate_stable_name_is_rejected() {
        assert_eq!(
            argument_error(
                "stable_name = \"example.record.created\", stable_name = \"example.record.updated\""
            ),
            "duplicate history argument"
        );
    }

    #[test]
    fn native_trait_options_are_independent_and_order_insensitive() {
        for (source, debug, partial_eq) in [
            (
                "stable_name = \"example.record\", derive_debug = false",
                false,
                true,
            ),
            (
                "derive_partial_eq = false, stable_name = \"example.record\",",
                true,
                false,
            ),
            (
                "derive_debug = false, derive_partial_eq = false, stable_name = \"example.record\"",
                false,
                false,
            ),
            (
                "derive_partial_eq = true, stable_name = \"example.record\", derive_debug = true,",
                true,
                true,
            ),
        ] {
            let arguments = syn::parse_str::<Arguments>(source).unwrap();
            let (input, _) = parse(
                arguments,
                parse_quote!(
                    struct Record {
                        value: u32,
                    }
                ),
            )
            .unwrap();
            assert_eq!(input.derives, RecordDerives { debug, partial_eq });
        }
    }

    #[test]
    fn native_trait_options_reject_duplicates_and_non_boolean_values() {
        for name in ["derive_debug", "derive_partial_eq"] {
            for values in ["false", "true"] {
                let source =
                    format!("stable_name = \"example.record\", {name} = {values}, {name} = false");
                assert_eq!(argument_error(&source), "duplicate history argument");
            }
            for value in ["0", "\"false\"", "Some(false)", "!true"] {
                let source = format!("stable_name = \"example.record\", {name} = {value}");
                assert!(
                    argument_error(&source).contains("boolean literal"),
                    "{source}"
                );
            }
        }
        assert!(
            argument_error("derive_debug = false").contains("requires an explicit `stable_name`")
        );
    }

    #[test]
    fn unknown_field_attributes_are_rejected() {
        let result = parse(
            parse_quote!(stable_name = "example.record.created"),
            parse_quote! {
                struct Record {
                    #[unknown]
                    value: String,
                }
            },
        );
        let Err(error) = result else {
            panic!("unknown field attribute was accepted");
        };
        assert_eq!(
            error.to_string(),
            "record fields support documentation, lints, history, and Redactable policies"
        );
    }

    #[test]
    fn redaction_derives_do_not_admit_wire_overrides() {
        let overrides: [Attribute; 3] = [
            parse_quote!(#[serde(rename = "other")]),
            parse_quote!(#[schemars(with = "String")]),
            parse_quote!(#[cfg(feature = "other")]),
        ];
        for attribute in overrides {
            let mut record: ItemStruct = parse_quote! {
                #[derive(redactable::Sensitive)]
                struct Account {
                    #[sensitive(Secret)]
                    owner: String,
                }
            };
            record
                .fields
                .iter_mut()
                .next()
                .unwrap()
                .attrs
                .push(attribute.clone());
            assert!(parse(parse_quote!(stable_name = "account.data"), record).is_err());
            let record: ItemStruct = parse_quote! {
                #attribute
                #[derive(redactable::Sensitive)]
                struct Account { owner: String }
            };
            assert!(parse(parse_quote!(stable_name = "account.data"), record).is_err());
        }
    }

    #[test]
    fn direct_redaction_keeps_record_and_field_attributes() {
        let (input, _) = parse(
            parse_quote!(stable_name = "account.data", derive_debug = false),
            parse_quote! {
                #[derive(redactable::Sensitive, Eq)]
                struct Account {
                    #[sensitive(Secret)]
                    owner: String,
                    #[not_sensitive]
                    kind: String,
                }
            },
        )
        .unwrap();
        assert!(!input.derives.debug);
        assert!(input.attributes[0].path().is_ident("derive"));
        assert!(input.fields[0].attributes[0].path().is_ident("sensitive"));
        assert!(input.fields[1].attributes[0]
            .path()
            .is_ident("not_sensitive"));
    }

    #[test]
    fn derive_syntax_and_helper_placement_are_checked_without_resolving_macros() {
        let malformed: [Attribute; 2] = [
            parse_quote!(#[derive = "Sensitive"]),
            parse_quote!(#[derive(Sensitive = true)]),
        ];
        for attribute in malformed {
            let record = parse_quote! {
                #attribute
                struct Account { owner: String }
            };
            assert!(parse(parse_quote!(stable_name = "account.data"), record).is_err());
        }
        assert!(parse(
            parse_quote!(stable_name = "account.data"),
            parse_quote! {
                struct Account { #[error("owner")] owner: String }
            }
        )
        .is_err());
        assert!(parse(
            parse_quote!(stable_name = "account.data"),
            parse_quote! {
                #[error("owner {owner}")]
                #[derive(HiddenDerive, Eq)]
                struct Account { #[redactable(recursive)] owner: String }
            }
        )
        .is_ok());
    }
}
