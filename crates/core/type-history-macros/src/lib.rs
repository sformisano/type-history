//! Macros for declaring histories and describing supporting field types.
//!
//! Most applications import [`versioned`] and [`Schema`] from `type-history`.
//! This crate connects those macros to the shared generator and the package's
//! frozen schema ledger.

mod input;
mod schema;
mod scope;

use input::Arguments;
use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as Tokens};
use proc_macro_crate::{crate_name, FoundCrate};
use quote::{format_ident, quote};
use std::{error::Error as StandardError, path::PathBuf, result::Result as StandardResult};
use syn::{parse_macro_input, DeriveInput, Error, Ident, ItemStruct, Path, Result};
use type_history_codegen::{
    admission::{self, Admission, Invocation},
    generate::{generate_history, generate_versioned, GenerationPaths},
    history::HistoryPlan,
    ledger::{HistoryLedger, RecordMetadata, SchemaIdentity},
};

/// Generate numbered structs and an alias for the current version.
///
/// A new `ReceiptCreated` history generates `ReceiptCreatedV1` and the alias
/// `ReceiptCreated`. At V2, that alias becomes `pub type ReceiptCreated = ReceiptCreatedV2;`.
/// Several field changes can share the same version number.
///
/// Initialize the package's ledger and call `type_history_build::compile()` from
/// `build.rs` before declaring a history. Then give the type a stable name:
/// Declare histories directly at module scope, including ordinary nested modules.
/// Undiscovered macro-generated, included, and function-local declarations fail
/// explicitly in every build profile. Each matching expansion still receives
/// the current ledger, frozen-shape, and strict checks.
/// Matching includes the discovered library module and record type; copying a
/// declaration's source position does not authorize a second expansion.
///
/// ```rust,ignore
/// use type_history::versioned;
///
/// #[versioned(stable_name = "shop.receipt.created")]
/// pub struct ReceiptCreated {
///     pub amount_cents: u64,
/// }
/// ```
///
/// Every history starts at V1. When fields change, write `#[history(...)]`
/// attributes for additions, updates, and removals, newest first. Additions and
/// updates need an explicit backfill; updates also need `previous_type` to
/// reconstruct the earlier field type.
///
/// Application code uses the alias's `into_versioned` and `from_versioned`
/// methods to store values with metadata and convert historical values to the
/// current type. Conversions only run forward. Builds reject changes to the
/// frozen stable name even when the Rust type or its module is renamed.
///
/// The macro supplies the generated structs' traits; additional derives on the
/// declaration are rejected. Generated records always implement `Clone`.
/// Standard `PartialEq` and field-wise `Debug` are enabled by default and keep
/// each field's native behavior. Set `derive_partial_eq = false` or
/// `derive_debug = false` to omit the respective trait on every retained version.
/// These choices do not change schemas, stored values, or version numbers.
///
/// Tuples with 13 through 16 elements have supported schemas and codecs but lack
/// native `Debug` and `PartialEq`. Disable both derives when such tuples occur
/// anywhere in a history's retained field types, including aliases and containers:
///
/// ```rust,ignore
/// #[versioned(stable_name = "example.long_tuple", derive_debug = false, derive_partial_eq = false)]
/// pub struct Record {
///     pub value: Tuple16Alias,
/// }
/// ```
#[proc_macro_attribute]
pub fn versioned(arguments: TokenStream, item: TokenStream) -> TokenStream {
    match expand(
        parse_macro_input!(arguments as Arguments),
        parse_macro_input!(item as ItemStruct),
    ) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.to_compile_error().into(),
    }
}

/// Describe how a supporting record, nonempty tuple struct, or enum is serialized.
///
/// For example, a receipt's address record needs a schema so changes to its fields
/// can be checked against the receipt's frozen versions. This derive supplies that
/// schema. Supply matching Serde implementations, ordinary field traits, and any
/// business validation separately.
///
/// Container `#[serde(deny_unknown_fields)]` is supported for named records and
/// externally tagged enums. Enum variants
/// can be unit, newtype, tuple with at least two fields, or named-field variants.
/// A one-field tuple struct has transparent newtype value encoding; multiple
/// fields retain their positional order. This derive adds schema traits only.
/// Other wire-shaping attributes, generics, empty enums, zero-field tuple variants,
/// empty tuple structs, and unit structs are rejected. Enums and tuple structs
/// evolve as fields of their containing versioned structs, without their own
/// history attributes.
#[proc_macro_derive(Schema)]
pub fn schema(item: TokenStream) -> TokenStream {
    match schema::expand(parse_macro_input!(item as DeriveInput)) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.to_compile_error().into(),
    }
}

fn expand(arguments: Arguments, item: ItemStruct) -> Result<Tokens> {
    let (input, stable_name) = input::parse(arguments, item)?;
    let plan = HistoryPlan::infer(&input.name, &input.fields)?;
    required_environment("TYPE_HISTORY_AUTHORITY_KIND", "standalone", &input.name)?;
    required_environment(
        "TYPE_HISTORY_SCHEMA_ID_PREFIX",
        "urn:typehistory:schema:",
        &input.name,
    )?;
    let ledger_path = std::env::var_os("TYPE_HISTORY_LEDGER_PATH")
        .map(PathBuf::from)
        .ok_or_else(|| {
            Error::new_spanned(
                &input.name,
                "call type_history_build::compile() from build.rs; history authority is mandatory",
            )
        })?;
    if !ledger_path.is_absolute() {
        return Err(Error::new_spanned(
            &input.name,
            "history authority path must be absolute",
        ));
    }
    let native = input.name.span().unwrap();
    let source_path = native.local_file().ok_or_else(|| {
        Error::new_spanned(
            &input.name,
            "unsupported history declaration: native source position is required",
        )
    })?;
    let invocation = Invocation {
        path: source_path
            .canonicalize()
            .map_err(|error| Error::new_spanned(&input.name, error))?,
        line: native.line(),
        column: native.column() - 1,
        stable_name: stable_name.clone(),
    };
    let checked = (|| -> StandardResult<_, Box<dyn StandardError>> {
        let path = std::env::var_os(admission::ENV)
            .ok_or("missing history admission; call type_history_build::compile() from build.rs")?;
        let path = PathBuf::from(path);
        Ok((Admission::read(&path, &invocation)?, path))
    })()
    .map_err(|error| Error::new_spanned(&input.name, format!("history admission: {error}")))?;
    let ((strict, declaration), admission_path) = checked;
    if std::env::var_os("CARGO_BIN_NAME").is_some()
        || std::env::var("CARGO_CRATE_NAME").ok().as_deref()
            != declaration.module_path.first().map(String::as_str)
    {
        return Err(Error::new_spanned(
            &input.name,
            "unsupported history declaration: declare histories in the package library",
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
        &stable_name,
        &ledger,
        &RecordMetadata {},
    )?;
    if strict && !history.frozen_shapes().contains_key(&history.head()) {
        return Err(Error::new_spanned(
            &input.name,
            "history is a draft; freeze it before a strict or release build",
        ));
    }
    let facade = facade();
    let support: Path = syn::parse_quote!(#facade::__private);
    let error = quote!(#facade::DecodeError);
    let history_trait = quote!(#facade::HasHistory);
    let paths = GenerationPaths {
        support,
        error: syn::parse2(error.clone())?,
        helper_prefix: helper_prefix(&input.name),
    };
    let generated = generate_history(&input, &stable_name, &history, &paths)?;
    let stored_record = generate_versioned(&input, &stable_name, &history, &paths)?;
    let items = generated.items;
    let current = generated.latest;
    let scope = scope::check(&declaration.module_path, &declaration.rust_name, &current)?;
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
    let admission_path = admission_path
        .to_str()
        .ok_or_else(|| Error::new_spanned(&input.name, "history admission path must be UTF-8"))?;
    let version_binding = format_ident!("__type_history_version", span = Span::mixed_site());
    Ok(quote! {
        const _: &::core::primitive::str = ::core::include_str!(#ledger_path);
        const _: &::core::primitive::str = ::core::include_str!(#admission_path);
        #scope
        #items
        #stored_record
        impl #history_trait for #current {
            const STABLE_NAME: #facade::StableName = #facade::StableName::new(#stable_name);
            const VERSION: #facade::PayloadVersion = match #facade::PayloadVersion::try_from_raw(#head) {
                ::core::result::Result::Ok(#version_binding) => #version_binding,
                ::core::result::Result::Err(_) => ::core::panic!("non-positive history version"),
            };
            fn history() -> #facade::History<Self, #error> {
                const RETAINED: &[#facade::PayloadVersion] = &[#(match #facade::PayloadVersion::try_from_raw(#retained) {
                    ::core::result::Result::Ok(#version_binding) => #version_binding,
                    ::core::result::Result::Err(_) => ::core::panic!("non-positive history version"),
                }),*];
                #facade::History::new(RETAINED, #decoder)
            }
        }
        #[allow(unexpected_cfgs)]
        #[cfg(all(test, type_history_schema_export))]
        #[test]
        fn #export() {
            ::std::println!("TYPE_HISTORY_SCHEMA_EXPORT_V1\t{}", #facade::__private::serde_json::json!({"stable_name": #stable_name, "versions": [#(#schemas),*]}));
        }
    })
}

fn required_environment(name: &str, expected: &str, marker: &Ident) -> Result<()> {
    match std::env::var(name) {
        Ok(value) if value == expected => Ok(()),
        _ => Err(Error::new_spanned(
            marker,
            format!(
                "{name} must be `{expected}`; call type_history_build::compile() from build.rs"
            ),
        )),
    }
}

fn facade() -> Path {
    match crate_name("type-history") {
        Ok(FoundCrate::Name(name)) => {
            let name = syn::parse_str::<Ident>(&name)
                .unwrap_or_else(|_| Ident::new_raw(&name, Span::call_site()));
            syn::parse_quote!(::#name)
        }
        Ok(FoundCrate::Itself) | Err(_) => syn::parse_quote!(::type_history),
    }
}

// Encoding each byte keeps distinct authored names distinct, including case differences.
fn helper_prefix(name: &Ident) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut prefix = String::from("__type_history_");
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
