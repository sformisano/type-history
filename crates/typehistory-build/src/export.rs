//! Compiler exports and ordinary candidate checks over one captured Cargo graph.
use crate::contract::ToolContract;
use crate::inventory::Admission;
use crate::lifecycle::LifecycleOps;
use crate::options::LifecycleOptions;
use crate::snapshot::Snapshot;
use crate::Result;
use serde::{de::DeserializeOwned, Serialize};
use std::path::Path;
use std::process::{Command, Output};
use typehistory_codegen::ledger::HistoryLedger;

pub(crate) fn export<M: Clone + Eq + Serialize + DeserializeOwned>(
    snapshot: &Snapshot,
    root: &Path,
    options: &LifecycleOptions,
    contract: &ToolContract,
    ops: &LifecycleOps<M>,
) -> Result<HistoryLedger<M>> {
    let inventory = (ops.discover)(root, Admission::Ordinary)?;
    let output = cargo(snapshot, &inventory.package, options, contract, true)?;
    (ops.decode_export)(&inventory, &output.stdout)
}

pub(crate) fn validate(
    snapshot: &Snapshot,
    package: &str,
    options: &LifecycleOptions,
    contract: &ToolContract,
) -> Result<()> {
    cargo(snapshot, package, options, contract, false)?;
    Ok(())
}
fn cargo(
    snapshot: &Snapshot,
    package: &str,
    options: &LifecycleOptions,
    contract: &ToolContract,
    export: bool,
) -> Result<Output> {
    let mut command = Command::new("cargo");
    command
        .env_remove("AXIS_REQUIRE_FROZEN_EVENTS")
        .env_remove("TYPEHISTORY_REQUIRE_FROZEN")
        .env_remove("AXIS_SCHEMA_EXPORT")
        .env_remove("TYPEHISTORY_SCHEMA_EXPORT");
    if export {
        command.args(["test", "--lib"]);
        command.env(contract.export_env, "1");
    } else {
        command.args(["check", "--lib"]);
    }
    snapshot.configure_cargo(&mut command)?;
    command
        .args([
            "--locked",
            "--offline",
            "--profile",
            "dev",
            "--package",
            package,
            "--target-dir",
        ])
        .arg(&snapshot.target);
    if let Some(features) = &options.features {
        command.args(["--features", features]);
    }
    if let Some(target) = &options.target {
        command.args(["--target", target]);
    }
    if export {
        command.args([contract.export_test_filter, "--", "--nocapture"]);
    }
    let output = command.output()?;
    if !output.status.success() {
        return Err(format!("{} failed for package {package}. If Cargo configuration restores AXIS_REQUIRE_FROZEN_EVENTS=1 or TYPEHISTORY_REQUIRE_FROZEN=1, a draft/reset cannot be exported; adjust the caller configuration deliberately. Source and ledger were not changed.\n{}\n{}", if export { "schema export" } else { "candidate compilation" }, String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr)).into());
    }
    Ok(output)
}
