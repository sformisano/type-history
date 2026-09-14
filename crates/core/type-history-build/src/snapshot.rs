//! Owned copies of all local Cargo inputs; live source never sees a candidate ledger.
use crate::contract::ToolContract;
use crate::package::workspace_manifest;
use crate::workspace::CargoMetadata;
use crate::Result;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::ffi::OsString;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::Builder as TempBuilder;
use tempfile::TempDir;
use toml::{Table, Value};

mod cargo_config;
mod exclusions;
mod graph;
mod manifest;
#[cfg(test)]
mod tests;

/// Owned immutable capture of the Cargo graph and effective configuration.
pub struct Snapshot {
    _owner: TempDir,
    /// Disposable compiled output owned by this capture.
    pub target: PathBuf,
    /// Isolated Cargo configuration sharing only cache data and locks.
    pub cargo_home: PathBuf,
    manifest: PathBuf,
    configs: Vec<PathBuf>,
    toolchain: Option<OsString>,
    mirror: PathBuf,
    roots: Vec<PathBuf>,
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
impl Snapshot {
    /// Capture and verify all local Cargo inputs before a candidate is written.
    pub fn create(
        metadata: &CargoMetadata,
        extra: &[PathBuf],
        contract: &ToolContract,
    ) -> Result<Self> {
        let owner = TempBuilder::new()
            .prefix("type-history-schema-")
            .tempdir_in(env::temp_dir().canonicalize()?)?;
        let mirror = owner.path().join("tree");
        let mut roots = vec![metadata.workspace_root.canonicalize()?];
        let mut manifests = BTreeSet::from([metadata.workspace_root.join("Cargo.toml")]);
        let mut workspaces = manifests.clone();
        for package in metadata
            .packages
            .iter()
            .filter(|package| package.source.is_none())
        {
            let root = package.root().canonicalize()?;
            manifests.insert(package.manifest_path.clone());
            let document: Value = toml::from_slice(&fs::read(&package.manifest_path)?)?;
            if let Some(workspace) = workspace_manifest(&root, &document)? {
                roots.push(
                    workspace
                        .parent()
                        .ok_or("workspace manifest parent")?
                        .to_owned(),
                );
                workspaces.insert(workspace.clone());
                manifests.insert(workspace);
            } else {
                workspaces.insert(package.manifest_path.clone());
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
        if !roots.iter().any(|root| invocation.starts_with(root)) {
            return Err("Cargo invocation directory is outside the resolved local graph".into());
        }
        let mut extra = extra.to_vec();
        for ancestor in invocation.ancestors() {
            extra.push(ancestor.join("Cargo.toml"));
            for name in ["config", "config.toml"] {
                extra.push(ancestor.join(".cargo").join(name));
            }
        }
        for root in &roots {
            for ancestor in root.ancestors() {
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
        for root in &snapshot.roots {
            snapshot.copy_tree(root)?;
        }
        for path in &snapshot.extra {
            if snapshot.roots.iter().any(|root| path.starts_with(root))
                || snapshot.before.get(path).is_some_and(|value| value == &[0])
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
    fn fingerprint(&self) -> Result<BTreeMap<PathBuf, Vec<u8>>> {
        let mut files = BTreeMap::new();
        for root in &self.roots {
            self.collect(root, &mut files)?;
        }
        for path in &self.extra {
            if files.contains_key(path) {
                continue;
            }
            match fs::read(path) {
                Ok(bytes) => {
                    files.insert(path.clone(), file_fingerprint(&bytes));
                }
                Err(error) if error.kind() == ErrorKind::NotFound => {
                    files.insert(path.clone(), vec![0]);
                }
                Err(error) => return Err(error.into()),
            }
        }
        Ok(files)
    }
    fn collect(&self, path: &Path, files: &mut BTreeMap<PathBuf, Vec<u8>>) -> Result<()> {
        if self.excluded(path) {
            return Ok(());
        }
        let metadata = fs::symlink_metadata(path)?;
        if metadata.file_type().is_symlink() {
            let target = fs::read_link(path)?;
            let resolved = path.canonicalize()?;
            if !self.roots.iter().any(|root| resolved.starts_with(root)) {
                return Err(format!("snapshot cannot preserve source symlink {} outside the resolved local source graph", path.display()).into());
            }
            files.insert(
                path.to_owned(),
                [vec![2], target.as_os_str().as_encoded_bytes().to_vec()].concat(),
            );
        } else if metadata.is_dir() {
            for child in fs::read_dir(path)? {
                self.collect(&child?.path(), files)?;
            }
        } else if metadata.is_file() {
            files.insert(
                path.to_owned(),
                [vec![1], Sha256::digest(fs::read(path)?).to_vec()].concat(),
            );
        } else {
            return Err(format!("snapshot input is not a regular file: {}", path.display()).into());
        }
        Ok(())
    }
    fn excluded(&self, path: &Path) -> bool {
        // Skip only this capture's owner when TMPDIR lies in the workspace.
        // Traversal stops here, before it can visit the growing mirror.
        path == self._owner.path()
            || path.starts_with(&self.excluded_target)
            || self.excluded_caches.contains(path)
            || self.excluded_paths.contains(path)
    }
    fn copy_tree(&self, path: &Path) -> Result<()> {
        if self.excluded(path) {
            return Ok(());
        }
        let metadata = fs::symlink_metadata(path)?;
        if metadata.file_type().is_symlink() {
            let target = fs::read_link(path)?;
            self.verify_captured(
                path,
                &[vec![2], target.as_os_str().as_encoded_bytes().to_vec()].concat(),
            )?;
            let destination = self.mapped(path)?;
            fs::create_dir_all(destination.parent().ok_or("symlink parent")?)?;
            let target = if target.is_absolute() {
                self.mapped(&target)?
            } else {
                target
            };
            #[cfg(unix)]
            std::os::unix::fs::symlink(target, destination)?;
            #[cfg(not(unix))]
            return Err(
                "source symlink snapshots require supported Unix filesystem semantics".into(),
            );
        } else if metadata.is_dir() {
            fs::create_dir_all(self.mapped(path)?)?;
            for child in fs::read_dir(path)? {
                self.copy_tree(&child?.path())?;
            }
        } else {
            self.copy_file(path)?;
        }
        Ok(())
    }
    fn copy_file(&self, path: &Path) -> Result<()> {
        // Read once: the bytes verified here are exactly the bytes written below.
        // Checking only the live file again after copying misses edit/copy/undo.
        let bytes = fs::read(path)?;
        self.verify_captured(path, &file_fingerprint(&bytes))?;
        let destination = self.mapped(path)?;
        fs::create_dir_all(destination.parent().ok_or("snapshot file parent")?)?;
        fs::write(&destination, bytes)?;
        fs::set_permissions(&destination, fs::metadata(path)?.permissions())?;
        Ok(())
    }
    fn verify_captured(&self, path: &Path, actual: &[u8]) -> Result<()> {
        if self.before.get(path).map(Vec::as_slice) != Some(actual) {
            return Err(format!("InputsChanged: copied input {} differs from its captured bytes; retry without editing inputs", path.display()).into());
        }
        Ok(())
    }
    fn verify_complete_copy(&self) -> Result<()> {
        for (path, expected) in &self.before {
            if expected.first() == Some(&1) {
                let copied = fs::read(self.mapped(path)?).map_err(|_| {
                    format!(
                        "InputsChanged: captured input {} is missing from the copied graph",
                        path.display()
                    )
                })?;
                self.verify_captured(path, &file_fingerprint(&copied))?;
            } else if expected.first() == Some(&2) {
                let target = fs::read_link(self.mapped(path)?).map_err(|_| {
                    format!(
                        "InputsChanged: captured symlink {} is missing from the copied graph",
                        path.display()
                    )
                })?;
                // Absolute link targets change only by acquiring the mirror prefix.
                let original =
                    if target.is_absolute() {
                        Path::new("/").join(target.strip_prefix(&self.mirror).map_err(|_| {
                            "InputsChanged: copied symlink points outside the mirror"
                        })?)
                    } else {
                        target
                    };
                self.verify_captured(
                    path,
                    &[vec![2], original.as_os_str().as_encoded_bytes().to_vec()].concat(),
                )?;
            }
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
            if manifest::remap_paths(&mut document, &self.mirror, &self.roots)? || standalone {
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
fn file_fingerprint(bytes: &[u8]) -> Vec<u8> {
    [vec![1], Sha256::digest(bytes).to_vec()].concat()
}
fn mirrored(mirror: &Path, path: &Path) -> Result<PathBuf> {
    Ok(mirror.join(
        path.strip_prefix(Path::new("/"))
            .map_err(|_| "snapshot requires absolute Unix paths")?,
    ))
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
