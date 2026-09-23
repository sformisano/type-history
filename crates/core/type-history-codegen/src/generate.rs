//! One generator for retained records, adjacent conversions, and frozen checks.

mod decode;
mod frozen;
mod historical;
#[cfg(test)]
mod tests;
mod transitions;
mod types;
mod versioned;

use crate::{
    history::{AuthorizedHistory, RetainedField},
    lint_attributes::{scoped_field_type_for_owner, with_lints},
    model::RecordInput,
};
use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};
use std::collections::BTreeSet;
use syn::{DeriveInput, Error, Ident, Path, Result, Type};

/// Concrete facade paths used by history generation.
pub struct GenerationPaths {
    /// Facade module reexporting the shared runtime and codec contracts.
    pub support: Path,
    /// Frontend-owned error type. Generated calls pass owned concrete errors.
    ///
    /// Requires `unsupported(stable_name, stored)`, `decode(stable_name, stored, json_error)`,
    /// and generic `upcast(stable_name, stored, from, to, callback_error)` constructors.
    /// The frontend chooses callback bounds and projects before erasing the type.
    pub error: Path,
    /// Prefix distinguishing generated functions within the invocation module.
    pub helper_prefix: String,
}

/// Trait implementations owned by the shared generator or its frontend.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PayloadTraits {
    /// Derive Clone and the native traits selected by [`RecordInput::derives`].
    #[default]
    Native,
    /// The frontend implements traits using [`GeneratedVersion::contract`].
    Frontend,
}

/// Optional frontend integration without changing [`GenerationPaths`].
#[derive(Debug, Clone, Default)]
pub struct GenerationOptions {
    /// Owner of generated payload Clone, PartialEq, and Debug implementations.
    pub payload_traits: PayloadTraits,
    /// Dedicated test-build cfg that permits observing changed frozen shapes.
    ///
    /// Only `all(test, cfg_name)` skips shape assertions. The frontend's build
    /// admission must withhold this cfg for strict and release builds.
    /// `None` keeps unconditional shape assertions for external callers.
    pub observation_cfg: Option<Ident>,
}

/// One generated version and its compiler-resolved schema export.
pub struct GeneratedVersion {
    /// Positive retained payload version.
    pub number: u32,
    /// Concrete generated record name for this version.
    pub name: Ident,
    /// Expression exporting this record's resolved JSON Schema.
    pub schema_expression: TokenStream,
    /// Expression exporting this record's resolved structural wire shape.
    pub shape_expression: TokenStream,
    /// Typed payload input for frontend-owned traits, including scoped field
    /// aliases, field visibility, and original authored field attributes.
    /// These attributes are metadata; they are not emitted on the payload.
    pub contract: DeriveInput,
}

/// Shared generated items and the facts needed for frontend integration.
pub struct GeneratedHistory {
    /// Retained records, current alias, converters, decoder, and frozen assertions.
    pub items: TokenStream,
    /// Current numbered struct on which the frontend implements its history traits.
    pub latest: Ident,
    /// Generated function selecting and decoding an exact stored version.
    pub decoder: Ident,
    /// Retained record names and schema exports in ascending version order.
    pub versions: Vec<GeneratedVersion>,
}

/// Generate an already-authorized history from retained field definitions.
///
/// Expand a [`crate::HistoryPlan`] against the ledger first. The returned items
/// include numbered structs, their current alias, adjacent conversions, a JSON
/// decoder, and compile-time checks of frozen schemas.
pub fn generate_history(
    input: &RecordInput,
    stable_name: &str,
    history: &AuthorizedHistory,
    paths: &GenerationPaths,
) -> Result<GeneratedHistory> {
    generate_history_with_options(
        input,
        stable_name,
        history,
        paths,
        &GenerationOptions::default(),
    )
}

/// Generate an authorized history with explicit frontend integration options.
pub fn generate_history_with_options(
    input: &RecordInput,
    stable_name: &str,
    history: &AuthorizedHistory,
    paths: &GenerationPaths,
    options: &GenerationOptions,
) -> Result<GeneratedHistory> {
    let expected = input
        .fields
        .iter()
        .map(|field| field.name.to_string().trim_start_matches("r#").to_owned())
        .collect::<BTreeSet<_>>();
    let actual = input
        .field_visibility
        .keys()
        .cloned()
        .collect::<BTreeSet<_>>();
    if expected != actual {
        return Err(Error::new_spanned(
            &input.name,
            "field visibility must cover the complete authored history inventory",
        ));
    }
    let context = Context {
        input,
        history,
        paths,
        stable_name,
    };
    let (payloads, versions) = types::generate(&context, options)?;
    let historical = historical::generate(&context, options);
    let conversions = transitions::generate(&context);
    let decoder = decode::generate(&context);
    let assertions = frozen::generate(&context, options);
    Ok(GeneratedHistory {
        items: with_lints(
            &input.attributes,
            quote!(#payloads #conversions #historical #decoder #assertions),
        )?,
        latest: context.payload(history.head()),
        decoder: context.helper("decode_history"),
        versions,
    })
}

/// Generate the standalone versioned value bridge for an authorized history.
///
/// Emit these items alongside [`generate_history`] to add `into_versioned` and
/// `from_versioned` and support for the Serde wrapper. The support facade must
/// provide `VersionedHistory` and `Versioned`.
pub fn generate_versioned(
    input: &RecordInput,
    stable_name: &str,
    history: &AuthorizedHistory,
    paths: &GenerationPaths,
) -> Result<TokenStream> {
    let context = Context {
        input,
        history,
        paths,
        stable_name,
    };
    with_lints(&input.attributes, versioned::generate(&context))
}

struct Context<'a> {
    input: &'a RecordInput,
    history: &'a AuthorizedHistory,
    paths: &'a GenerationPaths,
    stable_name: &'a str,
}

impl Context<'_> {
    fn field(&self, version: u32, name: &Ident) -> (usize, &RetainedField) {
        let retained = self
            .history
            .versions()
            .iter()
            .find(|retained| retained.version == version)
            .expect("generated retained version");
        retained
            .fields
            .iter()
            .enumerate()
            .find(|(_, field)| &field.name == name)
            .expect("generated retained field")
    }

    fn field_type(&self, version: u32, name: &Ident) -> Type {
        let (index, field) = self.field(version, name);
        scoped_field_type_for_owner(
            &self.payload(version),
            &self.input.name,
            index,
            &field.ty,
            &field.attributes,
        )
        .0
    }

    fn payload(&self, version: u32) -> Ident {
        format_ident!(
            "{}V{}",
            self.input.name,
            version,
            span = self.input.name.span()
        )
    }

    fn helper(&self, suffix: &str) -> Ident {
        format_ident!(
            "{}_{suffix}",
            self.paths.helper_prefix,
            span = Span::mixed_site()
        )
    }

    fn version(&self, value: u32) -> TokenStream {
        let support = &self.paths.support;
        let version = format_ident!("__type_history_version", span = Span::mixed_site());
        quote! {
            match #support::PayloadVersion::try_from_raw(#value) {
                ::core::result::Result::Ok(#version) => #version,
                ::core::result::Result::Err(_) => ::core::panic!("generated history has a non-positive retained version"),
            }
        }
    }

    fn stable_name(&self) -> TokenStream {
        let support = &self.paths.support;
        let stable_name = self.stable_name;
        quote!(#support::StableName::new(#stable_name))
    }
}
