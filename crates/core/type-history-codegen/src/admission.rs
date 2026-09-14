//! Standalone declarations checked by the build hook and matched by the macro.

use serde::{Deserialize, Serialize};
use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
};

/// Compiler signal carrying the complete declaration file's path.
pub const ENV: &str = "TYPE_HISTORY_ADMISSION";

/// Exact authored identifier position, independent of normalized Rust spelling.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Invocation {
    /// Canonical containing file.
    pub path: PathBuf,
    /// One-based line.
    pub line: usize,
    /// Zero-based Unicode character column.
    pub column: usize,
    /// ASCII history identity.
    pub stable_name: String,
}

/// Disposable declaration inventory for one successful Cargo build hook.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Admission {
    /// Effective policy selected by the build hook.
    pub strict: bool,
    /// Complete sorted invocation set.
    pub invocations: Vec<Invocation>,
}

impl Admission {
    /// Read the build output and require this actual declaration to be accounted for.
    pub fn read(path: &Path, invocation: &Invocation) -> Result<Self, Box<dyn Error>> {
        let admission: Self = serde_json::from_slice(&fs::read(path)?)?;
        if !admission.invocations.contains(invocation) {
            return Err("unsupported history declaration: use a directly declared module-level record discovered by the build hook".into());
        }
        Ok(admission)
    }
}

#[cfg(test)]
mod tests;
