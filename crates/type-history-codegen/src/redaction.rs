//! Explicit record redaction, independent of persisted wire schemas.

use crate::model::{NamedField, RecordInput};
use proc_macro2::{Span, TokenStream, TokenTree};
use quote::{quote, quote_spanned};
use std::collections::BTreeSet;
use syn::{spanned::Spanned, Attribute, Error, Ident, Meta, Path, Result};

/// A field policy applies to every retained occurrence of that field.
pub enum FieldPolicy {
    /// Require the field type's structural `Redactable` contract.
    Inherited,
    /// Apply the release's default `Secret` policy.
    Secret,
    /// Apply an authored Redactable policy.
    Policy(Path),
    /// Explicitly preserve the field in structured redacted output.
    Public,
}

/// Whether an attribute declares a field redaction policy.
pub fn is_policy(attribute: &Attribute) -> bool {
    attribute.path().is_ident("sensitive") || attribute.path().is_ident("not_sensitive")
}

/// Parse one policy, rejecting contradictory, duplicate, and malformed declarations.
pub fn parse_policy(attributes: &[Attribute]) -> Result<FieldPolicy> {
    let mut policies = attributes.iter().filter(|attribute| is_policy(attribute));
    let Some(attribute) = policies.next() else {
        return Ok(FieldPolicy::Inherited);
    };
    if let Some(duplicate) = policies.next() {
        return Err(Error::new_spanned(
            duplicate,
            "use exactly one field redaction policy",
        ));
    }
    match (&attribute.meta, attribute.path().is_ident("not_sensitive")) {
        (Meta::Path(_), true) => Ok(FieldPolicy::Public),
        (Meta::Path(_), false) => Ok(FieldPolicy::Secret),
        (Meta::List(_), false) => attribute.parse_args::<Path>().map(FieldPolicy::Policy),
        _ => Err(Error::new_spanned(
            attribute,
            "use `#[sensitive]`, `#[sensitive(Policy)]`, or `#[not_sensitive]`",
        )),
    }
}

/// Reject an explicit request when this generator lacks the integration feature.
pub fn require_capability(redacted: bool, marker: &Ident) -> Result<()> {
    if redacted && !cfg!(feature = "redactable") {
        return Err(Error::new_spanned(marker, "redacted histories require the `redactable` feature on type-history, type-history-build, and cargo-type-history"));
    }
    Ok(())
}

/// Validate explicit selection for standalone and downstream macro frontends.
pub fn validate(input: &RecordInput) -> Result<()> {
    require_capability(input.redacted, &input.name)?;
    for field in &input.fields {
        if !input.redacted && field.attributes.iter().any(is_policy) {
            return Err(Error::new_spanned(
                &field.name,
                "field redaction requires an explicitly redacted history",
            ));
        }
        parse_policy(&field.attributes)?;
    }
    Ok(())
}

/// Generate only redaction traits for a concrete named record.
///
/// `support` must reexport Redactable 0.12.0 as `redactable`. The caller supplies
/// `Clone` and `Serialize`. This function owns `Debug`, `RedactableWithMapper`,
/// `Redactable`, and `ToRedacted`; do not derive those traits again.
/// Inherited fields require `Redactable`. Debug never reads a field, including
/// inherited nested values, and therefore needs no field `Debug` bounds.
pub fn record(name: &Ident, fields: &[NamedField], support: &TokenStream) -> Result<TokenStream> {
    require_capability(true, name)?;
    let runtime = quote!(#support::redactable);
    let debug_text = format!("{name}(<redacted>)");
    let mut identifiers = BTreeSet::new();
    collect_identifiers(runtime.clone(), &mut identifiers);
    for field in fields {
        let ty = &field.ty;
        let attributes = &field.attributes;
        collect_identifiers(quote!(#ty #(#attributes)*), &mut identifiers);
    }
    let mut generic = "__TypeHistoryMapper".to_owned();
    while identifiers.contains(&generic) {
        generic.push('X');
    }
    let mapper_type = Ident::new(&generic, Span::mixed_site());
    let mut assignments = Vec::new();
    for field in fields {
        let member = &field.name;
        let ty = &field.ty;
        let value = match parse_policy(&field.attributes)? {
            FieldPolicy::Inherited => quote_spanned!(ty.span()=> {
                fn require_declared<T: #runtime::Redactable>(value: T, mapper: &impl #runtime::RedactableMapper) -> T {
                    #runtime::RedactableWithMapper::redact_with(value, mapper)
                }
                require_declared(self.#member, __type_history_mapper)
            }),
            FieldPolicy::Secret => quote_spanned!(ty.span()=>
                <#ty as #runtime::__private::PolicyField<#runtime::Secret>>::apply_field(self.#member, __type_history_mapper)),
            FieldPolicy::Policy(policy) => quote_spanned!(ty.span()=>
                <#ty as #runtime::__private::PolicyField<#policy>>::apply_field(self.#member, __type_history_mapper)),
            FieldPolicy::Public => quote!(self.#member),
        };
        assignments.push(quote!(#member: #value));
    }
    Ok(quote! {
        impl #runtime::RedactableWithMapper for #name {
            fn redact_with<#mapper_type: #runtime::RedactableMapper>(self, __type_history_mapper: &#mapper_type) -> Self {
                Self { #(#assignments),* }
            }
        }
        impl #runtime::Redactable for #name {}
        impl #runtime::ToRedacted for #name {
            fn to_redacted(&self) -> #runtime::RedactedValue {
                #runtime::__private::generated_redacted_json(self)
            }
        }
        impl ::core::fmt::Debug for #name {
            fn fmt(&self, formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                formatter.write_str(#debug_text)
            }
        }
    })
}

fn collect_identifiers(tokens: TokenStream, output: &mut BTreeSet<String>) {
    for token in tokens {
        match token {
            TokenTree::Ident(ident) => {
                output.insert(ident.to_string().trim_start_matches("r#").to_owned());
            }
            TokenTree::Group(group) => collect_identifiers(group.stream(), output),
            _ => {}
        }
    }
}
