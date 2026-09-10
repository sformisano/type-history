//! Validate generated standalone export rows against complete source inventory.
use crate::contract::STANDALONE;
use crate::inventory::PackageInventory;
use crate::Result;
use serde::Deserialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use typehistory_codegen::json_schema::JsonSchemaDocument;
use typehistory_codegen::ledger::{HistoryLedger, RecordMetadata, SchemaIdentity, Snapshot};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExportRecord {
    identity: String,
    versions: Vec<ExportVersion>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExportVersion {
    version: u32,
    wire: Value,
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
            .find(|item| item.identity == record.identity)
            .ok_or("unexpected exported history")?;
        if !seen.insert(record.identity.clone()) {
            return Err("duplicate exported history".into());
        }
        let mut versions = BTreeMap::new();
        for version in record.versions {
            if !(declaration.retained_from..=declaration.version).contains(&version.version) {
                return Err(format!(
                    "exported version differs from declared history: {} V{}",
                    declaration.identity, version.version
                )
                .into());
            }
            let schema = JsonSchemaDocument::from_export(
                version.wire,
                &schema_identity.schema_id(&record.identity),
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
        let expected = u64::from(declaration.version) - u64::from(declaration.retained_from) + 1;
        if versions.len() as u64 != expected {
            return Err(format!(
                "incomplete exported retained history for {}",
                declaration.identity
            )
            .into());
        }
        observed.insert(record.identity, versions);
    }
    if seen.len() != inventory.declarations.len() {
        return Err("incomplete history export; generated schema tests must run".into());
    }
    HistoryLedger::from_entries(observed, schema_identity).map_err(Into::into)
}

#[cfg(test)]
mod tests;
