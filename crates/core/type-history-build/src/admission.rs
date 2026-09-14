//! Standalone positional reconciliation and Cargo declaration-file output.

use crate::{
    inventory::PackageInventory,
    package::{self, PackageSource},
    Result,
};
use std::{
    env, fs,
    io::ErrorKind,
    path::{Path, PathBuf},
};
use type_history_codegen::{
    admission::{self, Admission, Declaration, Invocation},
    history::HistoryPlan,
    source::standalone::{self, RecordDeclaration},
};

pub(crate) fn invocation(
    package: &PackageSource,
    declaration: &RecordDeclaration,
) -> Result<Declaration> {
    let position = declaration.input.name.span().start();
    Ok(Declaration {
        invocation: Invocation {
            path: package.root.join(&declaration.source_path).canonicalize()?,
            line: position.line,
            column: position.column,
            stable_name: declaration.stable_name.clone(),
        },
        module_path: std::iter::once(package.library_name())
            .chain(declaration.module_path.iter().cloned())
            .collect(),
        rust_name: declaration.input.name.to_string(),
    })
}

// Custom frontends retain generic ledger metadata, but standalone expansion
// must also match the complete actual source declaration set.
pub(crate) fn discover<M>(
    root: &Path,
    inventory: &PackageInventory<M>,
) -> Result<Vec<Declaration>> {
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
        invocations.push(invocation(&package, &declaration)?);
    }
    Ok(invocations)
}

pub(crate) fn publish(strict: bool, mut declarations: Vec<Declaration>) -> Result<()> {
    declarations.sort();
    let out =
        PathBuf::from(env::var_os("OUT_DIR").ok_or("history admission requires Cargo OUT_DIR")?)
            .canonicalize()?;
    let path = out.join("type-history/admission.json");
    fs::create_dir_all(path.parent().expect("admission parent"))?;
    let bytes = serde_json::to_vec(&Admission {
        strict,
        declarations,
    })?;
    let previous = match fs::read(&path) {
        Ok(bytes) => Some(bytes),
        Err(error) if error.kind() == ErrorKind::NotFound => None,
        Err(error) => return Err(error.into()),
    };
    if previous.as_deref() != Some(bytes.as_slice()) {
        fs::write(&path, bytes)?;
    }
    println!("cargo::rustc-env={}={}", admission::ENV, path.display());
    Ok(())
}
