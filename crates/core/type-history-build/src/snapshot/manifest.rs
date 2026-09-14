//! Remap Cargo-owned path fields while leaving application metadata intact.
use super::mirrored;
use crate::Result;
use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};
use toml::Value;

pub(super) fn remap_paths(document: &mut Value, mirror: &Path, roots: &[PathBuf]) -> Result<bool> {
    let mut changed = false;
    if let Some(package) = document.get_mut("package") {
        changed |= fields(
            package,
            &["build", "workspace", "readme", "license-file"],
            mirror,
            roots,
        )?;
    }
    if let Some(workspace) = document.get_mut("workspace") {
        for key in ["members", "default-members", "exclude"] {
            if let Some(paths) = workspace.get_mut(key).and_then(Value::as_array_mut) {
                for path in paths {
                    changed |= remap_path(path, mirror, roots)?;
                }
            }
        }
        if let Some(package) = workspace.get_mut("package") {
            changed |= fields(package, &["readme", "license-file"], mirror, roots)?;
        }
        changed |= dependencies(workspace, mirror, roots)?;
    }
    changed |= dependencies(document, mirror, roots)?;
    if let Some(targets) = document.get_mut("target").and_then(Value::as_table_mut) {
        for (_, target) in targets.iter_mut() {
            changed |= dependencies(target, mirror, roots)?;
        }
    }
    if let Some(patches) = document.get_mut("patch").and_then(Value::as_table_mut) {
        for (_, registry) in patches.iter_mut() {
            changed |= dependency_paths(registry, mirror, roots)?;
        }
    }
    if let Some(replacements) = document.get_mut("replace") {
        changed |= dependency_paths(replacements, mirror, roots)?;
    }
    if let Some(library) = document.get_mut("lib") {
        changed |= fields(library, &["path"], mirror, roots)?;
    }
    for key in ["bin", "example", "test", "bench"] {
        if let Some(targets) = document.get_mut(key).and_then(Value::as_array_mut) {
            for target in targets {
                changed |= fields(target, &["path"], mirror, roots)?;
            }
        }
    }
    Ok(changed)
}

fn dependencies(table: &mut Value, mirror: &Path, roots: &[PathBuf]) -> Result<bool> {
    let mut changed = false;
    for key in [
        "dependencies",
        "dev-dependencies",
        "build-dependencies",
        "dev_dependencies",
        "build_dependencies",
    ] {
        if let Some(dependencies) = table.get_mut(key) {
            changed |= dependency_paths(dependencies, mirror, roots)?;
        }
    }
    Ok(changed)
}

fn dependency_paths(dependencies: &mut Value, mirror: &Path, roots: &[PathBuf]) -> Result<bool> {
    let mut changed = false;
    if let Some(dependencies) = dependencies.as_table_mut() {
        for (_, dependency) in dependencies.iter_mut() {
            changed |= fields(dependency, &["path"], mirror, roots)?;
        }
    }
    Ok(changed)
}

fn fields(table: &mut Value, keys: &[&str], mirror: &Path, roots: &[PathBuf]) -> Result<bool> {
    let mut changed = false;
    for key in keys {
        if let Some(value) = table.get_mut(*key) {
            changed |= remap_path(value, mirror, roots)?;
        }
    }
    Ok(changed)
}

fn remap_path(value: &mut Value, mirror: &Path, roots: &[PathBuf]) -> Result<bool> {
    let Some(path) = value
        .as_str()
        .map(Path::new)
        .filter(|path| path.is_absolute())
    else {
        return Ok(false);
    };
    // Workspace patterns need not exist. Check their lexical boundary too,
    // so a prefix alone cannot admit `/root/../outside`.
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                normalized.pop();
            }
            Component::CurDir => {}
            component => normalized.push(component.as_os_str()),
        }
    }
    // Existing paths retain filesystem semantics when `..` follows a symlink.
    let (normalized, unresolved) = match path.canonicalize() {
        Ok(path) => (path, false),
        Err(error) if error.kind() == ErrorKind::NotFound => (normalized, true),
        Err(error) => return Err(error.into()),
    };
    if !roots.iter().any(|root| normalized.starts_with(root)) {
        return Err(format!(
            "absolute manifest path {} is outside the checked local graph",
            path.display()
        )
        .into());
    }
    if unresolved && path.components().any(|part| part == Component::ParentDir) {
        return Err(format!(
            "cannot snapshot unresolved absolute manifest path {} containing '..'; use an existing path or a pattern without parent-directory components",
            path.display()
        ).into());
    }
    *value = Value::String(
        mirrored(mirror, &normalized)?
            .to_string_lossy()
            .into_owned(),
    );
    Ok(true)
}

#[cfg(test)]
mod tests;
