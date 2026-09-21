//! Keep the persistent lock inode inside its checked directory.
use crate::Result;
use std::fs::{File, TryLockError};
#[cfg(test)]
use std::io::Result as IoResult;
use std::path::Path;

pub(super) struct Lock(File);

impl Lock {
    pub(super) fn acquire(path: &Path, package: &str) -> Result<Self> {
        let lock = open(path)?;
        match lock.try_lock() {
            Ok(()) => Ok(Self(lock)),
            Err(TryLockError::WouldBlock) => Err(format!(
                "package {package} is busy: another history transaction holds its lock"
            )
            .into()),
            Err(error) => Err(format!("cannot lock package {package}: {error}").into()),
        }
    }

    #[cfg(test)]
    pub(super) fn duplicate(&self) -> IoResult<File> {
        self.0.try_clone()
    }
}

impl Drop for Lock {
    fn drop(&mut self) {
        // A fork can inherit this open file description before CLOEXEC takes
        // effect. Closing only our descriptor would then retain the lock.
        let _ = self.0.unlock();
    }
}

#[cfg(unix)]
fn open(path: &Path) -> Result<File> {
    use rustix::fs::{openat, Mode, OFlags, CWD};

    let parent = path.parent().ok_or("history lock parent")?;
    let name = path.file_name().ok_or("history lock filename")?;
    let directory = openat(
        CWD,
        parent,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )?;
    // NONBLOCK also lets us reject a FIFO without waiting for another endpoint.
    let lock = File::from(
        openat(
            directory,
            name,
            OFlags::RDWR | OFlags::CREATE | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::from_bits_truncate(0o666),
        )
        .map_err(|error| {
            format!(
                "cannot open history lock {} without following symlinks: {error}",
                path.display()
            )
        })?,
    );
    if !lock.metadata()?.is_file() {
        return Err("history lock must be a regular file".into());
    }
    Ok(lock)
}

#[cfg(not(unix))]
fn open(_path: &Path) -> Result<File> {
    Err("history locks require supported Unix filesystem semantics".into())
}
