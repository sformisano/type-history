//! Shared package lifecycle with one captured candidate and one commit point.
use crate::contract::ToolContract;
use crate::export;
use crate::inventory::{Admission, PackageInventory};
use crate::options::{self, Action, LifecycleOptions};
use crate::snapshot::Snapshot;
use crate::transaction::{encoded, Transaction};
use crate::workspace::{self, CargoMetadata, Package};
use crate::Result;
use serde::{de::DeserializeOwned, Serialize};
use std::ffi::OsString;
use std::fs;
use std::path::Path;
use type_history_codegen::ledger::{HistoryLedger, SchemaIdentity};

mod candidate;
use candidate::{compare_frozen, freeze_candidate, import_candidate, reset_candidate};
#[cfg(test)]
mod tests;

/// Frontend-owned source selection and compiler export decoding.
pub struct LifecycleOps<M> {
    /// Read the complete captured package inventory under the selected admission.
    pub discover: fn(&Path, Admission) -> Result<PackageInventory<M>>,
    /// Validate compiler rows against that captured inventory and metadata.
    pub decode_export: fn(&PackageInventory<M>, &[u8]) -> Result<HistoryLedger<M>>,
}

/// Run the shared CLI parsing, package selection, and lifecycle sequence.
pub fn run<M: Clone + Eq + Serialize + DeserializeOwned>(
    arguments: impl IntoIterator<Item = OsString>,
    contract: &ToolContract,
    ops: &LifecycleOps<M>,
) -> Result<()> {
    let Some(options) = options::parse(arguments, contract)? else {
        return Ok(());
    };
    let metadata = workspace::metadata(options.features.as_deref())?;
    for package in workspace::select(&metadata, options.package.as_deref(), contract)? {
        execute(&metadata, package, &options, contract, ops)?;
    }
    Ok(())
}

/// Execute one package operation with shared source capture and commit ordering.
pub fn execute<M: Clone + Eq + Serialize + DeserializeOwned>(
    metadata: &CargoMetadata,
    package: &Package,
    options: &LifecycleOptions,
    contract: &ToolContract,
    ops: &LifecycleOps<M>,
) -> Result<()> {
    if options.action == Action::Check {
        return check(metadata, package, options, contract, ops);
    }
    let transaction = Transaction::acquire(package.root(), &package.name, contract)?;
    let before = match options.action {
        Action::Init => {
            if transaction.original().is_some() {
                return Err("init refuses to overwrite an existing ledger".into());
            }
            HistoryLedger::empty(SchemaIdentity::new(contract.schema_id_prefix))
        }
        Action::Import if transaction.original().is_none() => {
            HistoryLedger::empty(SchemaIdentity::new(contract.schema_id_prefix))
        }
        _ => transaction.baseline(SchemaIdentity::new(contract.schema_id_prefix))?,
    };
    let mut extra = vec![transaction.path.clone()];
    let import_path = options
        .from
        .as_ref()
        .map(|path| path.canonicalize())
        .transpose()?;
    if let Some(path) = &import_path {
        extra.push(path.clone());
    }
    let snapshot = Snapshot::create(metadata, &extra, contract)?;
    let copied_root = snapshot.mapped(package.root())?;
    // Admission and compilation must inspect the same captured source.
    if options.action == Action::Init
        && (ops.discover)(&copied_root, Admission::ExplicitSetup)?.declaration_count != 0
    {
        return Err("init requires a package without history declarations; restore the committed ledger or use explicit import".into());
    }
    let copied_ledger = copied_root.join(contract.ledger_path);
    let preliminary = match options.action {
        Action::Reset => reset_candidate(&before, options)?,
        Action::Import => {
            let copied_import =
                snapshot.mapped(import_path.as_deref().ok_or("import requires --from")?)?;
            import_candidate(&before, options, &copied_import)?
        }
        _ => before.clone(),
    };
    // Candidate files exist only in this owned copy and use ordinary compiler authority.
    fs::create_dir_all(copied_ledger.parent().ok_or("snapshot ledger parent")?)?;
    fs::write(&copied_ledger, encoded(&preliminary)?)?;
    if options.action == Action::Init {
        export::validate(&snapshot, &package.name, options, contract)?;
        transaction.commit(&preliminary, &snapshot)?;
        println!(
            "{}: initialized empty history ledger; author declarations after initialization",
            package.name
        );
        return Ok(());
    }
    let observed = export::export(&snapshot, &copied_root, options, contract, ops)?;
    let candidate = match options.action {
        Action::Freeze => freeze_candidate(&before, &observed, options, contract)?,
        Action::Reset => {
            compare_frozen(&preliminary, &observed)?;
            preliminary
        }
        Action::Import => {
            if preliminary.entries() != observed.entries() {
                return Err("import must contain the complete compiler-resolved retained inventory and shapes".into());
            }
            preliminary
        }
        Action::Check => unreachable!("read-only check handled before acquiring a mutation lock"),
        Action::Init => unreachable!("init handled above"),
    };
    if candidate == before {
        snapshot.ensure_fresh()?;
        println!(
            "{}: selected authority is already frozen and unchanged; no-op",
            package.name
        );
        return Ok(());
    }
    fs::write(&copied_ledger, encoded(&candidate)?)?;
    (ops.discover)(&copied_root, Admission::Ordinary)?;
    export::validate(&snapshot, &package.name, options, contract)?;
    transaction.commit(&candidate, &snapshot)?;
    println!(
        "{}: {:?} committed and read back",
        package.name, options.action
    );
    Ok(())
}

fn check<M: Clone + Eq + Serialize + DeserializeOwned>(
    metadata: &CargoMetadata,
    package: &Package,
    options: &LifecycleOptions,
    contract: &ToolContract,
    ops: &LifecycleOps<M>,
) -> Result<()> {
    let baseline = HistoryLedger::read_file(
        &package.root().join(contract.ledger_path),
        SchemaIdentity::new(contract.schema_id_prefix),
    )?;
    let snapshot = Snapshot::create(
        metadata,
        &[package.root().join(contract.ledger_path)],
        contract,
    )?;
    let copied_root = snapshot.mapped(package.root())?;
    let observed = export::export(&snapshot, &copied_root, options, contract, ops)?;
    compare_frozen(&baseline, &observed)?;
    snapshot.ensure_fresh()?;
    println!(
        "{}: frozen history schemas match; ordinary development drafts remain drafts",
        package.name
    );
    Ok(())
}
