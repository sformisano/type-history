//! Shared finite source graph and frontend declaration selection.

mod diagnostic;
mod graph;
mod imports;
mod modules;
pub mod standalone;

pub use diagnostic::SourceDiagnostic;
pub use graph::{
    has_shape_cfg, path_segments, reject_shape_cfg, source_ident, SourceGraph, SourceUnit,
};
pub use imports::ImportIndex;

use std::path::{Path, PathBuf};

/// A source path relative to its package root when contained there.
pub fn relative(root: &Path, path: &Path) -> PathBuf {
    modules::relative(root, path)
}
