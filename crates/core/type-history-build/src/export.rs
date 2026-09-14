//! Compiler exports and ordinary candidate checks over one captured Cargo graph.
use crate::check_report::{CheckCode, Diagnostic, Origin};
use crate::compiler_diagnostics::{self, CompilerFailure};
use crate::contract::ToolContract;
use crate::inventory::Admission;
use crate::lifecycle::LifecycleOps;
use crate::options::{Action, FeatureSelection, LifecycleOptions};
use crate::snapshot::Snapshot;
use crate::Result;
use serde::{de::DeserializeOwned, Serialize};
use std::path::Path;
use std::process::{Command, Output};
use type_history_codegen::ledger::HistoryLedger;

pub(crate) fn export<M: Clone + Eq + Serialize + DeserializeOwned>(
    snapshot: &Snapshot,
    root: &Path,
    package: &str,
    options: &LifecycleOptions,
    contract: &ToolContract,
    ops: &LifecycleOps<M>,
) -> Result<HistoryLedger<M>> {
    let inventory = match (ops.discover)(root, Admission::Ordinary) {
        Ok(inventory) => inventory,
        Err(discovery_error) => {
            if options.action == Action::Check {
                // A Rust parse failure must still reach the ordinary compiler.
                // Successful compilation never excuses failed source discovery.
                cargo(snapshot, package, options, contract, true)?;
            }
            return Err(discovery_error);
        }
    };
    let output = cargo(snapshot, &inventory.package, options, contract, true)?;
    (ops.decode_export)(&inventory, &output.stdout)
}

pub(crate) fn validate(
    snapshot: &Snapshot,
    package: &str,
    _root: &Path,
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
        .env_remove(contract.strict_env)
        .env_remove(contract.export_env);
    if export {
        command.args(["test", "--lib"]);
        command.env(contract.export_env, "1");
    } else {
        command.args(["check", "--lib"]);
    }
    snapshot.configure_cargo(&mut command)?;
    command
        .args([
            "--message-format=json",
            "--locked",
            "--offline",
            "--profile",
            "dev",
            "--package",
            package,
            "--target-dir",
        ])
        .arg(&snapshot.target);
    FeatureSelection::from_options(options).apply_to(&mut command);
    if let Some(target) = &options.target {
        command.args(["--target", target]);
    }
    if export {
        command.args([contract.export_test_filter, "--", "--nocapture"]);
    }
    let output = command.output()?;
    let mut diagnostics = compiler_diagnostics::decode(&output.stdout, snapshot);
    eprint!("{}", String::from_utf8_lossy(&output.stderr));
    if !output.status.success() {
        let message = format!(
            "{} failed for package {package}. If Cargo configuration restores {}=1, a draft/reset cannot be exported; adjust the caller configuration deliberately. Source and ledger were not changed.\n{}",
            if export {
                "schema export"
            } else {
                "candidate compilation"
            },
            contract.strict_env,
            String::from_utf8_lossy(&output.stderr)
        );
        if diagnostics.is_empty() {
            diagnostics.push(Diagnostic::operational(
                CheckCode::CompilerFailed,
                Origin::Compiler,
                &message,
            ));
        }
        return Err(Box::new(CompilerFailure {
            diagnostics,
            message,
        }));
    }
    Ok(output)
}
