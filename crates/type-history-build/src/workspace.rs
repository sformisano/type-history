//! Cargo's resolved workspace and local source graph.
use crate::contract::ToolContract;
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
    let mut command = Command::new("cargo");
    command.args(["metadata", "--format-version", "1", "--locked", "--offline"]);
    if let Some(features) = features {
        command.args(["--features", features]);
    }
    let mut metadata = read_metadata(&mut command)?;
    include_local_manifests(&mut metadata)?;
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

fn include_local_manifests(metadata: &mut CargoMetadata) -> Result<()> {
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
            command
                .args([
                    "metadata",
                    "--format-version",
                    "1",
                    "--no-deps",
                    "--locked",
                    "--offline",
                    "--manifest-path",
                ])
                .arg(&manifest);
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
/// Select configured workspace libraries, or one exact configured package.
pub fn select<'a>(
    metadata: &'a CargoMetadata,
    requested: Option<&str>,
    contract: &ToolContract,
) -> Result<Vec<&'a Package>> {
    let domains: Vec<_> = metadata
        .packages
        .iter()
        .filter(|package| {
            metadata.workspace_members.contains(&package.id)
                && package.root().join(contract.config_path).is_file()
                && package.targets.iter().any(|target| {
                    target.kind.iter().any(|kind| {
                        matches!(
                            kind.as_str(),
                            "lib" | "rlib" | "dylib" | "cdylib" | "staticlib"
                        )
                    })
                })
        })
        .collect();
    match requested {
        Some(name) => {
            let matches: Vec<_> = domains
                .into_iter()
                .filter(|package| package.name == name)
                .collect();
            if matches.len() != 1 {
                return Err(format!(
                    "package {name} must select exactly one configured workspace library with {}",
                    contract.config_path
                )
                .into());
            }
            Ok(matches)
        }
        None if domains.is_empty() => Err(format!(
            "no configured workspace library packages with {}",
            contract.config_path
        )
        .into()),
        None => Ok(domains),
    }
}
