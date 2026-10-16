//! Fingerprint source identities and verify the exact bytes copied into the mirror.
use super::inputs::lexical_absolute;
use super::Snapshot;
use crate::Result;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

impl Snapshot {
    pub(super) fn fingerprint(&self) -> Result<BTreeMap<PathBuf, Vec<u8>>> {
        let mut files = BTreeMap::new();
        for root in &self.roots {
            self.collect(root, &mut files)?;
        }
        for path in &self.resolution {
            files.insert(path.clone(), entry_fingerprint(path)?);
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
        let metadata = match fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == ErrorKind::NotFound => {
                files.insert(path.to_owned(), vec![0]);
                return Ok(());
            }
            Err(error) => return Err(error.into()),
        };
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
    pub(super) fn copy_resolution(&self) -> Result<()> {
        for path in &self.resolution {
            match self.before.get(path).and_then(|value| value.first()) {
                Some(2) => self.copy_symlink(path)?,
                Some(3) => fs::create_dir_all(self.mapped(path)?)?,
                _ => {}
            }
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
    pub(super) fn copy_tree(&self, path: &Path) -> Result<()> {
        if self.excluded(path) {
            return Ok(());
        }
        let metadata = fs::symlink_metadata(path)?;
        if metadata.file_type().is_symlink() {
            self.copy_symlink(path)?;
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
    fn copy_symlink(&self, path: &Path) -> Result<()> {
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
            let copied_resolution =
                lexical_absolute(&destination.parent().ok_or("symlink parent")?.join(&target))?;
            if copied_resolution.starts_with(&self.mirror) {
                target
            } else {
                return Err(format!(
                    "snapshot relative symlink {} would escape the owned mirror",
                    path.display()
                )
                .into());
            }
        };
        if fs::symlink_metadata(&destination).is_ok() {
            if fs::read_link(&destination).is_ok_and(|existing| existing == target) {
                return Ok(());
            }
            return Err(format!(
                "InputsChanged: copied symlink {} conflicts with the captured graph",
                path.display()
            )
            .into());
        }
        #[cfg(unix)]
        std::os::unix::fs::symlink(target, destination)?;
        #[cfg(not(unix))]
        return Err("source symlink snapshots require supported Unix filesystem semantics".into());
        Ok(())
    }
    pub(super) fn copy_file(&self, path: &Path) -> Result<()> {
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
    pub(super) fn verify_captured(&self, path: &Path, actual: &[u8]) -> Result<()> {
        if self.before.get(path).map(Vec::as_slice) != Some(actual) {
            return Err(format!("InputsChanged: copied input {} differs from its captured bytes; retry without editing inputs", path.display()).into());
        }
        Ok(())
    }
    pub(super) fn verify_complete_copy(&self) -> Result<()> {
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
            } else if expected.first() == Some(&3)
                && !fs::symlink_metadata(self.mapped(path)?).is_ok_and(|metadata| metadata.is_dir())
            {
                return Err(format!(
                    "InputsChanged: captured directory {} is missing from the copied graph",
                    path.display()
                )
                .into());
            }
        }
        Ok(())
    }
}

pub(super) fn file_fingerprint(bytes: &[u8]) -> Vec<u8> {
    [vec![1], Sha256::digest(bytes).to_vec()].concat()
}

pub(super) fn entry_fingerprint(path: &Path) -> Result<Vec<u8>> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(vec![0]),
        Err(error) => return Err(error.into()),
    };
    if metadata.file_type().is_symlink() {
        let target = fs::read_link(path)?;
        Ok([vec![2], target.as_os_str().as_encoded_bytes().to_vec()].concat())
    } else if metadata.is_dir() {
        Ok(vec![3])
    } else if metadata.is_file() {
        Ok(file_fingerprint(&fs::read(path)?))
    } else {
        Err(format!("snapshot input is not a regular file: {}", path.display()).into())
    }
}
