//! Pure candidate algorithms preserve frozen shapes, metadata, and reservations.
use crate::contract::ToolContract;
use crate::options::LifecycleOptions;
use crate::Result;
use serde::{de::DeserializeOwned, Serialize};
use std::path::Path;
use type_history_codegen::ledger::{HistoryLedger, Snapshot as HistorySnapshot};

pub(crate) fn compare_frozen<M: Clone + Eq + Serialize + DeserializeOwned>(
    saved: &HistoryLedger<M>,
    observed: &HistoryLedger<M>,
) -> Result<()> {
    for (stable_name, versions) in saved.entries() {
        for (version, frozen) in versions {
            let actual = observed.snapshot(stable_name, *version).ok_or_else(|| {
                format!("retained history missing from source/export: {stable_name} V{version}")
            })?;
            if frozen.metadata != actual.metadata {
                return Err(format!("history metadata changed: {stable_name} V{version}").into());
            }
            if !frozen.reset_draft && !same_shape(frozen, actual) {
                return Err(format!("frozen history shape changed: {stable_name} V{version}; restore it and declare the next version").into());
            }
        }
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
    options: &LifecycleOptions,
    checked_path: &Path,
) -> Result<HistoryLedger<M>> {
    let imported = HistoryLedger::read_file(checked_path, saved.schema_identity().clone())?;
    for (stable_name, versions) in imported.entries() {
        let floor = *versions
            .first_key_value()
            .ok_or("empty import stable name")?
            .0;
        match options.floors.get(stable_name) {
            Some(selected) if *selected != floor || floor == 1 => {
                return Err(format!("mismatched/non-required --floor for {stable_name}").into());
            }
            None if floor != 1 => {
                return Err(format!("import requires --floor {stable_name}={floor}").into());
            }
            _ => {}
        }
        if versions.values().any(|snapshot| snapshot.reset_draft) {
            return Err("import cannot create or clear reset reservations".into());
        }
    }
    for stable_name in options.floors.keys() {
        if !imported.entries().contains_key(stable_name) {
            return Err(
                format!("--floor selects unknown imported stable name {stable_name}").into(),
            );
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
fn same_shape<M: PartialEq>(left: &HistorySnapshot<M>, right: &HistorySnapshot<M>) -> bool {
    left.metadata == right.metadata && left.schema.shape() == right.schema.shape()
}
