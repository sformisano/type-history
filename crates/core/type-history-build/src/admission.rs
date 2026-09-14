//! Standalone positional reconciliation and Cargo declaration-file output.

use crate::{inventory::PackageInventory, package, Result};
use std::{
    env, fs,
    path::{Path, PathBuf},
};
use type_history_codegen::{
    admission::{self, Admission, Invocation},
    history::HistoryPlan,
    source::standalone::{self, RecordDeclaration},
};

pub(crate) fn invocation(root: &Path, declaration: &RecordDeclaration) -> Result<Invocation> {
    let position = declaration.input.name.span().start();
    Ok(Invocation {
        path: root.join(&declaration.source_path).canonicalize()?,
        line: position.line,
        column: position.column,
        stable_name: declaration.stable_name.clone(),
    })
}

// Custom frontends retain generic ledger metadata, but standalone expansion
// must also match the complete actual source declaration set.
pub(crate) fn discover<M>(root: &Path, inventory: &PackageInventory<M>) -> Result<Vec<Invocation>> {
    let package = package::read(root, &["type-history"])?;
    let source = standalone::discover(root, &package.library, &package.facades)?;
    for path in package.tracked_paths.iter().chain(&source.tracked_paths) {
        println!("cargo::rerun-if-changed={}", path.display());
    }
    if source.declarations.len() != inventory.declarations.len() {
        return Err("history source differs from supplied inventory".into());
    }
    let mut invocations = Vec::new();
    for declaration in source.declarations {
        let plan = HistoryPlan::infer(&declaration.input.name, &declaration.input.fields)?;
        if !inventory
            .declarations
            .iter()
            .any(|entry| entry.stable_name == declaration.stable_name && entry.version == plan.head)
        {
            return Err("history source differs from supplied inventory".into());
        }
        invocations.push(invocation(root, &declaration)?);
    }
    Ok(invocations)
}

pub(crate) fn publish(strict: bool, mut invocations: Vec<Invocation>) -> Result<()> {
    invocations.sort();
    let out =
        PathBuf::from(env::var_os("OUT_DIR").ok_or("history admission requires Cargo OUT_DIR")?)
            .canonicalize()?;
    let path = out.join("type-history/admission.json");
    fs::create_dir_all(path.parent().expect("admission parent"))?;
    fs::write(
        &path,
        serde_json::to_vec(&Admission {
            strict,
            invocations,
        })?,
    )?;
    println!("cargo::rustc-env={}={}", admission::ENV, path.display());
    Ok(())
}
