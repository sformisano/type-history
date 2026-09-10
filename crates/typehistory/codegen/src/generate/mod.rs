//! One generator for retained records, adjacent conversions, and frozen checks.

mod decode;
mod frozen;
mod transitions;
mod types;

use crate::{
    history::{AuthorizedHistory, RetainedField},
    lint_attributes::{scoped_field_type, with_lints},
    model::RecordInput,
};
use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};
use std::collections::BTreeSet;
use syn::{Error, Ident, Path, Result, Type};

/// Concrete facade paths used by the two macro frontends.
pub struct GenerationPaths {
    /// Facade module reexporting the shared runtime and codec contracts.
    pub support: Path,
    /// Frontend-owned error type. Generated calls pass owned concrete errors.
    ///
    /// Requires `unsupported(identity, stored)`, `decode(identity, stored, json_error)`,
    /// and generic `upcast(identity, stored, from, to, callback_error)` constructors.
    /// The frontend chooses callback bounds and projects before erasing the type.
    /// Axis can select its `ToRedactedError` contract here without requiring
    /// Redactable's distinct `ToRedacted` contract on those errors.
    pub error: Path,
    /// Prefix distinguishing generated functions within the invocation module.
    pub helper_prefix: String,
}

/// One generated version and its compiler-resolved schema export.
pub struct GeneratedVersion {
    /// Positive retained payload version.
    pub number: u32,
    /// Concrete generated record name for this version.
    pub name: Ident,
    /// Expression exporting this record's resolved JSON Schema.
    pub schema_expression: TokenStream,
}

/// Shared generated items and the facts needed for frontend integration.
pub struct GeneratedHistory {
    /// Retained records, current alias, converters, decoder, and frozen assertions.
    pub items: TokenStream,
    /// Concrete current record that receives frontend live traits.
    pub latest: Ident,
    /// Generated function selecting and decoding an exact stored version.
    pub decoder: Ident,
    /// Retained record names and schema exports in ascending version order.
    pub versions: Vec<GeneratedVersion>,
}

/// Generate an already-authorized history without interpreting aggregate roles.
pub fn generate_history(
    input: &RecordInput,
    identity: &str,
    history: &AuthorizedHistory,
    paths: &GenerationPaths,
) -> Result<GeneratedHistory> {
    crate::redaction::validate(input)?;
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
        identity,
    };
    let (payloads, versions) = types::generate(&context)?;
    let conversions = transitions::generate(&context);
    let decoder = decode::generate(&context);
    let assertions = frozen::generate(&context);
    Ok(GeneratedHistory {
        items: with_lints(
            &input.attributes,
            quote!(#payloads #conversions #decoder #assertions),
        )?,
        latest: context.payload(history.head()),
        decoder: context.helper("decode_history"),
        versions,
    })
}

struct Context<'a> {
    input: &'a RecordInput,
    history: &'a AuthorizedHistory,
    paths: &'a GenerationPaths,
    identity: &'a str,
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
        scoped_field_type(&self.payload(version), index, &field.ty, &field.attributes).0
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
        let version = format_ident!("__typehistory_version", span = Span::mixed_site());
        quote! {
            match #support::PayloadVersion::try_from_raw(#value) {
                ::core::result::Result::Ok(#version) => #version,
                ::core::result::Result::Err(_) => panic!("generated history has a non-positive retained version"),
            }
        }
    }

    fn identity(&self) -> TokenStream {
        let support = &self.paths.support;
        let identity = self.identity;
        quote!(#support::TypeIdentity::new(#identity))
    }
}
