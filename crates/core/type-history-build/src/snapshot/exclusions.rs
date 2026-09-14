//! Exclude identified generated paths without excluding same-named source inputs.
use crate::contract::ToolContract;
use crate::workspace::CargoMetadata;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

pub(super) fn owned_paths(
    metadata: &CargoMetadata,
    contract: &ToolContract,
    roots: &[PathBuf],
) -> BTreeSet<PathBuf> {
    let mut paths: BTreeSet<_> = metadata
        .packages
        .iter()
        .filter(|package| package.source.is_none())
        .map(|package| package.root().join(contract.lock_path))
        .collect();
    for root in roots {
        let git = root.join(".git");
        if git_metadata(&git) {
            paths.insert(git);
        }
    }
    paths
}

fn git_metadata(path: &Path) -> bool {
    if path.is_dir() {
        path.join("HEAD").is_file() && path.join("objects").is_dir()
    } else {
        fs::read(path).is_ok_and(|bytes| bytes.starts_with(b"gitdir: "))
    }
}
