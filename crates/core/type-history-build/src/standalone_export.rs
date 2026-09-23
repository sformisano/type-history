//! Validate generated standalone export rows against complete source inventory.
use crate::contract::STANDALONE;
use crate::inventory::PackageInventory;
use crate::Result;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use type_history_codegen::json_schema::JsonSchemaDocument;
use type_history_codegen::ledger::{HistoryLedger, RecordMetadata, SchemaIdentity, Snapshot};
use type_history_core::SchemaShape;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExportRecord {
    stable_name: String,
    versions: Vec<ExportVersion>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExportVersion {
    version: u32,
    shape: SchemaShape,
}

/// Decode all generated standalone tests and reject missing, extra, or duplicate rows.
pub fn decode(
    inventory: &PackageInventory<RecordMetadata>,
    output: &[u8],
) -> Result<HistoryLedger<RecordMetadata>> {
    let schema_identity = SchemaIdentity::new(STANDALONE.schema_id_prefix);
    let mut observed = BTreeMap::new();
    let mut seen = BTreeSet::new();
    for line in std::str::from_utf8(output)?.lines() {
        let Some((_, json)) = line.split_once(STANDALONE.export_marker) else {
            continue;
        };
        let record: ExportRecord = serde_json::from_str(json)?;
        let declaration = inventory
            .declarations
            .iter()
            .find(|item| item.stable_name == record.stable_name)
            .ok_or("unexpected exported history")?;
        if !seen.insert(record.stable_name.clone()) {
            return Err("duplicate exported history".into());
        }
        let mut versions = BTreeMap::new();
        for version in record.versions {
            if !(1..=declaration.version).contains(&version.version) {
                return Err(format!(
                    "exported version differs from declared history: {} V{}",
                    declaration.stable_name, version.version
                )
                .into());
            }
            let schema = JsonSchemaDocument::try_from_shape(
                &version.shape,
                &schema_identity.schema_id(&record.stable_name),
            )?;
            if versions
                .insert(
                    version.version,
                    Snapshot {
                        metadata: declaration.metadata,
                        schema,
                        reset_draft: false,
                    },
                )
                .is_some()
            {
                return Err("duplicate exported history version".into());
            }
        }
        let expected = u64::from(declaration.version);
        if versions.len() as u64 != expected {
            return Err(format!(
                "incomplete exported retained history for {}",
                declaration.stable_name
            )
            .into());
        }
        observed.insert(record.stable_name, versions);
    }
    if seen.len() != inventory.declarations.len() {
        return Err("incomplete history export; generated schema tests must run".into());
    }
    HistoryLedger::from_entries(observed, schema_identity).map_err(Into::into)
}

#[cfg(test)]
mod tests;
