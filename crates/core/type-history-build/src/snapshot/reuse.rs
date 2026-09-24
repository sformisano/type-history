//! One locked Cargo target reused by lifecycle snapshots of a target directory.
//!
//! Only dependency artifacts survive between operations. Captured copies, the
//! isolated Cargo home, and Cargo's state for local packages are removed before
//! capture, so local packages always rebuild from the new copy regardless of
//! file timestamps. A busy lock or an unusable directory selects a private
//! snapshot instead of waiting.

use crate::workspace::CargoMetadata;
use crate::Result;
use std::collections::BTreeSet;
use std::env;
use std::fs::{self, File, TryLockError};
use std::io::{ErrorKind, Result as IoResult};
use std::path::{Path, PathBuf};

/// Setting this to `1` gives every lifecycle operation a private snapshot.
const PRIVATE_SNAPSHOT_ENV: &str = "TYPE_HISTORY_PRIVATE_SNAPSHOT";

pub(super) struct Reusable {
    root: PathBuf,
    lock: File,
}

impl Reusable {
    /// Lock the reusable root inside Cargo's target directory, if available.
    pub(super) fn acquire(metadata: &CargoMetadata) -> Result<Option<Self>> {
        match env::var_os(PRIVATE_SNAPSHOT_ENV) {
            None => {}
            Some(value) if value == "1" => return Ok(None),
            Some(_) => {
                return Err(format!("{PRIVATE_SNAPSHOT_ENV} must be unset or exactly 1").into())
            }
        }
        // Only Cargo creates its target directory, with its backup exclusion.
        if !metadata.target_directory.is_dir() {
            return Ok(None);
        }
        let local = metadata
            .packages
            .iter()
            .filter(|package| package.source.is_none())
            .map(|package| package.name.as_str())
            .collect();
        Ok(Self::prepare(
            &metadata.target_directory.join("type-history-snapshot"),
            &local,
        )
        .unwrap_or(None))
    }

    fn prepare(root: &Path, local: &BTreeSet<&str>) -> IoResult<Option<Self>> {
        fs::create_dir_all(root)?;
        // Cleanup must never follow a link out of the target directory.
        if !fs::symlink_metadata(root)?.is_dir() {
            return Ok(None);
        }
        let root = root.canonicalize()?;
        let lock = File::options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(root.join("lock"))?;
        match lock.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => return Ok(None),
            Err(TryLockError::Error(error)) => return Err(error),
        }
        let owner = Self { root, lock };
        // An interrupted earlier operation can leave its copies behind.
        owner.clear()?;
        owner.forget(local)?;
        Ok(Some(owner))
    }

    pub(super) fn path(&self) -> &Path {
        &self.root
    }

    fn clear(&self) -> IoResult<()> {
        for name in ["tree", "cargo-home"] {
            match fs::remove_dir_all(self.root.join(name)) {
                Err(error) if error.kind() != ErrorKind::NotFound => return Err(error),
                _ => {}
            }
        }
        Ok(())
    }

    // Without its fingerprint, Cargo rebuilds a package even when an earlier
    // copy looks newer; build-script output then starts empty as well.
    fn forget(&self, local: &BTreeSet<&str>) -> IoResult<()> {
        // Host output lives in `target/debug`; `--target` adds `target/<triple>/debug`.
        for entry in read_dir(&self.root.join("target"))? {
            let path = entry?.path();
            for profile in [path.join("debug"), path] {
                for state in [".fingerprint", "build"] {
                    for entry in read_dir(&profile.join(state))? {
                        let entry = entry?;
                        if entry
                            .file_name()
                            .to_str()
                            .is_some_and(|name| owned(name, local))
                        {
                            fs::remove_dir_all(entry.path())?;
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

impl Drop for Reusable {
    fn drop(&mut self) {
        let _ = self.clear();
        // A fork can inherit this open file description; release it explicitly.
        let _ = self.lock.unlock();
    }
}

fn read_dir(path: &Path) -> IoResult<Vec<IoResult<fs::DirEntry>>> {
    match fs::read_dir(path) {
        Ok(entries) => Ok(entries.collect()),
        Err(error) if matches!(error.kind(), ErrorKind::NotFound | ErrorKind::NotADirectory) => {
            Ok(Vec::new())
        }
        Err(error) => Err(error),
    }
}

// Cargo names per-unit state `<package>-<16 hexadecimal digits>`.
fn owned(name: &str, local: &BTreeSet<&str>) -> bool {
    name.rsplit_once('-').is_some_and(|(package, hash)| {
        hash.len() == 16
            && hash.bytes().all(|byte| byte.is_ascii_hexdigit())
            && local.contains(package)
    })
}

#[cfg(test)]
mod tests {
    use super::owned;
    use std::collections::BTreeSet;

    #[test]
    fn only_exact_local_package_state_is_forgotten() {
        let local = BTreeSet::from(["type-history", "consumer"]);
        assert!(owned("type-history-0123456789abcdef", &local));
        assert!(owned("consumer-fedcba9876543210", &local));
        for name in [
            "type-history-core-0123456789abcdef",
            "serde-0123456789abcdef",
            "consumer-0123456789abcde",
            "consumer-0123456789abcdefg",
            "consumer-0123456789abcdez",
            "consumer",
            "lib-consumer",
        ] {
            assert!(!owned(name, &local), "{name}");
        }
    }
}
