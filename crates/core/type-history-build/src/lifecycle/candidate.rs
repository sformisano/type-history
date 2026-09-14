//! Pure candidate algorithms preserve frozen shapes, metadata, and reservations.
use super::check::compare_ledgers;
use crate::check_report::{CheckCode, DiagnosticCode};
use crate::contract::ToolContract;
use crate::options::LifecycleOptions;
use crate::Result;
use serde::{de::DeserializeOwned, Serialize};
use std::path::Path;
use type_history_codegen::ledger::HistoryLedger;

pub(crate) fn compare_frozen<M: Clone + Eq + Serialize + DeserializeOwned>(
    saved: &HistoryLedger<M>,
    observed: &HistoryLedger<M>,
) -> Result<()> {
    if let Some(finding) = compare_ledgers(saved, observed, false, None, None)?.first() {
        let stable_name = finding.stable_name.as_deref().unwrap_or("<unknown>");
        let version = finding.version.unwrap_or_default();
        let description = match finding.code {
            DiagnosticCode::Check(CheckCode::MetadataChanged) => "history metadata changed",
            DiagnosticCode::Check(CheckCode::HistoryMissing | CheckCode::VersionMissing) => {
                "retained history missing from source/export"
            }
            _ => "frozen history shape changed",
        };
        return Err(format!("{description}: {stable_name} V{version}; {}", finding.hint).into());
    }
    Ok(())
}

pub(crate) fn freeze_candidate<M: Clone + Eq + Serialize + DeserializeOwned>(
    saved: &HistoryLedger<M>,
    observed: &HistoryLedger<M>,
    options: &LifecycleOptions,
    contract: &ToolContract,
) -> Result<HistoryLedger<M>> {
    compare_frozen(saved, observed)?;
    if options.selected.is_none()
        && saved
            .entries()
            .values()
            .flat_map(|versions| versions.values())
            .any(|entry| entry.reset_draft)
    {
        return Err(format!(
            "package freeze refuses reset reservations; select exact --{} and --version",
            contract.selector
        )
        .into());
    }
    if let Some((stable_name, version)) = &options.selected {
        if observed.snapshot(stable_name, *version).is_none() {
            return Err(
                format!("selected history is not declared: {stable_name} V{version}").into(),
            );
        }
    }
    let mut candidate = saved.entries().clone();
    for (stable_name, versions) in observed.entries() {
        for (version, actual) in versions {
            if options
                .selected
                .as_ref()
                .is_some_and(|(selected, selected_version)| {
                    selected != stable_name || selected_version != version
                })
            {
                continue;
            }
            let versions = candidate.entry(stable_name.clone()).or_default();
            if versions.get(version).is_none_or(|entry| entry.reset_draft) {
                versions.insert(*version, actual.clone());
            }
        }
    }
    HistoryLedger::from_entries(candidate, saved.schema_identity().clone()).map_err(Into::into)
}
pub(crate) fn reset_candidate<M: Clone + Eq + Serialize + DeserializeOwned>(
    saved: &HistoryLedger<M>,
    options: &LifecycleOptions,
) -> Result<HistoryLedger<M>> {
    let (stable_name, version) = options
        .selected
        .as_ref()
        .ok_or("reset requires an exact stable name/version")?;
    let mut candidate = saved.entries().clone();
    let versions = candidate
        .get_mut(stable_name)
        .ok_or("reset requires an existing frozen stable name")?;
    if versions.last_key_value().map(|(head, _)| head) != Some(version) {
        return Err("reset only permits the highest retained frozen version".into());
    }
    let entry = versions
        .get_mut(version)
        .ok_or("reset requires a frozen version")?;
    if options.undo && !entry.reset_draft {
        return Err("undo requires an existing reset reservation".into());
    }
    if !options.undo && entry.reset_draft {
        return Err("selected version already has a reset reservation".into());
    }
    entry.reset_draft = !options.undo;
    HistoryLedger::from_entries(candidate, saved.schema_identity().clone()).map_err(Into::into)
}
pub(crate) fn import_candidate<M: Clone + Eq + Serialize + DeserializeOwned>(
    saved: &HistoryLedger<M>,
    checked_path: &Path,
) -> Result<HistoryLedger<M>> {
    let imported = HistoryLedger::read_file(checked_path, saved.schema_identity().clone())?;
    for versions in imported.entries().values() {
        if versions.values().any(|snapshot| snapshot.reset_draft) {
            return Err("import cannot create or clear reset reservations".into());
        }
    }
    for (stable_name, versions) in saved.entries() {
        for (version, existing) in versions {
            if imported.snapshot(stable_name, *version) != Some(existing) {
                return Err(format!(
                    "import changes or deletes existing authority: {stable_name} V{version}"
                )
                .into());
            }
        }
    }
    Ok(imported)
}
