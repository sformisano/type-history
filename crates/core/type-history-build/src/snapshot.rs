//! Owned copies of all local Cargo inputs; live source never sees a candidate ledger.
use crate::contract::ToolContract;
use crate::package::workspace_manifest_logical;
use crate::workspace::CargoMetadata;
use crate::Result;
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::Builder as TempBuilder;
use tempfile::TempDir;
use toml::{Table, Value};

mod cargo_config;
mod exclusions;
mod files;
mod graph;
mod inputs;
mod manifest;
mod reuse;
#[cfg(test)]
mod tests;

use files::file_fingerprint;
use inputs::{capture_optional_input, declared_snapshot_inputs, mirrored, resolve_snapshot_input};

/// Owned immutable capture of the Cargo graph and effective configuration.
pub struct Snapshot {
    _owner: Owner,
    /// Compiled output for this capture. [`Snapshot::create`] removes it with the
    /// snapshot. Lifecycle operations normally build in `type-history-snapshot/target`
    /// inside Cargo's target directory and keep only dependency output between them.
    pub target: PathBuf,
    /// Isolated Cargo configuration sharing only cache data and locks.
    pub cargo_home: PathBuf,
    manifest: PathBuf,
    configs: Vec<PathBuf>,
    toolchain: Option<OsString>,
    mirror: PathBuf,
    roots: Vec<PathBuf>,
    resolution: BTreeSet<PathBuf>,
    boundaries: Vec<PathBuf>,
    manifests: BTreeSet<PathBuf>,
    workspaces: BTreeSet<PathBuf>,
    extra: Vec<PathBuf>,
    excluded_target: PathBuf,
    excluded_caches: BTreeSet<PathBuf>,
    excluded_paths: BTreeSet<PathBuf>,
    before: BTreeMap<PathBuf, Vec<u8>>,
    environment: Vec<(OsString, OsString)>,
    compiler: Vec<u8>,
}

// Directory holding the mirror, isolated Cargo home, and compiled output.
enum Owner {
    Private(TempDir),
    Reusable(reuse::Reusable),
}
impl Owner {
    fn private() -> Result<Self> {
        Ok(Self::Private(
            TempBuilder::new()
                .prefix("type-history-schema-")
                .tempdir_in(env::temp_dir().canonicalize()?)?,
        ))
    }
    fn path(&self) -> &Path {
        match self {
            Self::Private(owner) => owner.path(),
            Self::Reusable(owner) => owner.path(),
        }
    }
}

impl Snapshot {
    /// Capture and verify all local Cargo inputs before a candidate is written.
    pub fn create(
        metadata: &CargoMetadata,
        extra: &[PathBuf],
        contract: &ToolContract,
    ) -> Result<Self> {
        Self::capture(metadata, extra, contract, Owner::private()?)
    }

    /// Capture for a lifecycle operation. Dependencies compiled by earlier
    /// operations stay reusable; every local input is copied and rebuilt.
    pub(crate) fn create_reusable(
        metadata: &CargoMetadata,
        extra: &[PathBuf],
        contract: &ToolContract,
    ) -> Result<Self> {
        if let Some(owner) = reuse::Reusable::acquire(metadata)? {
            // A target filesystem the capture cannot use falls back to a private
            // snapshot; input errors recur there and are reported.
            if let Ok(snapshot) = Self::capture(metadata, extra, contract, Owner::Reusable(owner)) {
                return Ok(snapshot);
            }
        }
        Self::capture(metadata, extra, contract, Owner::private()?)
    }

    fn capture(
        metadata: &CargoMetadata,
        extra: &[PathBuf],
        contract: &ToolContract,
        owner: Owner,
    ) -> Result<Self> {
        let mirror = owner.path().join("tree");
        let workspace_root = metadata.workspace_root.canonicalize()?;
        let mut roots = Vec::new();
        let mut resolution = BTreeSet::new();
        let root_manifest_input = resolve_snapshot_input(&workspace_root, Path::new("Cargo.toml"))?;
        let root_manifest = root_manifest_input.resolved;
        resolution.extend(root_manifest_input.resolution);
        let mut boundaries = BTreeSet::from([workspace_root.clone()]);
        let mut manifests = BTreeSet::from([root_manifest.clone()]);
        let mut workspaces = manifests.clone();
        let mut extra = extra.to_vec();
        extra.push(root_manifest.clone());
        capture_optional_input(
            &workspace_root,
            Path::new("Cargo.lock"),
            &mut extra,
            &mut resolution,
        )?;
        for package in metadata
            .packages
            .iter()
            .filter(|package| package.source.is_none())
        {
            let root = package.root().canonicalize()?;
            boundaries.insert(root.clone());
            let manifest = package.manifest_path.canonicalize()?;
            manifests.insert(manifest.clone());
            let document: Value = toml::from_slice(&fs::read(&manifest)?)?;
            for input in declared_snapshot_inputs(&root, &document)? {
                roots.push(input.resolved);
                resolution.extend(input.resolution);
            }
            if let Some(workspace) = workspace_manifest_logical(&root, &document)? {
                let parent = workspace
                    .parent()
                    .ok_or("workspace manifest parent")?
                    .to_owned();
                let name = workspace.file_name().ok_or("workspace manifest name")?;
                let input = resolve_snapshot_input(&parent, Path::new(name))?;
                boundaries.insert(parent.canonicalize()?);
                workspaces.insert(input.resolved.clone());
                manifests.insert(input.resolved.clone());
                extra.push(input.resolved);
                resolution.extend(input.resolution);
                capture_optional_input(
                    &parent,
                    Path::new("Cargo.lock"),
                    &mut extra,
                    &mut resolution,
                )?;
            } else {
                workspaces.insert(manifest);
            }
            roots.push(root);
        }
        roots.sort();
        roots.dedup();
        // Capture caches before folding nested workspace roots into their parent.
        let excluded_caches = cargo_cache_roots(&roots);
        let excluded_paths = exclusions::owned_paths(metadata, contract, &roots);
        let all = roots.clone();
        roots.retain(|path| {
            !all.iter()
                .any(|parent| parent != path && path.starts_with(parent))
        });
        let invocation = env::current_dir()?.canonicalize()?;
        // An explicit workspace member can live outside its workspace directory.
        if !boundaries.iter().any(|root| invocation.starts_with(root)) {
            return Err("Cargo invocation directory is outside the resolved local graph".into());
        }
        for ancestor in invocation.ancestors() {
            extra.push(ancestor.join("Cargo.toml"));
            for name in ["config", "config.toml"] {
                extra.push(ancestor.join(".cargo").join(name));
            }
        }
        for root in boundaries.iter().chain(&roots) {
            let discovery_root = if root.is_dir() {
                root.as_path()
            } else {
                root.parent().ok_or("snapshot input parent")?
            };
            for ancestor in discovery_root.ancestors() {
                // Excluded packages still depend on ancestor membership rules.
                // Track absent manifests too, so a new workspace is detected.
                extra.push(ancestor.join("Cargo.toml"));
                for name in ["config", "config.toml"] {
                    extra.push(ancestor.join(".cargo").join(name));
                }
            }
        }
        let cargo_home = env::var_os("CARGO_HOME")
            .map(PathBuf::from)
            .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".cargo")))
            .ok_or("cannot locate Cargo home for an isolated schema snapshot")?
            .canonicalize()?;
        for name in ["config", "config.toml"] {
            extra.push(cargo_home.join(name));
        }
        extra.sort();
        extra.dedup();
        let mut snapshot = Self {
            target: owner.path().join("target"),
            cargo_home: owner.path().join("cargo-home"),
            manifest: mirrored(&mirror, &metadata.workspace_root.join("Cargo.toml"))?,
            configs: Vec::new(),
            toolchain: crate::toolchain::active()?,
            mirror,
            roots,
            resolution,
            boundaries: boundaries.into_iter().collect(),
            manifests,
            workspaces,
            extra,
            excluded_target: metadata.target_directory.clone(),
            excluded_caches,
            excluded_paths,
            before: BTreeMap::new(),
            environment: environment(),
            compiler: compiler()?,
            _owner: owner,
        };
        snapshot.before = snapshot.fingerprint()?;
        snapshot.configs =
            cargo_config::ordered_configs(&invocation, &cargo_home, &snapshot.before);
        snapshot.copy_resolution()?;
        for root in &snapshot.roots {
            snapshot.copy_tree(root)?;
        }
        for path in &snapshot.extra {
            if snapshot.roots.iter().any(|root| path.starts_with(root))
                || snapshot.before.get(path).is_some_and(|value| value == &[0])
                || snapshot.before.get(path).and_then(|value| value.first()) == Some(&2)
            {
                continue;
            }
            // Inputs outside the source graph are individual files, including
            // global Cargo configs whose symlink targets live in a dotfiles repo.
            snapshot.copy_file(path)?;
        }
        snapshot.verify_complete_copy()?;
        snapshot.check_config_preservation()?;
        cargo_config::share_caches(&cargo_home, &snapshot.cargo_home)?;
        snapshot.remap_manifests()?;
        snapshot.ensure_fresh()?;
        Ok(snapshot)
    }
    /// Resolve one absolute captured path inside the owned mirror.
    pub fn mapped(&self, original: &Path) -> Result<PathBuf> {
        mirrored(&self.mirror, original)
    }

    /// Map a captured absolute path back to its original diagnostic location.
    pub fn original(&self, captured: &Path) -> Option<PathBuf> {
        captured
            .strip_prefix(&self.mirror)
            .ok()
            .map(|relative| Path::new("/").join(relative))
    }

    /// Cargo reports relative compiler paths from this captured workspace.
    pub(crate) fn workspace_root(&self) -> &Path {
        self.manifest.parent().expect("absolute workspace manifest")
    }
    /// Append checked Cargo inputs after the subcommand and before test arguments.
    pub fn configure_cargo(&self, command: &mut Command) -> Result<()> {
        self.configure_cargo_at(command, &self.manifest)
    }
    pub(crate) fn configure_cargo_at(&self, command: &mut Command, manifest: &Path) -> Result<()> {
        cargo_config::check_root()?;
        command
            .current_dir("/")
            .env("CARGO_HOME", &self.cargo_home)
            .arg("--manifest-path")
            .arg(manifest);
        if let Some(toolchain) = &self.toolchain {
            command.env("RUSTUP_TOOLCHAIN", toolchain);
        }
        for config in &self.configs {
            command.arg("--config").arg(self.mapped(config)?);
        }
        Ok(())
    }
    /// Reject changes to captured bytes, relevant environment, or compiler.
    pub fn ensure_fresh(&self) -> Result<()> {
        self.ensure_fresh_ignoring(None)
    }
    /// Check freshness while ignoring the transaction's staged candidate.
    pub fn ensure_fresh_ignoring(&self, staged: Option<&Path>) -> Result<()> {
        let mut current = self.fingerprint()?;
        if let Some(path) = staged {
            current.remove(path);
        }
        if self.before != current
            || self.environment != environment()
            || self.compiler != compiler()?
        {
            return Err("InputsChanged: source, manifests, ledger, Cargo configuration, environment or compiler changed; retry without editing inputs".into());
        }
        Ok(())
    }
    fn check_config_preservation(&self) -> Result<()> {
        cargo_config::check_environment(Path::new("<environment>"), b"", &self.environment)?;
        for path in &self.configs {
            let copied = self.mapped(path)?;
            if !copied.is_file() {
                continue;
            }
            let bytes = fs::read(&copied)?;
            // A config may itself be a captured link. Verify its resolved copied
            // file without replacing the link's separate identity fingerprint.
            let resolved = copied.canonicalize()?;
            let original = Path::new("/").join(resolved.strip_prefix(&self.mirror)?);
            self.verify_captured(&original, &file_fingerprint(&bytes))?;
            cargo_config::validate(path, &bytes, &self.roots, &self.mirror)?;
            cargo_config::check_environment(path, &bytes, &self.environment)?;
        }
        cargo_config::check_root()?;
        Ok(())
    }
    fn remap_manifests(&self) -> Result<()> {
        for path in &self.manifests {
            let copied = self.mapped(path)?;
            if !copied.is_file() {
                continue;
            }
            let text = fs::read_to_string(&copied)?;
            let mut document: Value = toml::from_str(&text)?;
            // Cargo can otherwise walk out of the mirror into a live workspace
            // above TMPDIR. Make every resolved standalone boundary explicit.
            let standalone = self.workspaces.contains(path) && document.get("workspace").is_none();
            if standalone {
                document
                    .as_table_mut()
                    .ok_or("manifest must be a TOML table")?
                    .insert("workspace".into(), Value::Table(Table::new()));
            }
            if manifest::remap_paths(&mut document, &self.mirror, &self.boundaries)? || standalone {
                let staged = copied.with_extension("toml.type-history-stage");
                fs::write(&staged, toml::to_string(&document)?)?;
                fs::rename(staged, copied)?;
            }
        }
        Ok(())
    }
}

fn cargo_cache_roots(roots: &[PathBuf]) -> BTreeSet<PathBuf> {
    const SIGNATURE: &[u8] = b"Signature: 8a477f597d28d172789f06886806bc55\n";
    roots
        .iter()
        .map(|root| root.join("target"))
        .filter(|path| {
            fs::read(path.join("CACHEDIR.TAG")).is_ok_and(|tag| tag.starts_with(SIGNATURE))
        })
        .collect()
}
fn environment() -> Vec<(OsString, OsString)> {
    let mut result: Vec<_> = env::vars_os()
        .filter(|(name, _)| {
            let name = name.to_string_lossy();
            name.starts_with("CARGO")
                || name.starts_with("RUST")
                || name.starts_with("TYPE_HISTORY")
                || matches!(name.as_ref(), "PATH" | "HOME")
        })
        .collect();
    result.sort();
    result
}
fn compiler() -> Result<Vec<u8>> {
    let output = Command::new("rustc").arg("-vV").output()?;
    if !output.status.success() {
        return Err("cannot identify rustc for snapshot freshness".into());
    }
    Ok(output.stdout)
}
