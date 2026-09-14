//! Exact package and lifecycle selectors.
use crate::contract::ToolContract;
use crate::Result;
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::Command;

/// Ledger operation selected by a lifecycle CLI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Compare the frozen ledger with compiler-exported schemas without writing it.
    Check,
    /// Initialize an empty package ledger.
    Init,
    /// Freeze the selected current draft shapes.
    Freeze,
    /// Reserve or restore the exact highest frozen version.
    Reset,
    /// Validate and install an explicit complete retained baseline.
    Import,
}
/// Validated package, history, and Cargo selectors.
#[derive(Debug)]
pub struct LifecycleOptions {
    /// Ledger operation.
    pub action: Action,
    /// One exact workspace package, required for mutations.
    pub package: Option<String>,
    /// One exact stable name and positive version.
    pub selected: Option<(String, u32)>,
    /// Whether to close a reset by restoring its frozen shape.
    pub undo: bool,
    /// Explicit import file.
    pub from: Option<PathBuf>,
    /// Cargo feature selection.
    pub features: Option<String>,
    /// Disable Cargo default features for metadata, compiler export, and validation.
    ///
    /// Set this to `false` to keep Cargo's default features, or `true` to pass
    /// `--no-default-features` to each Cargo command.
    pub no_default_features: bool,
    /// Installed target triple; custom files are rejected.
    pub target: Option<String>,
}

#[derive(Clone, Copy)]
pub(crate) struct FeatureSelection<'a> {
    no_default_features: bool,
    features: Option<&'a str>,
}

impl<'a> FeatureSelection<'a> {
    pub(crate) fn with_defaults(features: Option<&'a str>) -> Self {
        Self {
            no_default_features: false,
            features,
        }
    }

    pub(crate) fn from_options(options: &'a LifecycleOptions) -> Self {
        Self {
            no_default_features: options.no_default_features,
            features: options.features.as_deref(),
        }
    }

    pub(crate) fn apply_to(self, command: &mut Command) {
        if self.no_default_features {
            command.arg("--no-default-features");
        }
        if let Some(features) = self.features {
            command.args(["--features", features]);
        }
    }
}

/// Describe the frontend's fixed command and selector spelling.
pub fn usage(contract: &ToolContract) -> String {
    format!(
        "cargo {} <init|freeze|check|reset|import> --package NAME\nfreeze/reset: --{} EXACT_NAME --version N; reset: --undo\nimport: --from FILE\ncheck: --released-baseline FILE (requires --package NAME); --format json\nCargo selection: --no-default-features; --features FEATURES; --target TRIPLE\ncheck without --package checks initialized workspace libraries read-only",
        contract.command, contract.selector
    )
}

/// Parse exact selectors and reject unsupported operation combinations.
pub fn parse(
    arguments: impl IntoIterator<Item = OsString>,
    contract: &ToolContract,
) -> Result<Option<LifecycleOptions>> {
    let usage = usage(contract);
    let selector = format!("--{}", contract.selector);
    let mut args = arguments
        .into_iter()
        .map(|value| {
            value
                .into_string()
                .map_err(|_| "arguments must be UTF-8".into())
        })
        .collect::<Result<Vec<String>>>()?;
    if args.first().is_some_and(|value| value == contract.command) {
        args.remove(0);
    }
    if args
        .first()
        .is_some_and(|value| matches!(value.as_str(), "--help" | "-h"))
    {
        println!("{usage}");
        return Ok(None);
    }
    let action = match args.first().map(String::as_str) {
        Some("init") => Action::Init, Some("freeze") => Action::Freeze,
        Some("check") => Action::Check, Some("reset") => Action::Reset,
        Some("import") => Action::Import,
        Some("update") => return Err(format!("cargo {} update was removed; use cargo {} freeze --package NAME (or explicit init/import for a missing ledger)", contract.command, contract.command).into()),
        _ => return Err(usage.into()),
    };
    let mut options = LifecycleOptions {
        action,
        package: None,
        selected: None,
        undo: false,
        from: None,
        features: None,
        no_default_features: false,
        target: None,
    };
    let (mut stable_name, mut version) = (None, None);
    let mut args = args.into_iter().skip(1);
    while let Some(flag) = args.next() {
        if flag == "--undo" {
            if options.undo {
                return Err("duplicate --undo".into());
            }
            options.undo = true;
            continue;
        }
        if flag == "--no-default-features" {
            if options.no_default_features {
                return Err("duplicate --no-default-features".into());
            }
            options.no_default_features = true;
            continue;
        }
        let value = args
            .next()
            .ok_or_else(|| format!("missing value for {flag}"))?;
        match flag.as_str() {
            "--package" | "-p" => set(&mut options.package, value, "package")?,
            flag if flag == selector => {
                exact_stable_name(&value)?;
                set(&mut stable_name, value, contract.selector)?;
            }
            "--version" => set(&mut version, positive(&value)?, "version")?,
            "--from" => set(&mut options.from, PathBuf::from(value), "from")?,
            "--features" => set(&mut options.features, value, "features")?,
            "--target" => {
                named_target(&value)?;
                set(&mut options.target, value, "target")?;
            }
            _ => return Err(format!("unknown option {flag}; {usage}").into()),
        }
    }
    options.selected = match (stable_name, version) {
        (Some(stable_name), Some(version)) => Some((stable_name, version)),
        (None, None) => None,
        _ => return Err(format!("{selector} and --version must be supplied together").into()),
    };
    if action != Action::Check && options.package.is_none() {
        return Err("mutation requires one explicit --package NAME".into());
    }
    if options
        .package
        .as_deref()
        .is_some_and(|name| name.is_empty() || name.contains(['*', '?', '[', ']']))
    {
        return Err("package must be one exact name".into());
    }
    if action == Action::Reset && options.selected.is_none() {
        return Err(format!("reset requires {selector} and --version").into());
    }
    if options.selected.is_some() && !matches!(action, Action::Freeze | Action::Reset) {
        return Err("history selection is only valid for freeze/reset".into());
    }
    if options.undo && action != Action::Reset {
        return Err("--undo is only valid for reset".into());
    }
    if options.from.is_some() && action != Action::Import {
        return Err("--from is only valid for import".into());
    }
    if action == Action::Import && options.from.is_none() {
        return Err("import requires --from FILE".into());
    }
    Ok(Some(options))
}
fn set<T>(slot: &mut Option<T>, value: T, name: &str) -> Result<()> {
    if slot.replace(value).is_some() {
        return Err(format!("duplicate --{name}").into());
    }
    Ok(())
}
fn positive(value: &str) -> Result<u32> {
    let result = value.parse::<u32>()?;
    if result == 0 || result.to_string() != value {
        return Err("versions must be canonical positive u32 integers".into());
    }
    Ok(result)
}
fn exact_stable_name(value: &str) -> Result<()> {
    if value.is_empty() || value.contains(['*', '?', '[', ']']) {
        return Err("history stable name must be exact; wildcards are not accepted".into());
    }
    Ok(())
}

/// Lifecycle snapshots accept named targets; custom specification files are not
/// captured target inputs and must never be read from the live filesystem.
pub(crate) fn named_target(value: &str) -> Result<()> {
    if value.is_empty() || value.contains(['/', '\\']) || value.ends_with(".json") {
        return Err("custom target specifications are not supported by history snapshots; use an installed target triple".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{parse, Action};
    use crate::contract::STANDALONE;
    use std::ffi::OsString;

    #[test]
    fn command_and_selector_spellings_are_exact() {
        let contract = &STANDALONE;
        let selector = format!("--{}", contract.selector);
        let options = parse(
            [
                contract.command,
                "freeze",
                "-p",
                "billing",
                &selector,
                "billing.invoice.issued",
                "--version",
                "2",
            ]
            .into_iter()
            .map(OsString::from),
            contract,
        )
        .unwrap()
        .unwrap();
        assert_eq!(options.action, Action::Freeze);
        assert_eq!(options.selected, Some(("billing.invoice.issued".into(), 2)));
        let wrong = "--unknown";
        assert!(parse(
            [
                "freeze",
                "-p",
                "billing",
                wrong,
                "billing.invoice.issued",
                "--version",
                "2",
            ]
            .into_iter()
            .map(OsString::from),
            contract
        )
        .is_err());
    }
}

/// Rendering requested for a read-only check.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum OutputFormat {
    /// Human-readable output.
    #[default]
    Text,
    /// One versioned JSON report on stdout.
    Json,
}
/// Additive check settings; existing lifecycle option literals remain valid.
#[derive(Debug, Clone, Default)]
pub struct CheckOptions {
    /// Independently supplied release ledger, for one exact package.
    pub released_baseline: Option<PathBuf>,
    /// Check output representation.
    pub format: OutputFormat,
}
/// Detect an explicitly requested JSON envelope even when argument parsing fails.
pub fn requests_json(arguments: &[OsString]) -> bool {
    arguments
        .windows(2)
        .any(|pair| pair[0] == "--format" && pair[1] == "json")
}
/// Parse additive check-only flags while preserving the existing lifecycle parser.
pub fn parse_with_check(
    arguments: impl IntoIterator<Item = OsString>,
    contract: &ToolContract,
) -> Result<Option<(LifecycleOptions, CheckOptions)>> {
    let arguments: Vec<_> = arguments.into_iter().collect();
    let mut remaining = Vec::new();
    let mut check = CheckOptions::default();
    let mut format = None;
    let mut iter = arguments.into_iter();
    while let Some(flag) = iter.next() {
        if flag == "--released-baseline" || flag == "--format" {
            let value = iter
                .next()
                .ok_or_else(|| format!("missing value for {}", flag.to_string_lossy()))?;
            if value.to_string_lossy().starts_with('-') {
                return Err(format!("missing value for {}", flag.to_string_lossy()).into());
            }
            if flag == "--released-baseline" {
                set(
                    &mut check.released_baseline,
                    PathBuf::from(value),
                    "released-baseline",
                )?;
            } else {
                let selected = match value.to_str() {
                    Some("json") => OutputFormat::Json,
                    _ => return Err("--format supports only json".into()),
                };
                set(&mut format, selected, "format")?;
            }
        } else {
            remaining.push(flag);
        }
    }
    let Some(options) = parse(remaining, contract)? else {
        return Ok(None);
    };
    if (format.is_some() || check.released_baseline.is_some()) && options.action != Action::Check {
        return Err("--released-baseline and --format are only valid for check".into());
    }
    if check.released_baseline.is_some() && options.package.is_none() {
        return Err("--released-baseline requires one explicit --package NAME".into());
    }
    check.format = format.unwrap_or_default();
    Ok(Some((options, check)))
}
