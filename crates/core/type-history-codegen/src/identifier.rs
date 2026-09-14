//! Canonical names shared by generated histories and their frontends.

use proc_macro2::Span;
use syn::Ident;

/// Unique lowercase helper name that preserves case and Unicode distinctions.
pub(crate) fn helper_name(purpose: &str, name: &Ident) -> Ident {
    let mut encoded = format!("__type_history_{purpose}_");
    for byte in name.to_string().trim_start_matches("r#").bytes() {
        encoded.push_str(&format!("{byte:02x}"));
    }
    Ident::new(&encoded, Span::mixed_site())
}

/// Normalize one supported Rust identifier to the durable ASCII snake-case form.
pub fn normalize_rust_identifier(value: &str) -> Result<String, IdentifierError> {
    let value = value.strip_prefix("r#").unwrap_or(value);
    if value.is_empty()
        || !value.is_ascii()
        || !value
            .bytes()
            .all(|byte| byte == b'_' || byte.is_ascii_alphanumeric())
        || !value.bytes().any(|byte| byte.is_ascii_alphanumeric())
    {
        return Err(IdentifierError::InvalidRustIdentifier {
            value: value.to_owned(),
        });
    }

    let bytes = value.as_bytes();
    let mut output = String::with_capacity(bytes.len() + 4);
    for (index, byte) in bytes.iter().copied().enumerate() {
        if byte == b'_' {
            if !output.is_empty() && !output.ends_with('_') {
                output.push('_');
            }
            continue;
        }
        let previous = index.checked_sub(1).and_then(|i| bytes.get(i)).copied();
        let next = bytes.get(index + 1).copied();
        if byte.is_ascii_uppercase()
            && !output.is_empty()
            && !output.ends_with('_')
            && (previous.is_some_and(|p| p.is_ascii_lowercase() || p.is_ascii_digit())
                || (previous.is_some_and(|p| p.is_ascii_uppercase())
                    && next.is_some_and(|n| n.is_ascii_lowercase())))
        {
            output.push('_');
        }
        output.push(char::from(byte.to_ascii_lowercase()));
    }
    let output = output.trim_matches('_').to_owned();
    if !valid_segment(&output) {
        return Err(IdentifierError::InvalidRustIdentifier {
            value: value.to_owned(),
        });
    }
    Ok(output)
}

fn valid_segment(value: &str) -> bool {
    let mut bytes = value.bytes();
    matches!(bytes.next(), Some(b'a'..=b'z'))
        && bytes.all(|byte| matches!(byte, b'a'..=b'z' | b'0'..=b'9' | b'_'))
}

/// A Rust identifier cannot form the supported canonical name.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum IdentifierError {
    /// Empty, non-ASCII or noncanonical identifier.
    #[error("unsupported Rust identifier `{value}`")]
    InvalidRustIdentifier {
        /// Rejected name.
        value: String,
    },
}
