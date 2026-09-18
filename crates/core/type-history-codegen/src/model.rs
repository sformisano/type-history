//! Authored record input shared by the macro and source discovery.
use std::collections::BTreeMap;
use syn::{Attribute, Ident, Type, Visibility};

/// One authored field and its history, documentation, and lint attributes.
#[derive(Clone)]
pub struct NamedField {
    /// Authored field name.
    pub name: Ident,
    /// Current or final historical Rust type.
    pub ty: Type,
    /// Attributes before history consumption.
    pub attributes: Vec<Attribute>,
}

/// Native trait derives applied uniformly to every retained payload.
///
/// These generation options do not affect schemas or stored values. Both
/// options default to true, preserving the usual native field semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordDerives {
    /// Generate the standard field-wise `Debug` implementation.
    pub debug: bool,
    /// Generate the standard field-wise `PartialEq` implementation.
    pub partial_eq: bool,
}

impl Default for RecordDerives {
    fn default() -> Self {
        Self {
            debug: true,
            partial_eq: true,
        }
    }
}

/// Complete inventory used to infer and generate one record history.
pub struct RecordInput {
    /// Name of the current alias.
    pub name: Ident,
    /// Visibility of concrete versions and the current alias.
    pub visibility: Visibility,
    /// Shape-neutral record attributes.
    pub attributes: Vec<Attribute>,
    /// Native trait configuration for every retained version. Use
    /// `RecordDerives::default()` to retain the standard generated traits.
    pub derives: RecordDerives,
    /// Every retained field lifetime, including removed fields.
    pub fields: Vec<NamedField>,
    /// Visibility for every canonical authored field name.
    pub field_visibility: BTreeMap<String, Visibility>,
}
