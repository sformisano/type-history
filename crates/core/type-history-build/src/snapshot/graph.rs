//! Bind Cargo's discovery result to the graph resolved from captured inputs.
use super::Snapshot;
use crate::options::LifecycleOptions;
use crate::workspace::{self, CargoMetadata, Package};
use crate::Result;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

impl Snapshot {
    pub(crate) fn verify_graph(
        &self,
        metadata: &CargoMetadata,
        options: &LifecycleOptions,
    ) -> Result<()> {
        let captured = workspace::captured_metadata(self, options)?;
        let expected = Graph::read(metadata, |path| Ok(path.to_owned()))?;
        let actual = Graph::read(&captured, |path| {
            self.original(path)
                .ok_or_else(|| "captured local Cargo path escaped its snapshot".into())
        })?;
        if expected != actual {
            return Err("InputsChanged: Cargo package graph or workspace membership changed before source capture; retry without editing inputs".into());
        }
        self.ensure_fresh()
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
enum PackageKey {
    Local(PathBuf),
    External(String),
}

#[derive(Debug, PartialEq, Eq)]
struct PackageInputs {
    name: String,
    kinds: BTreeSet<BTreeSet<String>>,
    dependencies: BTreeSet<PathBuf>,
}

#[derive(Debug, PartialEq, Eq)]
struct Graph {
    root: PathBuf,
    packages: BTreeMap<PackageKey, PackageInputs>,
    members: BTreeSet<PackageKey>,
}

impl Graph {
    fn read(metadata: &CargoMetadata, original: impl Fn(&Path) -> Result<PathBuf>) -> Result<Self> {
        let key = |package: &Package| -> Result<PackageKey> {
            Ok(if package.source.is_none() {
                PackageKey::Local(original(&package.manifest_path)?)
            } else {
                // External package IDs include their exact source and version.
                PackageKey::External(package.id.clone())
            })
        };
        let mut packages = BTreeMap::new();
        let mut members = BTreeSet::new();
        for package in &metadata.packages {
            if metadata.workspace_members.contains(&package.id) {
                members.insert(key(package)?);
            }
            let inputs = PackageInputs {
                name: package.name.clone(),
                kinds: package
                    .targets
                    .iter()
                    .map(|target| target.kind.iter().cloned().collect())
                    .collect(),
                dependencies: package
                    .dependencies
                    .iter()
                    .filter_map(|dep| dep.path.as_deref())
                    .map(&original)
                    .collect::<Result<_>>()?,
            };
            if packages.insert(key(package)?, inputs).is_some() {
                return Err("Cargo metadata contains duplicate package identities".into());
            }
        }
        Ok(Self {
            root: original(&metadata.workspace_root)?,
            packages,
            members,
        })
    }
}
