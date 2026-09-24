//! Cargo manifest inputs needed by source declaration selectors.

use crate::Result;
use std::{
    collections::BTreeSet,
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};
use toml::{Table, Value};

/// Manifest-derived library and facade names shared by both source selectors.
pub struct PackageSource {
    /// Cargo package name.
    pub package: String,
    /// Canonical package root.
    pub root: PathBuf,
    /// Parsed package manifest for frontend-owned metadata checks.
    pub manifest: Value,
    /// Selected ordinary library target.
    pub library: PathBuf,
    /// Rust dependency spellings of selected public facades.
    pub facades: Vec<String>,
    /// Package and workspace manifests used during resolution.
    pub tracked_paths: Vec<PathBuf>,
}

impl PackageSource {
    pub(crate) fn library_name(&self) -> String {
        self.manifest
            .get("lib")
            .and_then(|value| value.get("name"))
            .and_then(Value::as_str)
            .unwrap_or(&self.package)
            .replace('-', "_")
    }
}

/// Resolve a package library and renamed/inherited facade dependencies.
pub fn read(root: &Path, facade_packages: &[&str]) -> Result<PackageSource> {
    let root = root.canonicalize()?;
    let manifest_path = root.join("Cargo.toml");
    let manifest: Value = toml::from_slice(&fs::read(&manifest_path)?)?;
    let package = manifest
        .get("package")
        .and_then(|value| value.get("name"))
        .and_then(Value::as_str)
        .ok_or("missing package name")?
        .to_owned();
    let mut tracked_paths = vec![manifest_path];
    let mut workspace = Table::new();
    if let Some(path) = workspace_manifest(&root, &manifest)? {
        let parent: Value = toml::from_slice(&fs::read(&path)?)?;
        let value = parent
            .get("workspace")
            .ok_or("workspace manifest has no workspace table")?;
        tracked_paths.push(path);
        if let Some(table) = value.get("dependencies").and_then(Value::as_table) {
            workspace = table.clone();
        }
    }
    let facades = facade_names(&manifest, &workspace, facade_packages);
    let library = root.join(
        manifest
            .get("lib")
            .and_then(|value| value.get("path"))
            .and_then(Value::as_str)
            .unwrap_or("src/lib.rs"),
    );
    tracked_paths.sort();
    tracked_paths.dedup();
    Ok(PackageSource {
        package,
        root,
        manifest,
        library,
        facades,
        tracked_paths,
    })
}

/// Resolve the manifest that owns a local package's workspace inheritance.
pub(crate) fn workspace_manifest(root: &Path, manifest: &Value) -> Result<Option<PathBuf>> {
    workspace_manifest_logical(root, manifest)?
        .map(|path| path.canonicalize())
        .transpose()
        .map_err(Into::into)
}

/// Resolve the authored path Cargo uses for a local package's workspace manifest.
pub(crate) fn workspace_manifest_logical(root: &Path, manifest: &Value) -> Result<Option<PathBuf>> {
    // This reads workspace ownership without resolving dependencies or acquiring
    // the build lock. Ancestor search alone ignores Cargo's membership/exclusions.
    let cargo = env::var_os("CARGO");
    // Cargo supplies its exact executable to build scripts. Direct CLI callers
    // also retain their selected rustup toolchain when leaving the caller cwd.
    let toolchain = if cargo.is_none() {
        crate::toolchain::active()?
    } else {
        None
    };
    let mut command = Command::new(cargo.unwrap_or_else(|| "cargo".into()));
    command
        .current_dir("/")
        .args([
            "locate-project",
            "--workspace",
            "--message-format",
            "plain",
            "--locked",
            "--offline",
            "--manifest-path",
        ])
        .arg(root.join("Cargo.toml"));
    if let Some(toolchain) = toolchain {
        command.env("RUSTUP_TOOLCHAIN", toolchain);
    }
    let output = command.output()?;
    if !output.status.success() {
        return Err(format!(
            "Cargo workspace resolution failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    let path = PathBuf::from(String::from_utf8(output.stdout)?.trim());
    Ok((path != root.join("Cargo.toml") || manifest.get("workspace").is_some()).then_some(path))
}

fn facade_names(manifest: &Value, workspace: &Table, facade_packages: &[&str]) -> Vec<String> {
    // Inventories cannot depend on cfg. Collect dependency spellings from every
    // target table; Rust still owns target selection and macro resolution.
    let target_tables = manifest
        .get("target")
        .and_then(Value::as_table)
        .into_iter()
        .flat_map(Table::values);
    std::iter::once(manifest)
        .chain(target_tables)
        .filter_map(|table| table.get("dependencies").and_then(Value::as_table))
        .flat_map(Table::iter)
        .filter(|(name, value)| {
            let resolved = if value.get("workspace").and_then(Value::as_bool) == Some(true) {
                workspace.get(*name).unwrap_or(value)
            } else {
                value
            };
            facade_packages.contains(
                &resolved
                    .get("package")
                    .and_then(Value::as_str)
                    .unwrap_or(name),
            )
        })
        .map(|(name, _)| name.replace('-', "_"))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::read;
    use std::fs;

    #[test]
    fn explicit_workspace_owns_inherited_aliases_and_tracked_manifest() {
        let owner = tempfile::tempdir().unwrap();
        let root = owner.path().join("parent/member");
        let workspace = owner.path().join("workspace");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir(root.join("src")).unwrap();
        fs::write(root.join("src/lib.rs"), "").unwrap();
        fs::create_dir(&workspace).unwrap();
        let unrelated = owner.path().join("parent/Cargo.toml");
        fs::write(&unrelated, "[workspace]\n").unwrap();
        fs::write(root.join("Cargo.toml"), "[package]\nname = 'member'\nworkspace = '../../workspace'\n[dependencies]\nhistory_api.workspace = true\n").unwrap();
        let selected = workspace.join("Cargo.toml");
        fs::write(&selected, "[workspace]\nmembers = ['../parent/member']\n[workspace.dependencies]\nhistory_api = { package = 'type-history', version = '0.3.1' }\n").unwrap();
        let package = read(&root, &["type-history"]).unwrap();
        assert_eq!(package.facades, ["history_api"]);
        assert!(package.tracked_paths.contains(&selected));
        assert!(!package.tracked_paths.contains(&unrelated));
    }
}
