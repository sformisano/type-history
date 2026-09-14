//! Decode Cargo's compiler messages without changing compilation success.
use crate::check_report::{CheckCode, Diagnostic, Location, Origin};
use crate::snapshot::Snapshot;
use serde_json::Value;
use std::error::Error;
use std::fmt::{Display, Formatter, Result as FmtResult};
use std::path::Path;
use type_history_codegen::diagnostics::{compare_shapes, parse_compiler_marker};

#[derive(Debug)]
pub(crate) struct CompilerFailure {
    pub diagnostics: Vec<Diagnostic>,
    pub message: String,
}
impl Display for CompilerFailure {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.write_str(&self.message)
    }
}
impl Error for CompilerFailure {}

pub(crate) fn decode(output: &[u8], snapshot: &Snapshot) -> Vec<Diagnostic> {
    let mut result = Vec::new();
    for line in output.split(|byte| *byte == b'\n') {
        let Ok(row) = serde_json::from_slice::<Value>(line) else {
            continue;
        };
        if row["reason"] != "compiler-message" {
            continue;
        }
        let message = &row["message"];
        if let Some(rendered) = message["rendered"].as_str() {
            eprint!("{rendered}");
        }
        if message["level"] != "error" {
            continue;
        }
        let location = source_location(message, snapshot, snapshot.workspace_root());
        if !collect_markers(message, &location, &mut result) {
            let mut finding = Diagnostic::operational(
                CheckCode::CompilerFailed,
                Origin::Compiler,
                message["message"].as_str().unwrap_or("compiler failed"),
            );
            finding.location = location;
            result.push(finding);
        }
    }
    result
}
fn collect_markers(
    message: &Value,
    location: &Option<Location>,
    findings: &mut Vec<Diagnostic>,
) -> bool {
    let mut found = false;
    if let Some(text) = message["message"].as_str() {
        match parse_compiler_marker(text) {
            Ok(Some(observation)) => {
                found = true;
                let differences = compare_shapes(&observation.expected, &observation.actual);
                if differences.is_empty() {
                    let mut finding = Diagnostic::operational(
                        CheckCode::CompilerFailed,
                        Origin::Compiler,
                        "compiler assertion failed despite an equal diagnostic observation; inspect compiler output",
                    );
                    finding.location = location.clone();
                    findings.push(finding);
                }
                for difference in differences {
                    findings.push(Diagnostic::shape(
                        difference,
                        Origin::Compiler,
                        &observation.stable_name,
                        observation.version,
                        location.clone(),
                    ));
                }
            }
            Err(error) => {
                found = true;
                let mut finding = Diagnostic::operational(
                    CheckCode::CompilerFailed,
                    Origin::Compiler,
                    format!("invalid compiler schema diagnostic: {error}"),
                );
                finding.location = location.clone();
                findings.push(finding);
            }
            Ok(None) => {}
        }
    }
    if let Some(children) = message["children"].as_array() {
        for child in children {
            found |= collect_markers(child, location, findings);
        }
    }
    found
}
fn source_location(message: &Value, snapshot: &Snapshot, root: &Path) -> Option<Location> {
    let spans = message["spans"].as_array()?;
    for primary in [true, false] {
        for span in spans {
            if span["is_primary"].as_bool().unwrap_or(false) != primary {
                continue;
            }
            if let Some(location) = span_location(span, snapshot, root) {
                return Some(location);
            }
        }
    }
    None
}
fn span_location(span: &Value, snapshot: &Snapshot, root: &Path) -> Option<Location> {
    // Macro expansion callsites are nearer to the user's declaration than core panic machinery.
    if let Some(expansion) = span.get("expansion").filter(|value| !value.is_null()) {
        if let Some(location) = span_location(&expansion["span"], snapshot, root) {
            return Some(location);
        }
    }
    let file = Path::new(span["file_name"].as_str()?);
    let captured = if file.is_absolute() {
        file.to_owned()
    } else {
        root.join(file)
    };
    let original = snapshot.original(&captured)?;
    // Do not turn virtual compiler paths into fabricated source locations.
    if !captured.is_file() {
        return None;
    }
    Some(Location {
        file: original.to_string_lossy().into_owned(),
        line: span["line_start"].as_u64(),
        column: span["column_start"].as_u64(),
    })
}
