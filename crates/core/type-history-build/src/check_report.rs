//! Stable version-one reports for read-only history checks.
use crate::Result;
use serde::Serialize;
use serde_json::Value;
use std::fmt::{Display, Error as FmtError, Formatter, Result as FmtResult};
use type_history_codegen::diagnostics::{
    readable_path, DifferenceCode, PathSegment, SchemaDifference,
};

/// Origin of a check finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    /// Independent release authority.
    ReleasedBaseline,
    /// Current committed authority.
    CurrentLedger,
    /// Compiler-resolved source.
    Compiler,
}
/// Stable non-shape failure codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckCode {
    /// A retained history disappeared.
    HistoryMissing,
    /// A retained version disappeared.
    VersionMissing,
    /// Equality-protected metadata changed.
    MetadataChanged,
    /// A release or its current entry has a reset reservation.
    ReleasedReset,
    /// Input could not be selected, parsed, or captured.
    InputInvalid,
    /// Cargo or the compiler failed.
    CompilerFailed,
    /// Captured inputs changed during the check.
    SnapshotStale,
}
/// Typed stable code for either schema or operational findings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum DiagnosticCode {
    /// Recursive schema difference.
    Shape(DifferenceCode),
    /// Authority or operational failure.
    Check(CheckCode),
}
impl Display for DiagnosticCode {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        let value = serde_json::to_value(self).map_err(|_| FmtError)?;
        f.write_str(value.as_str().ok_or(FmtError)?)
    }
}
/// Attributable original source or authority location.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct Location {
    /// Original file path.
    pub file: String,
    /// One-based line when known.
    pub line: Option<u64>,
    /// One-based column when known.
    pub column: Option<u64>,
}
/// One actionable finding. Null values mean absent or unavailable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Diagnostic {
    /// Stable machine code.
    pub code: DiagnosticCode,
    /// Compared authority or compiler source.
    pub origin: Origin,
    /// Exact history when identifiable.
    pub stable_name: Option<String>,
    /// Exact version when identifiable.
    pub version: Option<u32>,
    /// Complete typed nested path.
    pub path: Vec<PathSegment>,
    /// Expected schema descriptor, metadata, or scalar.
    pub expected: Value,
    /// Actual schema descriptor, metadata, or scalar.
    pub actual: Value,
    /// Explanation and correction guidance.
    pub hint: String,
    /// Closest attributable original location.
    pub location: Option<Location>,
}
impl Diagnostic {
    pub(crate) fn operational(code: CheckCode, origin: Origin, hint: impl Into<String>) -> Self {
        Self {
            code: DiagnosticCode::Check(code),
            origin,
            stable_name: None,
            version: None,
            path: Vec::new(),
            expected: Value::Null,
            actual: Value::Null,
            hint: hint.into(),
            location: None,
        }
    }
    pub(crate) fn shape(
        difference: SchemaDifference,
        origin: Origin,
        name: &str,
        version: u32,
        location: Option<Location>,
    ) -> Self {
        Self {
            code: DiagnosticCode::Shape(difference.code),
            origin,
            stable_name: Some(name.into()),
            version: Some(version),
            path: difference.path,
            expected: difference.expected,
            actual: difference.actual,
            hint: difference.hint,
            location,
        }
    }
}
/// Findings from one selected package.
#[derive(Debug, Clone, Serialize)]
pub struct PackageReport {
    /// Cargo package name.
    pub package: String,
    /// Deterministically ordered independent findings.
    pub diagnostics: Vec<Diagnostic>,
}
/// Complete JSON check envelope.
#[derive(Debug, Clone, Serialize)]
pub struct CheckReport {
    /// Report contract version.
    pub schema_version: u32,
    /// Always `check`.
    pub command: &'static str,
    /// Whether every requested check succeeded.
    pub ok: bool,
    /// Selected packages in package-name order.
    pub packages: Vec<PackageReport>,
    /// Invocation-wide failures.
    pub errors: Vec<Diagnostic>,
}
impl Default for CheckReport {
    fn default() -> Self {
        Self {
            schema_version: 1,
            command: "check",
            ok: true,
            packages: Vec::new(),
            errors: Vec::new(),
        }
    }
}
impl CheckReport {
    pub(crate) fn finish(&mut self) {
        self.packages.sort_by(|a, b| a.package.cmp(&b.package));
        for package in &mut self.packages {
            order(&mut package.diagnostics);
        }
        order(&mut self.errors);
        self.ok = self.errors.is_empty() && self.packages.iter().all(|p| p.diagnostics.is_empty());
    }
    pub(crate) fn render(&self, json: bool) -> Result<()> {
        if json {
            println!("{}", serde_json::to_string(self)?);
        } else {
            for package in &self.packages {
                if package.diagnostics.is_empty() {
                    println!(
                        "{}: frozen history schemas match; ordinary development drafts remain drafts",
                        package.package
                    );
                }
                for finding in &package.diagnostics {
                    eprintln!("{}: {}", package.package, render_finding(finding));
                }
            }
            for finding in &self.errors {
                eprintln!("{}", render_finding(finding));
            }
        }
        Ok(())
    }
}
pub(crate) fn order(findings: &mut Vec<Diagnostic>) {
    findings.sort_by(|a, b| {
        (
            &a.stable_name,
            a.version,
            a.origin,
            &a.path,
            a.code.to_string(),
            a.expected.to_string(),
            a.actual.to_string(),
            &a.hint,
            &a.location,
        )
            .cmp(&(
                &b.stable_name,
                b.version,
                b.origin,
                &b.path,
                b.code.to_string(),
                b.expected.to_string(),
                b.actual.to_string(),
                &b.hint,
                &b.location,
            ))
    });
    findings.dedup_by(|a, b| {
        a.code == b.code
            && a.origin == b.origin
            && a.stable_name == b.stable_name
            && a.version == b.version
            && a.path == b.path
            && a.expected == b.expected
            && a.actual == b.actual
            && (matches!(a.code, DiagnosticCode::Shape(_))
                || (a.hint == b.hint && a.location == b.location))
    });
}
fn render_finding(d: &Diagnostic) -> String {
    format!(
        "{} {} V{} {}: expected {}, actual {}; {}",
        d.code,
        d.stable_name.as_deref().unwrap_or("<unknown>"),
        d.version
            .map(|v| v.to_string())
            .unwrap_or_else(|| "?".into()),
        readable_path(&d.path),
        d.expected,
        d.actual,
        d.hint
    )
}
