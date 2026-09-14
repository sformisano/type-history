//! Package lock and one truthful atomic ledger commit.
use crate::contract::ToolContract;
use crate::snapshot::Snapshot;
use crate::Result;
use serde::{de::DeserializeOwned, Serialize};
use sha2::{Digest, Sha256};
#[cfg(test)]
use std::cell::Cell;
use std::fs::{self, File, OpenOptions, TryLockError};
#[cfg(test)]
use std::io::Error as IoError;
use std::io::{ErrorKind, Result as IoResult, Write};
use std::path::{Path, PathBuf};
use tempfile::Builder as TempBuilder;
use type_history_codegen::ledger::{HistoryLedger, SchemaIdentity};

pub(crate) struct Transaction {
    _lock: File,
    pub path: PathBuf,
    original: Option<Vec<u8>>,
}
impl Transaction {
    pub fn acquire(package_root: &Path, package: &str, contract: &ToolContract) -> Result<Self> {
        let path = package_root.join(contract.ledger_path);
        let directory = path.parent().ok_or("history ledger parent")?;
        if fs::symlink_metadata(directory).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
            return Err("history authority directory must not be a symlink; keep it inside its owning package".into());
        }
        fs::create_dir_all(directory)?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(package_root.join(contract.lock_path))?;
        match lock.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => {
                return Err(format!(
                    "package {package} is busy: another history transaction holds its lock"
                )
                .into());
            }
            Err(error) => return Err(format!("cannot lock package {package}: {error}").into()),
        }
        if fs::symlink_metadata(&path).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
            return Err("history authority must be a regular file, not a symlink".into());
        }
        let original = read_optional(&path)?;
        Ok(Self {
            _lock: lock,
            path,
            original,
        })
    }
    pub fn original(&self) -> Option<&[u8]> {
        self.original.as_deref()
    }
    pub fn baseline<M: Clone + Eq + Serialize + DeserializeOwned>(
        &self,
        schema_identity: SchemaIdentity,
    ) -> Result<HistoryLedger<M>> {
        HistoryLedger::parse(self.original.as_deref().ok_or(
            "missing history authority; restore its committed ledger, or use explicit init/import",
        )?, schema_identity)
        .map_err(Into::into)
    }
    pub fn commit<M: Clone + Eq + Serialize + DeserializeOwned>(
        &self,
        candidate: &HistoryLedger<M>,
        snapshot: &Snapshot,
    ) -> Result<()> {
        self.commit_checked(candidate, |staged| {
            snapshot.ensure_fresh_ignoring(Some(staged))
        })
    }
    fn commit_checked<M: Clone + Eq + Serialize + DeserializeOwned>(
        &self,
        candidate: &HistoryLedger<M>,
        freshness: impl FnOnce(&Path) -> Result<()>,
    ) -> Result<()> {
        let bytes = encoded(candidate)?;
        let parent = self.path.parent().ok_or("ledger parent")?;
        let mut staged = TempBuilder::new()
            .prefix(".history-candidate-")
            .tempfile_in(parent)?;
        if let Ok(metadata) = fs::metadata(&self.path) {
            staged.as_file().set_permissions(metadata.permissions())?;
        }
        io_boundary(CommitOperation::Write)?;
        staged.write_all(&bytes)?;
        io_boundary(CommitOperation::Flush)?;
        staged.as_file().sync_all()?;
        freshness(staged.path())?;
        if read_optional(&self.path)? != self.original {
            return Err("InputsChanged: live history ledger changed before commit".into());
        }
        // This rename is the commit point. No compiler work follows it.
        io_boundary(CommitOperation::Rename)?;
        if self.original.is_none() {
            staged.persist_noclobber(&self.path)
        } else {
            staged.persist(&self.path)
        }
        .map_err(|error| format!("history ledger rename failed before commit: {error}"))?;
        let committed = io_boundary(CommitOperation::Readback)
            .and_then(|()| fs::read(&self.path))
            .map_err(|error| {
                format!(
                    "COMMITTED: ledger replaced but readback failed; inspect {}: {error}",
                    self.path.display()
                )
            })?;
        if Sha256::digest(&committed) != Sha256::digest(&bytes) {
            return Err(format!(
                "COMMITTED: ledger readback differs; inspect {} before retrying",
                self.path.display()
            )
            .into());
        }
        io_boundary(CommitOperation::DirectorySync).and_then(|()| File::open(parent)).and_then(|directory| directory.sync_all()).map_err(|error| format!("COMMITTED: ledger readback passed but directory durability failed; inspect {}: {error}", self.path.display()))?;
        Ok(())
    }
}
pub(crate) fn encoded<M: Clone + Eq + Serialize + DeserializeOwned>(
    baseline: &HistoryLedger<M>,
) -> Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec_pretty(baseline.entries())?;
    bytes.push(b'\n');
    Ok(bytes)
}
fn read_optional(path: &Path) -> Result<Option<Vec<u8>>> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

// The test-only fault state is never selected by a feature or environment variable.
// Tests execute the production commit path and fail at an actual I/O boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CommitOperation {
    Write,
    Flush,
    Rename,
    Readback,
    DirectorySync,
}
#[cfg(test)]
std::thread_local! {
    static IO_FAULT: Cell<Option<CommitOperation>> = const { Cell::new(None) };
}
fn io_boundary(operation: CommitOperation) -> IoResult<()> {
    #[cfg(test)]
    if IO_FAULT.with(|fault| fault.get() == Some(operation)) {
        return Err(IoError::other(format!("injected {operation:?} failure")));
    }
    let _ = operation;
    Ok(())
}
#[cfg(test)]
mod tests;
