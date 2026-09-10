//! Standalone attribute frontend for the shared history generator.

mod input;
mod schema;

use input::Arguments;
use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as Tokens};
use proc_macro_crate::{crate_name, FoundCrate};
use quote::{format_ident, quote};
use std::path::PathBuf;
use syn::{parse_macro_input, DeriveInput, Error, Ident, ItemStruct, Path, Result};
use typehistory_codegen::{
    generate::{generate_history, generate_versioned, GenerationPaths},
    history::HistoryPlan,
    ledger::{HistoryLedger, RecordMetadata, SchemaIdentity},
};

/// Replace a record inventory with its authorized retained versions.
///
/// Requires `stable_id = "billing.invoice.issued"` and package authority from the
/// Typehistory build hook. The optional `retained_from = vN` requires explicit
/// import authority. Field `#[history(...)]` records describe additions, updates,
/// and removals newest first. The macro supplies record traits and the current
/// alias; additional derives on its input are rejected.
/// The alias provides `into_versioned` and `from_versioned` methods. They keep
/// identity and version metadata with the data and load the current type without
/// requiring application code to select numbered types. The `stable_id` value
/// is frozen with the schemas and must survive Rust type and module renames.
///
/// Ordinary records implement `Clone`, `PartialEq`, and field-wise `Debug`.
/// The optional `redacted` argument requires the `redactable` feature on the
/// runtime, build hook, and CLI. It selects fixed masked `Debug`, structural
/// Redactable traits, and `HasRedactedHistory` instead of `HasHistory`.
/// Field `#[sensitive]`, `#[sensitive(Policy)]`, and `#[not_sensitive]` policies
/// apply across all retained occurrences. See the runtime's `redaction` module.
///
/// Import the public attribute in an initialized Typehistory library:
///
/// ```rust,ignore
/// use typehistory::typehistory;
///
/// #[typehistory(stable_id = "billing.receipt.recorded")]
/// pub struct Receipt {
///     pub amount_cents: u64,
/// }
/// ```
#[proc_macro_attribute]
pub fn typehistory(arguments: TokenStream, item: TokenStream) -> TokenStream {
    match expand(
        parse_macro_input!(arguments as Arguments),
        parse_macro_input!(item as ItemStruct),
    ) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.to_compile_error().into(),
    }
}

/// Describe the structural wire shape of a concrete named record.
///
/// Generates structural schema contracts. Authors supply codecs and business validation.
/// Authors supply matching ordinary traits. Container `#[serde(deny_unknown_fields)]`
/// is supported for a closed Serde record. Other wire-shaping attributes,
/// generics, and non-record declarations are rejected.
#[proc_macro_derive(Schema)]
pub fn schema(item: TokenStream) -> TokenStream {
    match schema::expand(parse_macro_input!(item as DeriveInput)) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.to_compile_error().into(),
    }
}

fn expand(arguments: Arguments, item: ItemStruct) -> Result<Tokens> {
    let (input, identity) = input::parse(arguments, item)?;
    let plan = HistoryPlan::infer(&input.name, input.retained_floor, &input.fields)?;
    required_environment("TYPEHISTORY_AUTHORITY_KIND", "standalone", &input.name)?;
    required_environment(
        "TYPEHISTORY_SCHEMA_ID_PREFIX",
        "urn:typehistory:schema:",
        &input.name,
    )?;
    let ledger_path = std::env::var_os("TYPEHISTORY_LEDGER_PATH")
        .map(PathBuf::from)
        .ok_or_else(|| {
            Error::new_spanned(
                &input.name,
                "call typehistory_build::compile() from build.rs; history authority is mandatory",
            )
        })?;
    if !ledger_path.is_absolute() {
        return Err(Error::new_spanned(
            &input.name,
            "history authority path must be absolute",
        ));
    }
    let ledger = HistoryLedger::<RecordMetadata>::read_file(
        &ledger_path,
        SchemaIdentity::new("urn:typehistory:schema:"),
    )
    .map_err(|error| Error::new_spanned(&input.name, error))?;
    let history = plan.expand_authorized(
        &input.name,
        &input.fields,
        &identity,
        &ledger,
        &RecordMetadata {},
    )?;
    let facade = facade();
    let support: Path = syn::parse_quote!(#facade::__private);
    let (error, history_trait) = if input.redacted {
        (
            quote!(#facade::redaction::RedactedDecodeError),
            quote!(#facade::redaction::HasRedactedHistory),
        )
    } else {
        (quote!(#facade::DecodeError), quote!(#facade::HasHistory))
    };
    let paths = GenerationPaths {
        support,
        error: syn::parse2(error.clone())?,
        helper_prefix: helper_prefix(&input.name),
    };
    let generated = generate_history(&input, &identity, &history, &paths)?;
    let stored_record = generate_versioned(&input, &identity, &history, &paths)?;
    let items = generated.items;
    let current = generated.latest;
    let decoder = generated.decoder;
    let head = history.head();
    let retained = generated
        .versions
        .iter()
        .map(|version| version.number)
        .collect::<Vec<_>>();
    let export = format_ident!("{}_schema_export", paths.helper_prefix);
    let schemas = generated.versions.iter().map(|version| {
        let number = version.number;
        let schema = &version.schema_expression;
        quote!(#facade::__private::serde_json::json!({ "version": #number, "wire": #schema }))
    });
    let ledger_path = ledger_path
        .to_str()
        .ok_or_else(|| Error::new_spanned(&input.name, "history authority path must be UTF-8"))?;
    Ok(quote! {
        const _: &str = include_str!(#ledger_path);
        #items
        #stored_record
        impl #history_trait for #current {
            const TYPE: #facade::TypeIdentity = #facade::TypeIdentity::new(#identity);
            const VERSION: #facade::PayloadVersion = match #facade::PayloadVersion::try_from_raw(#head) {
                ::core::result::Result::Ok(version) => version,
                ::core::result::Result::Err(_) => panic!("non-positive history version"),
            };
            fn history() -> #facade::History<Self, #error> {
                const RETAINED: &[#facade::PayloadVersion] = &[#(match #facade::PayloadVersion::try_from_raw(#retained) {
                    ::core::result::Result::Ok(version) => version,
                    ::core::result::Result::Err(_) => panic!("non-positive history version"),
                }),*];
                #facade::History::new(RETAINED, #decoder)
            }
        }
        #[allow(unexpected_cfgs)]
        #[cfg(all(test, typehistory_schema_export))]
        #[test]
        fn #export() {
            let schema = #facade::__private::serde_json::json!({"identity": #identity, "versions": [#(#schemas),*]});
            ::std::println!("TYPEHISTORY_SCHEMA_EXPORT_V1\t{}", schema);
        }
    })
}

fn required_environment(name: &str, expected: &str, marker: &Ident) -> Result<()> {
    match std::env::var(name) {
        Ok(value) if value == expected => Ok(()),
        _ => Err(Error::new_spanned(
            marker,
            format!("{name} must be `{expected}`; call typehistory_build::compile() from build.rs"),
        )),
    }
}

fn facade() -> Path {
    match crate_name("typehistory") {
        Ok(FoundCrate::Name(name)) => {
            let name = syn::parse_str::<Ident>(&name)
                .unwrap_or_else(|_| Ident::new_raw(&name, Span::call_site()));
            syn::parse_quote!(::#name)
        }
        Ok(FoundCrate::Itself) | Err(_) => syn::parse_quote!(::typehistory),
    }
}

// Encoding each byte keeps distinct authored names distinct, including case differences.
fn helper_prefix(name: &Ident) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut prefix = String::from("__typehistory_");
    for byte in name.to_string().trim_start_matches("r#").bytes() {
        prefix.push(char::from(HEX[usize::from(byte >> 4)]));
        prefix.push(char::from(HEX[usize::from(byte & 15)]));
    }
    prefix
}

#[cfg(test)]
mod tests {
    use super::helper_prefix;
    use syn::{parse_quote, Ident};

    #[test]
    fn case_distinct_record_names_keep_distinct_helper_identifiers() {
        let upper: Ident = parse_quote!(AB);
        let mixed: Ident = parse_quote!(Ab);
        assert_ne!(helper_prefix(&upper), helper_prefix(&mixed));
    }
}
