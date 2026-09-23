//! Complete package declaration inventories under the shared history authority.

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};
use syn::Ident;
use type_history_codegen::{
    history::HistoryPlan,
    ledger::{HistoryLedger, HistoryReadiness},
    model::NamedField,
};

use crate::Result;

/// Whether discovery can require an existing committed authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Admission {
    /// Ordinary builds and lifecycle candidates require valid authority.
    Ordinary,
    /// Explicit init/import discovery remains bounded and cannot certify a build.
    ExplicitSetup,
}

/// Exact authored location supplied by frontend source discovery.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceLocation {
    /// Containing source file, relative to the inventory root or absolute.
    pub file: PathBuf,
    /// One-based authored line.
    pub line: u64,
    /// One-based authored column.
    pub column: u64,
}

/// Optional source information for a declaration and its retained fields.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeclarationSource {
    /// Declaration location used when no authored field matches a difference.
    pub declaration: SourceLocation,
    /// Authored locations keyed by durable field name.
    pub fields: BTreeMap<String, SourceLocation>,
}

/// One source declaration with its current authorized retained inventory.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Declaration<M> {
    /// Frontend's normalized declaration name.
    pub name: String,
    /// Stable name shared by every version of this type.
    pub stable_name: String,
    /// Frontend-owned equality-protected metadata.
    pub metadata: M,
    /// Current inferred positive version.
    pub version: u32,
    /// Exact retained versions; empty only during explicit setup discovery.
    pub retained_versions: Vec<u32>,
    /// Current source readiness under committed authority.
    pub readiness: HistoryReadiness,
    /// Optional exact source locations; custom frontends may omit them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<DeclarationSource>,
}

/// Complete source inventory and tracked paths for one captured package.
#[derive(Clone, Debug)]
pub struct PackageInventory<M> {
    /// Cargo package name.
    pub package: String,
    /// Canonical root of the source being inspected.
    pub root: PathBuf,
    /// Complete logical type inventory.
    pub declarations: Vec<Declaration<M>>,
    /// Authored files, manifests, configuration, ledger, and module candidates.
    pub tracked_paths: Vec<PathBuf>,
    /// Number of outer declarations, including declarations with no fields.
    pub declaration_count: usize,
}

impl<M: Clone + Eq + Serialize + DeserializeOwned> Declaration<M> {
    /// Authorize a bounded history before reconstructing its retained records.
    pub fn resolve(
        name: String,
        stable_name: String,
        metadata: M,
        plan: HistoryPlan,
        marker: &Ident,
        fields: &[NamedField],
        ledger: Option<&HistoryLedger<M>>,
    ) -> Result<Self> {
        let (readiness, retained_versions) = match ledger {
            Some(ledger) => (
                ledger.authorize_history(&stable_name, &metadata, plan)?,
                plan.expand_authorized(marker, fields, &stable_name, ledger, &metadata)?
                    .retained_versions(),
            ),
            None => (HistoryReadiness::Draft, Vec::new()),
        };
        Ok(Self {
            name,
            stable_name,
            metadata,
            version: plan.head,
            retained_versions,
            readiness,
            source: None,
        })
    }

    /// Attach exact locations already found by the frontend source graph.
    pub fn with_source(mut self, source: DeclarationSource) -> Self {
        self.source = Some(source);
        self
    }
}

/// Validate complete stable name inventory, including declarations whose macros disappeared.
pub fn validate<M: Clone + Eq + Serialize + DeserializeOwned>(
    inventory: &PackageInventory<M>,
    ledger: &HistoryLedger<M>,
) -> Result<()> {
    let mut declared = BTreeSet::new();
    for declaration in &inventory.declarations {
        if !declared.insert(declaration.stable_name.as_str()) {
            return Err(format!(
                "duplicate declared stable name: {}",
                declaration.stable_name
            )
            .into());
        }
        let readiness = ledger.authorize_history(
            &declaration.stable_name,
            &declaration.metadata,
            HistoryPlan {
                head: declaration.version,
            },
        )?;
        if readiness != declaration.readiness
            || declaration
                .retained_versions
                .iter()
                .copied()
                .ne(1..=declaration.version)
        {
            return Err(format!(
                "{}: source inventory was not authorized by the current ledger",
                declaration.stable_name
            )
            .into());
        }
    }
    let missing = ledger
        .stable_names()
        .filter(|stable_name| !declared.contains(stable_name))
        .collect::<Vec<_>>();
    if let Some(first) = missing.first() {
        return Err(format!("{first}: committed history exists but no source declaration declares it; restore the declaration rather than deleting retained history ({} missing in total)", missing.len()).into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{validate, Declaration, PackageInventory};
    use std::{collections::BTreeMap, path::PathBuf};
    use syn::{parse_quote, Ident};
    use type_history_codegen::{
        history::HistoryPlan,
        json_schema::JsonSchemaDocument,
        ledger::{HistoryLedger, RecordMetadata, SchemaIdentity, Snapshot},
    };
    use type_history_core::SchemaShape;

    #[test]
    fn declared_huge_range_is_rejected_before_reconstruction() {
        let ledger = HistoryLedger::empty(SchemaIdentity::new("urn:typehistory:schema:"));
        let marker: Ident = parse_quote!(Invoice);
        let declaration = Declaration::resolve(
            "Invoice".to_owned(),
            "billing.invoice.issued".to_owned(),
            RecordMetadata {},
            HistoryPlan { head: u32::MAX },
            &marker,
            &[],
            Some(&ledger),
        );
        assert!(declaration.is_err());
    }

    #[test]
    fn frozen_inventory_survives_erased_source_declarations() {
        let stable_name = "billing.invoice.issued";
        let schema_identity = SchemaIdentity::new("urn:typehistory:schema:");
        let schema = JsonSchemaDocument::from_shape(
            &SchemaShape::Record { fields: Vec::new() },
            &schema_identity.schema_id(stable_name),
        );
        let ledger = HistoryLedger::from_entries(
            BTreeMap::from([(
                stable_name.to_owned(),
                BTreeMap::from([(
                    1,
                    Snapshot {
                        metadata: RecordMetadata {},
                        schema,
                        reset_draft: false,
                    },
                )]),
            )]),
            schema_identity,
        )
        .unwrap();
        let inventory = PackageInventory {
            package: "demo".to_owned(),
            root: PathBuf::from("."),
            declarations: Vec::new(),
            tracked_paths: Vec::new(),
            declaration_count: 0,
        };
        assert!(validate(&inventory, &ledger)
            .unwrap_err()
            .to_string()
            .contains("committed history exists"));
    }
}
