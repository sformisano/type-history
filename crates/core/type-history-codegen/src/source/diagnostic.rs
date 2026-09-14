//! Attributable source-resolution failures with supported rewrites.

use std::{io::Error as IoError, path::PathBuf};

use syn::Error;

/// Attributable source-resolution failure with a supported rewrite.
#[derive(Debug, thiserror::Error)]
pub enum SourceDiagnostic {
    /// Filesystem failure.
    #[error("cannot read schema source `{path}`")]
    Io {
        /// Affected path.
        path: PathBuf,
        /// Filesystem cause.
        #[source]
        source: IoError,
    },
    /// Source was not UTF-8.
    #[error("schema source `{path}` is not UTF-8; write ordinary UTF-8 Rust source")]
    NonUtf8 {
        /// Affected path.
        path: PathBuf,
    },
    /// Rust syntax could not be parsed.
    #[error("cannot parse schema source `{path}`: {source}")]
    Parse {
        /// Affected path.
        path: PathBuf,
        /// Parser cause.
        #[source]
        source: Error,
    },
    /// A standalone declaration could not be lowered by the shared parser.
    #[error("cannot compile history declaration in `{path}`: {source}")]
    Declaration {
        /// Affected source path.
        path: PathBuf,
        /// Shared parser cause.
        #[source]
        source: Error,
    },
    /// Module graph cycle.
    #[error("schema module graph cycles through `{path}`; make modules finite and direct")]
    ModuleCycle {
        /// Cyclic path.
        path: PathBuf,
    },
    /// Invalid literal path.
    #[error(
        "module `{module}` is unsupported: {reason}; use a package-local relative string literal"
    )]
    InvalidModulePath {
        /// Module name.
        module: String,
        /// Rejection reason.
        reason: &'static str,
    },
    /// Module path escaped package.
    #[error("schema module `{path}` escapes the package; move it under the package root")]
    EscapingModule {
        /// Escaping path.
        path: PathBuf,
    },
    /// Both conventional module candidates exist.
    #[error("module `{module}` is ambiguous between `{first}` and `{second}`; keep exactly one")]
    AmbiguousModule {
        /// Module name.
        module: String,
        /// Flat candidate.
        first: PathBuf,
        /// Nested candidate.
        second: PathBuf,
    },
    /// No conventional module candidate exists.
    #[error(
        "module `{module}` is missing (`{first}` or `{second}`); write one ordinary source file"
    )]
    MissingModule {
        /// Module name.
        module: String,
        /// Flat candidate.
        first: PathBuf,
        /// Nested candidate.
        second: PathBuf,
    },
    /// Shape-changing configuration.
    #[error("conditional {construct} in `{path}` can change persisted shape; keep the schema unconditional")]
    ConditionalSchema {
        /// Conditional construct.
        construct: &'static str,
        /// Affected path.
        path: PathBuf,
    },
    /// A schema entrypoint was visible only through a glob.
    #[error("schema entrypoint `{name}` resolves only through a glob; rewrite as `{rewrite}`")]
    GlobEntrypoint {
        /// Entrypoint name.
        name: String,
        /// Supported rewrite.
        rewrite: String,
    },
    /// Two imports bind one schema name.
    #[error("schema import `{name}` is ambiguous; keep one explicit import or rename")]
    AmbiguousImport {
        /// Ambiguous binding.
        name: String,
    },
    /// Explicit import/re-export chain was cyclic.
    #[error("schema import/re-export chain cycles through `{path}`; make the chain finite")]
    ImportCycle {
        /// Cyclic symbol path.
        path: String,
    },
    /// A `super` chain escaped the crate root.
    #[error("schema import `{path}` escapes the crate root; use a finite crate-local path")]
    EscapingImport {
        /// Rejected import path.
        path: String,
    },
    /// cfg_attr syntax was not auditable.
    #[error(
        "cfg_attr on a schema surface is not auditable; keep only documentation or lint attributes"
    )]
    InvalidCfgAttr,
    /// Project source occupied a namespace reserved for generated authority.
    #[error("project item `{name}` in `{path}` uses a generated reserved prefix")]
    ReservedGeneratedName {
        /// Conflicting project-authored identifier.
        name: String,
        /// Source file containing it.
        path: PathBuf,
    },
}
