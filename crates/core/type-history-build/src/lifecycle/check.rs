//! Read-only captured authority checks with independent release protection.
use super::LifecycleOps;
use crate::check_report::{
    self, CheckCode, CheckReport, Diagnostic, Location, Origin, PackageReport,
};
use crate::compiler_diagnostics::CompilerFailure;
use crate::contract::ToolContract;
use crate::export;
use crate::inventory::PackageInventory;
use crate::options::{self, Action, CheckOptions, LifecycleOptions, OutputFormat};
use crate::snapshot::Snapshot;
use crate::workspace::{self, CargoMetadata, Package};
use crate::Result;
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;
use std::error::Error;
use std::ffi::OsString;
use std::fs;
use std::path::Path;
use type_history_codegen::diagnostics::{compare_shapes, PathSegment};
use type_history_codegen::ledger::{HistoryLedger, SchemaIdentity};

#[cfg(test)]
mod tests;

/// Run lifecycle commands with additive check settings and a complete JSON envelope.
pub fn run_with_check<M: Clone + Eq + Serialize + DeserializeOwned>(
    arguments: impl IntoIterator<Item = OsString>,
    contract: &ToolContract,
    ops: &LifecycleOps<M>,
) -> Result<()> {
    let arguments: Vec<_> = arguments.into_iter().collect();
    let json = options::requests_json(&arguments);
    let mut report = CheckReport::default();
    let parsed = match options::parse_with_check(arguments, contract) {
        Ok(Some(parsed)) => parsed,
        Ok(None) => return Ok(()),
        Err(error) => {
            report.errors.push(Diagnostic::operational(
                CheckCode::InputInvalid,
                Origin::CurrentLedger,
                error.to_string(),
            ));
            return emit(report, json);
        }
    };
    let (options, check) = parsed;
    if options.action != Action::Check {
        let metadata = workspace::metadata_for(&options)?;
        for package in workspace::select(&metadata, options.package.as_deref(), contract)? {
            super::execute(&metadata, package, &options, contract, ops)?;
        }
        return Ok(());
    }
    let metadata = match workspace::metadata_for(&options) {
        Ok(metadata) => metadata,
        Err(error) => {
            report.errors.push(Diagnostic::operational(
                CheckCode::CompilerFailed,
                Origin::Compiler,
                error.to_string(),
            ));
            return emit(report, json);
        }
    };
    let extra = check
        .released_baseline
        .iter()
        .filter_map(|path| path.canonicalize().ok())
        .collect::<Vec<_>>();
    let snapshot =
        match Snapshot::create_reusable(&metadata, &extra, contract).and_then(|snapshot| {
            snapshot.verify_graph(&metadata, &options)?;
            Ok(snapshot)
        }) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                report.errors.push(operation_error(error));
                return emit(report, json);
            }
        };
    let mut packages = match workspace::select_captured(
        &metadata,
        options.package.as_deref(),
        contract,
        &snapshot,
    ) {
        Ok(packages) => packages,
        Err(error) => {
            report.errors.push(Diagnostic::operational(
                CheckCode::InputInvalid,
                Origin::CurrentLedger,
                error.to_string(),
            ));
            return emit(report, json);
        }
    };
    packages.sort_by(|a, b| a.name.cmp(&b.name));
    for package in packages {
        report.packages.push(execute_check_in(
            &metadata,
            package,
            &options,
            &check,
            contract,
            ops,
            Some(&snapshot),
        ));
    }
    // Earlier package results remain bound to this capture until the whole check ends.
    if let Err(error) = snapshot.ensure_fresh() {
        report.errors.push(operation_error(error));
    }
    emit(report, check.format == OutputFormat::Json)
}
fn emit(mut report: CheckReport, json: bool) -> Result<()> {
    report.finish();
    report.render(json)?;
    if report.ok {
        Ok(())
    } else {
        Err("history check failed; see diagnostics".into())
    }
}
/// Check one package without acquiring a mutation lock or changing captured authority.
pub fn execute_check<M: Clone + Eq + Serialize + DeserializeOwned>(
    metadata: &CargoMetadata,
    package: &Package,
    options: &LifecycleOptions,
    check: &CheckOptions,
    contract: &ToolContract,
    ops: &LifecycleOps<M>,
) -> PackageReport {
    execute_check_in(metadata, package, options, check, contract, ops, None)
}

fn execute_check_in<M: Clone + Eq + Serialize + DeserializeOwned>(
    metadata: &CargoMetadata,
    package: &Package,
    options: &LifecycleOptions,
    check: &CheckOptions,
    contract: &ToolContract,
    ops: &LifecycleOps<M>,
    shared: Option<&Snapshot>,
) -> PackageReport {
    let mut result = PackageReport {
        package: package.name.clone(),
        diagnostics: Vec::new(),
    };
    if options.action != Action::Check
        || (check.released_baseline.is_some() && options.package.as_deref() != Some(&package.name))
    {
        result.diagnostics.push(Diagnostic::operational(
            CheckCode::InputInvalid,
            Origin::CurrentLedger,
            "released comparison requires check and one exact selected --package",
        ));
        return result;
    }
    let current_path = package.root().join(contract.ledger_path);
    let released_path = match check
        .released_baseline
        .as_ref()
        .map(|path| path.canonicalize())
        .transpose()
    {
        Ok(path) => path,
        Err(error) => {
            result.diagnostics.push(input_error(
                error.to_string(),
                Origin::ReleasedBaseline,
                check.released_baseline.as_deref(),
            ));
            return result;
        }
    };
    if let Some(released) = &released_path {
        if aliases_current_ledger(&current_path, released) {
            result.diagnostics.push(input_error(
                "--released-baseline must be independent of the current ledger",
                Origin::ReleasedBaseline,
                Some(released),
            ));
            return result;
        }
    }
    let mut extra = vec![current_path.clone()];
    if let Some(path) = &released_path {
        extra.push(path.clone());
    }
    let owned;
    let snapshot = match shared {
        Some(snapshot) => snapshot,
        None => {
            owned =
                match Snapshot::create_reusable(metadata, &extra, contract).and_then(|snapshot| {
                    snapshot.verify_graph(metadata, options)?;
                    Ok(snapshot)
                }) {
                    Ok(snapshot) => snapshot,
                    Err(error) => {
                        result.diagnostics.push(operation_error(error));
                        return result;
                    }
                };
            &owned
        }
    };
    let attempt = (|| -> Result<()> {
        let copied_root = snapshot.mapped(package.root())?;
        let identity = SchemaIdentity::new(contract.schema_id_prefix);
        let current = match read_captured(snapshot, &current_path, identity.clone()) {
            Ok(ledger) => ledger,
            Err(error) => {
                result.diagnostics.push(input_error(
                    error.to_string(),
                    Origin::CurrentLedger,
                    Some(&current_path),
                ));
                return Ok(());
            }
        };
        if let Some(path) = &released_path {
            let released = match read_captured(snapshot, path, identity) {
                Ok(ledger) => ledger,
                Err(error) => {
                    result.diagnostics.push(input_error(
                        error.to_string(),
                        Origin::ReleasedBaseline,
                        Some(path),
                    ));
                    return Ok(());
                }
            };
            result.diagnostics.extend(compare_ledgers(
                &released,
                &current,
                true,
                Some(file_location(&current_path)),
                Some(file_location(path)),
            )?);
        }
        // Baseline findings do not suppress independent compiler observations.
        let observed = export::export(
            snapshot,
            &copied_root,
            &package.name,
            options,
            contract,
            ops,
        )?;
        let mut differences = compare_ledgers(&current, &observed.ledger, false, None, None)?;
        for difference in &mut differences {
            difference.location = source_location(difference, &observed.inventory, snapshot);
        }
        // A rejected observation needs no second compiler pass. A clean one
        // still must satisfy ordinary (non-test, non-export) frozen assertions.
        if differences.is_empty() {
            export::validate(snapshot, &package.name, &copied_root, options, contract)?;
        }
        result.diagnostics.extend(differences);
        Ok(())
    })();
    if let Err(error) = attempt {
        if let Some(failure) = error.downcast_ref::<CompilerFailure>() {
            result.diagnostics.extend(failure.diagnostics.clone());
        } else {
            result.diagnostics.push(operation_error(error));
        }
    }
    // Every captured failure path must verify freshness too.
    if let Err(error) = snapshot.ensure_fresh() {
        result.diagnostics.push(Diagnostic::operational(
            CheckCode::SnapshotStale,
            Origin::CurrentLedger,
            error.to_string(),
        ));
    }
    check_report::order(&mut result.diagnostics);
    result
}

fn source_location<M>(
    diagnostic: &Diagnostic,
    inventory: &PackageInventory<M>,
    snapshot: &Snapshot,
) -> Option<Location> {
    let declaration = inventory.declarations.iter().find(|declaration| {
        Some(declaration.stable_name.as_str()) == diagnostic.stable_name.as_deref()
    })?;
    let source = declaration.source.as_ref()?;
    let location = match diagnostic.path.first() {
        Some(PathSegment::Field { name }) => source.fields.get(name).unwrap_or(&source.declaration),
        _ => &source.declaration,
    };
    let captured = inventory.root.join(&location.file);
    let original = snapshot.original(&captured)?;
    Some(Location {
        file: original.to_string_lossy().into_owned(),
        line: Some(location.line),
        column: Some(location.column),
    })
}
fn aliases_current_ledger(current: &Path, released: &Path) -> bool {
    if current
        .canonicalize()
        .is_ok_and(|path| path.as_path() == released)
    {
        return true;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;

        if let (Ok(current), Ok(released)) = (fs::metadata(current), fs::metadata(released)) {
            return current.dev() == released.dev() && current.ino() == released.ino();
        }
    }
    false
}

fn read_captured<M: Clone + Eq + Serialize + DeserializeOwned>(
    snapshot: &Snapshot,
    path: &Path,
    identity: SchemaIdentity,
) -> Result<HistoryLedger<M>> {
    let bytes = fs::read(snapshot.mapped(path)?)?;
    HistoryLedger::parse(&bytes, identity).map_err(Into::into)
}
fn file_location(path: &Path) -> Location {
    Location {
        file: path.to_string_lossy().into_owned(),
        line: None,
        column: None,
    }
}
fn input_error(message: impl Into<String>, origin: Origin, path: Option<&Path>) -> Diagnostic {
    let mut diagnostic = Diagnostic::operational(CheckCode::InputInvalid, origin, message);
    diagnostic.location = path.map(file_location);
    diagnostic
}
fn operation_error(error: Box<dyn Error>) -> Diagnostic {
    let message = error.to_string();
    let code = if message.contains("InputsChanged:") {
        CheckCode::SnapshotStale
    } else {
        CheckCode::InputInvalid
    };
    Diagnostic::operational(code, Origin::CurrentLedger, message)
}
pub(super) fn compare_ledgers<M: Clone + Eq + Serialize + DeserializeOwned>(
    saved: &HistoryLedger<M>,
    actual: &HistoryLedger<M>,
    released: bool,
    location: Option<Location>,
    released_location: Option<Location>,
) -> Result<Vec<Diagnostic>> {
    let origin = if released {
        Origin::ReleasedBaseline
    } else {
        Origin::CurrentLedger
    };
    let mut findings = Vec::new();
    for (name, versions) in saved.entries() {
        for (version, expected) in versions {
            let mut finding = |code, wanted, observed, hint: &str, location: Option<Location>| {
                let mut diagnostic = Diagnostic::operational(code, origin, hint);
                diagnostic.stable_name = Some(name.clone());
                diagnostic.version = Some(*version);
                diagnostic.expected = wanted;
                diagnostic.actual = observed;
                diagnostic.location = location;
                findings.push(diagnostic);
            };
            if released && expected.reset_draft {
                finding(
                    CheckCode::ReleasedReset,
                    Value::Bool(false),
                    Value::Bool(true),
                    "released authority must contain only frozen entries; supply the original release ledger",
                    released_location.clone(),
                );
            }
            let Some(observed) = actual.snapshot(name, *version) else {
                finding(
                    if actual.entries().contains_key(name) {
                        CheckCode::VersionMissing
                    } else {
                        CheckCode::HistoryMissing
                    },
                    serde_json::to_value(expected)?,
                    Value::Null,
                    "restore the retained history and version; declare changes in a later version",
                    location.clone(),
                );
                continue;
            };
            if released && observed.reset_draft {
                finding(
                    CheckCode::ReleasedReset,
                    Value::Bool(false),
                    Value::Bool(true),
                    "a reset reservation cannot waive released history; restore and freeze the released shape",
                    location.clone(),
                );
            }
            if expected.metadata != observed.metadata {
                finding(
                    CheckCode::MetadataChanged,
                    serde_json::to_value(&expected.metadata)?,
                    serde_json::to_value(&observed.metadata)?,
                    "restore the original history metadata",
                    location.clone(),
                );
            }
            if released || !expected.reset_draft {
                findings.extend(
                    compare_shapes(&expected.schema.shape(), &observed.schema.shape())
                        .into_iter()
                        .map(|difference| {
                            Diagnostic::shape(difference, origin, name, *version, location.clone())
                        }),
                );
            }
        }
    }
    Ok(findings)
}
