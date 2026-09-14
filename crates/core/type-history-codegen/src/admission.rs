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

/// One discovered invocation and its full library module path.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Declaration {
    /// Exact source position and history identity.
    pub invocation: Invocation,
    /// Library crate name followed by normalized module names.
    pub module_path: Vec<String>,
    /// Authored current-record alias, resolved by rustc within that module.
    pub rust_name: String,
}

/// Disposable declaration inventory for one successful Cargo build hook.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Admission {
    /// Effective policy selected by the build hook.
    pub strict: bool,
    /// Complete sorted declaration set.
    pub declarations: Vec<Declaration>,
}

impl Admission {
    /// Read the build output and require this actual declaration to be accounted for.
    /// Return effective strictness and the declaration for compiler checking.
    pub fn read(
        path: &Path,
        invocation: &Invocation,
    ) -> Result<(bool, Declaration), Box<dyn Error>> {
        let admission: Self = serde_json::from_slice(&fs::read(path)?)?;
        let declaration = admission.declarations.into_iter()
            .find(|declaration| declaration.invocation == *invocation)
            .filter(|declaration| !declaration.module_path.is_empty())
            .ok_or("unsupported history declaration: use a directly declared module-level record discovered by the build hook")?;
        Ok((admission.strict, declaration))
    }
}

#[cfg(test)]
mod tests;
