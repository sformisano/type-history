//! Complete package declaration inventories under the shared history authority.

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
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

/// One source declaration with its current authorized retained inventory.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Declaration<M> {
    /// Frontend's normalized declaration name.
    pub name: String,
    /// Stable logical identity.
    pub identity: String,
    /// Frontend-owned equality-protected metadata.
    pub metadata: M,
    /// Current inferred positive version.
    pub version: u32,
    /// Explicit or default retained floor.
    pub retained_from: u32,
    /// Exact retained versions; empty only during explicit setup discovery.
    pub retained_versions: Vec<u32>,
    /// Current source readiness under committed authority.
    pub readiness: HistoryReadiness,
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
        identity: String,
        metadata: M,
        plan: HistoryPlan,
        marker: &Ident,
        fields: &[NamedField],
        ledger: Option<&HistoryLedger<M>>,
    ) -> Result<Self> {
        let (readiness, retained_versions) = match ledger {
            Some(ledger) => (
                ledger.authorize_history(&identity, &metadata, plan)?,
                plan.expand_authorized(marker, fields, &identity, ledger, &metadata)?
                    .retained_versions(),
            ),
            None => (HistoryReadiness::Draft, Vec::new()),
        };
        Ok(Self {
            name,
            identity,
            metadata,
            version: plan.head,
            retained_from: plan.floor,
            retained_versions,
            readiness,
        })
    }
}

/// Validate complete identity inventory, including declarations whose macros disappeared.
pub fn validate<M: Clone + Eq + Serialize + DeserializeOwned>(
    inventory: &PackageInventory<M>,
    ledger: &HistoryLedger<M>,
) -> Result<()> {
    let mut declared = BTreeSet::new();
    for declaration in &inventory.declarations {
        if !declared.insert(declaration.identity.as_str()) {
            return Err(format!("duplicate declared identity: {}", declaration.identity).into());
        }
        let readiness = ledger.authorize_history(
            &declaration.identity,
            &declaration.metadata,
            HistoryPlan {
                floor: declaration.retained_from,
                head: declaration.version,
            },
        )?;
        if readiness != declaration.readiness
            || declaration
                .retained_versions
                .iter()
                .copied()
                .ne(declaration.retained_from..=declaration.version)
        {
            return Err(format!(
                "{}: source inventory was not authorized by the current ledger",
                declaration.identity
            )
            .into());
        }
    }
    let missing = ledger
        .identities()
        .filter(|identity| !declared.contains(identity))
        .collect::<Vec<_>>();
    if let Some(first) = missing.first() {
        return Err(format!("{first}: committed history exists but no source declaration declares it; restore the declaration rather than deleting retained history ({} missing in total)", missing.len()).into());
    }
    Ok(())
}

/// Reject two active authority configurations in one package.
pub fn reject_conflicting_setup(root: &Path) -> Result<()> {
    if root.join("axis.toml").exists() && root.join("type-history.toml").exists() {
        return Err(
            "one package cannot select both axis.toml and type-history.toml history authorities"
                .into(),
        );
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
            HistoryPlan {
                floor: 1,
                head: u32::MAX,
            },
            &marker,
            &[],
            Some(&ledger),
        );
        assert!(declaration.is_err());
    }

    #[test]
    fn frozen_inventory_survives_erased_source_declarations() {
        let identity = "billing.invoice.issued";
        let schema_identity = SchemaIdentity::new("urn:typehistory:schema:");
        let schema = JsonSchemaDocument::from_shape(
            &SchemaShape::Record { fields: Vec::new() },
            &schema_identity.schema_id(identity),
        );
        let ledger = HistoryLedger::from_entries(
            BTreeMap::from([(
                identity.to_owned(),
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
