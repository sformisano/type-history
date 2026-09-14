//! Cargo's resolved workspace and local source graph.
use crate::contract::ToolContract;
use crate::options::{FeatureSelection, LifecycleOptions};
use crate::snapshot::Snapshot;
use crate::Result;
use serde::Deserialize;
use std::collections::BTreeSet;
use std::io::Result as IoResult;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Cargo-resolved workspace and local dependency graph.
#[derive(Debug, Deserialize)]
pub struct CargoMetadata {
    /// Resolved packages and local manifests needed to validate their graph.
    pub packages: Vec<Package>,
    /// Package IDs selected as workspace members.
    pub workspace_members: Vec<String>,
    /// Absolute root of the selected workspace.
    pub workspace_root: PathBuf,
    /// Existing generated output excluded from source capture.
    pub target_directory: PathBuf,
}
/// One package from Cargo metadata.
#[derive(Debug, Deserialize)]
pub struct Package {
    /// Exact Cargo package ID.
    pub id: String,
    /// Exact Cargo package name.
    pub name: String,
    /// Absolute package manifest.
    pub manifest_path: PathBuf,
    /// Registry or Git source; absent for local path packages.
    pub source: Option<String>,
    /// Cargo targets declared by the package.
    pub targets: Vec<Target>,
    /// Declared dependency paths, including inactive optional dependencies.
    #[serde(default)]
    pub dependencies: Vec<Dependency>,
}
/// A declared dependency's local source, as resolved by Cargo.
#[derive(Debug, Deserialize)]
pub struct Dependency {
    /// Absolute source directory for a path dependency; absent for remote sources.
    pub path: Option<PathBuf>,
}
/// One Cargo target's library or binary kinds.
#[derive(Debug, Deserialize)]
pub struct Target {
    /// Target kinds reported by Cargo.
    pub kind: Vec<String>,
}
impl Package {
    /// Owning directory of this package manifest.
    pub fn root(&self) -> &Path {
        self.manifest_path.parent().expect("Cargo manifest parent")
    }
}
/// Read the locked graph and all local manifests Cargo needs to validate it.
pub fn metadata(features: Option<&str>) -> Result<CargoMetadata> {
    metadata_with_selection(FeatureSelection::with_defaults(features))
}

pub(crate) fn metadata_for(options: &LifecycleOptions) -> Result<CargoMetadata> {
    metadata_with_selection(FeatureSelection::from_options(options))
}

fn metadata_with_selection(selection: FeatureSelection<'_>) -> Result<CargoMetadata> {
    let mut command = Command::new("cargo");
    command.args(["metadata", "--format-version", "1", "--locked", "--offline"]);
    selection.apply_to(&mut command);
    let mut metadata = read_metadata(&mut command)?;
    include_local_manifests(&mut metadata, |command, manifest| {
        command.arg("--manifest-path").arg(manifest);
        Ok(())
    })?;
    Ok(metadata)
}

pub(crate) fn captured_metadata(
    snapshot: &Snapshot,
    options: &LifecycleOptions,
) -> Result<CargoMetadata> {
    let mut command = Command::new("cargo");
    command.args(["metadata", "--format-version", "1", "--locked", "--offline"]);
    FeatureSelection::from_options(options).apply_to(&mut command);
    snapshot.configure_cargo(&mut command)?;
    let mut metadata = read_metadata(&mut command)?;
    include_local_manifests(&mut metadata, |command, manifest| {
        snapshot.configure_cargo_at(command, manifest)
    })?;
    Ok(metadata)
}

fn read_metadata(command: &mut Command) -> Result<CargoMetadata> {
    let output = command.output()?;
    if !output.status.success() {
        return Err(format!(
            "Cargo metadata failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(serde_json::from_slice(&output.stdout)?)
}

fn include_local_manifests(
    metadata: &mut CargoMetadata,
    configure: impl Fn(&mut Command, &Path) -> Result<()>,
) -> Result<()> {
    let mut known = metadata
        .packages
        .iter()
        .map(|package| package.manifest_path.canonicalize())
        .collect::<IoResult<BTreeSet<_>>>()?;
    let mut index = 0;
    while index < metadata.packages.len() {
        let package = &metadata.packages[index];
        let paths = package
            .dependencies
            .iter()
            .filter(|_| package.source.is_none())
            .filter_map(|dependency| dependency.path.as_ref())
            .cloned()
            .collect::<Vec<_>>();
        index += 1;
        for path in paths {
            let manifest = path.join("Cargo.toml").canonicalize()?;
            if known.contains(&manifest) {
                continue;
            }
            // Cargo still reads inactive path manifests to validate a lockfile.
            // Ask Cargo for their declared paths without enabling their features
            // or resolving/fetching their inactive registry dependencies.
            let mut command = Command::new("cargo");
            command.args([
                "metadata",
                "--format-version",
                "1",
                "--no-deps",
                "--locked",
                "--offline",
            ]);
            configure(&mut command, &manifest)?;
            let local = read_metadata(&mut command)?;
            for package in local.packages {
                if known.insert(package.manifest_path.canonicalize()?) {
                    metadata.packages.push(package);
                }
            }
            if !known.contains(&manifest) {
                return Err(format!(
                    "Cargo metadata omitted local manifest {}",
                    manifest.display()
                )
                .into());
            }
        }
    }
    Ok(())
}
/// Select initialized workspace libraries, or one exact library for setup or validation.
pub fn select<'a>(
    metadata: &'a CargoMetadata,
    requested: Option<&str>,
    contract: &ToolContract,
) -> Result<Vec<&'a Package>> {
    select_with(metadata, requested, contract, |package| {
        Ok(package.root().join(contract.ledger_path).is_file())
    })
}

pub(crate) fn select_captured<'a>(
    metadata: &'a CargoMetadata,
    requested: Option<&str>,
    contract: &ToolContract,
    snapshot: &Snapshot,
) -> Result<Vec<&'a Package>> {
    select_with(metadata, requested, contract, |package| {
        Ok(snapshot
            .mapped(&package.root().join(contract.ledger_path))?
            .is_file())
    })
}

fn select_with<'a>(
    metadata: &'a CargoMetadata,
    requested: Option<&str>,
    contract: &ToolContract,
    initialized: impl Fn(&Package) -> Result<bool>,
) -> Result<Vec<&'a Package>> {
    let libraries: Vec<_> = metadata
        .packages
        .iter()
        .filter(|package| {
            metadata.workspace_members.contains(&package.id)
                && package.targets.iter().any(|target| {
                    target.kind.iter().any(|kind| {
                        matches!(
                            kind.as_str(),
                            "lib" | "rlib" | "dylib" | "cdylib" | "staticlib"
                        )
                    })
                })
        })
        .map(|package| Ok((requested.is_some() || initialized(package)?).then_some(package)))
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .flatten()
        .collect();
    match requested {
        Some(name) => {
            let matches: Vec<_> = libraries
                .into_iter()
                .filter(|package| package.name == name)
                .collect();
            if matches.len() != 1 {
                return Err(
                    format!("package {name} must select exactly one workspace library").into(),
                );
            }
            Ok(matches)
        }
        None if libraries.is_empty() => Err(format!(
            "no initialized workspace library packages with {}; run cargo {} init --package NAME",
            contract.ledger_path, contract.command
        )
        .into()),
        None => Ok(libraries),
    }
}

#[cfg(test)]
mod tests {
    use super::{select, CargoMetadata, Package, Target};
    use crate::contract::STANDALONE;
    use std::fs;

    #[test]
    fn workspace_selection_uses_ledgers_and_explicit_setup_needs_only_a_library() {
        let root = tempfile::tempdir().unwrap();
        let packages = ["initialized", "empty", "binary", "dependency"]
            .into_iter()
            .map(|name| Package {
                id: name.into(),
                name: name.into(),
                manifest_path: root.path().join(name).join("Cargo.toml"),
                source: None,
                targets: vec![Target {
                    kind: vec![if name == "binary" { "bin" } else { "lib" }.into()],
                }],
                dependencies: Vec::new(),
            })
            .collect();
        let metadata = CargoMetadata {
            packages,
            workspace_members: ["initialized", "empty", "binary"]
                .map(String::from)
                .to_vec(),
            workspace_root: root.path().to_owned(),
            target_directory: root.path().join("target"),
        };
        assert!(select(&metadata, None, &STANDALONE).is_err());
        assert_eq!(
            select(&metadata, Some("empty"), &STANDALONE).unwrap()[0].name,
            "empty"
        );
        for name in ["initialized", "binary", "dependency"] {
            let ledger = root.path().join(name).join(STANDALONE.ledger_path);
            fs::create_dir_all(ledger.parent().unwrap()).unwrap();
            // Selection must include malformed ledgers so ordinary validation can reject them.
            fs::write(ledger, "invalid").unwrap();
        }
        let selected = select(&metadata, None, &STANDALONE).unwrap();
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].name, "initialized");
        for name in ["binary", "dependency", "missing"] {
            assert!(select(&metadata, Some(name), &STANDALONE).is_err());
        }
    }
}
