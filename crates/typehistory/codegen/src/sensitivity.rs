//! Shared parsing for history field sensitivity metadata.

use syn::{Attribute, Error, Meta, Path, Result};

/// One deterministic sensitivity declaration for a schema field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SensitivityMetadata {
    /// No explicit marker; inherit the field type's redaction behavior.
    Inherited,
    /// Apply the default scalar redaction policy.
    Sensitive,
    /// Apply one explicit redaction policy.
    SensitivePolicy(Path),
    /// Preserve the field value in redacted output.
    NotSensitive,
}

/// Parse exactly one non-contradictory sensitivity declaration.
pub fn parse_sensitivity_metadata(attributes: &[Attribute]) -> Result<SensitivityMetadata> {
    let sensitive = attributes
        .iter()
        .filter(|attribute| attribute.path().is_ident("sensitive"))
        .collect::<Vec<_>>();
    let not_sensitive = attributes
        .iter()
        .filter(|attribute| attribute.path().is_ident("not_sensitive"))
        .collect::<Vec<_>>();

    if !sensitive.is_empty() && !not_sensitive.is_empty() {
        return Err(Error::new_spanned(
            not_sensitive[0],
            "field sensitivity is contradictory; use exactly one of `sensitive` or `not_sensitive`",
        ));
    }
    if sensitive.len() > 1 {
        return Err(Error::new_spanned(
            sensitive[1],
            "duplicate `sensitive` field declaration",
        ));
    }
    if not_sensitive.len() > 1 {
        return Err(Error::new_spanned(
            not_sensitive[1],
            "duplicate `not_sensitive` field declaration",
        ));
    }

    if let Some(attribute) = not_sensitive.first() {
        if !matches!(attribute.meta, Meta::Path(_)) {
            return Err(Error::new_spanned(
                attribute,
                "`not_sensitive` is a marker and accepts no arguments",
            ));
        }
        return Ok(SensitivityMetadata::NotSensitive);
    }

    let Some(attribute) = sensitive.first() else {
        return Ok(SensitivityMetadata::Inherited);
    };
    match &attribute.meta {
        Meta::Path(_) => Ok(SensitivityMetadata::Sensitive),
        Meta::List(_) => attribute
            .parse_args::<Path>()
            .map(SensitivityMetadata::SensitivePolicy)
            .map_err(|_| {
                Error::new_spanned(
                    attribute,
                    "`sensitive` accepts either no arguments or one redaction policy path",
                )
            }),
        Meta::NameValue(_) => Err(Error::new_spanned(
            attribute,
            "`sensitive` accepts either no arguments or one redaction policy path",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_sensitivity_metadata, SensitivityMetadata};
    use syn::Attribute;

    #[test]
    fn parses_each_supported_form() {
        let none: Vec<Attribute> = Vec::new();
        assert_eq!(
            parse_sensitivity_metadata(&none).expect("inherited"),
            SensitivityMetadata::Inherited
        );
        let scalar = vec![syn::parse_quote!(#[sensitive])];
        assert_eq!(
            parse_sensitivity_metadata(&scalar).expect("scalar"),
            SensitivityMetadata::Sensitive
        );
        let policy = vec![syn::parse_quote!(#[sensitive(crate::SecretPolicy)])];
        assert!(matches!(
            parse_sensitivity_metadata(&policy).expect("policy"),
            SensitivityMetadata::SensitivePolicy(_)
        ));
        let clear = vec![syn::parse_quote!(#[not_sensitive])];
        assert_eq!(
            parse_sensitivity_metadata(&clear).expect("not sensitive"),
            SensitivityMetadata::NotSensitive
        );
    }

    #[test]
    fn rejects_duplicates_contradictions_and_marker_arguments_in_any_order() {
        for attributes in [
            vec![
                syn::parse_quote!(#[sensitive]),
                syn::parse_quote!(#[sensitive(crate::Policy)]),
            ],
            vec![
                syn::parse_quote!(#[not_sensitive]),
                syn::parse_quote!(#[not_sensitive]),
            ],
            vec![
                syn::parse_quote!(#[sensitive]),
                syn::parse_quote!(#[not_sensitive]),
            ],
            vec![
                syn::parse_quote!(#[not_sensitive]),
                syn::parse_quote!(#[sensitive]),
            ],
            vec![syn::parse_quote!(#[not_sensitive(crate::Policy)])],
        ] {
            assert!(parse_sensitivity_metadata(&attributes).is_err());
        }
    }
}
