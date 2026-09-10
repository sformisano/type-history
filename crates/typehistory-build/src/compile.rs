//! Ordinary build enforcement shared by Axis and standalone history packages.

use serde::{de::DeserializeOwned, Serialize};
use std::{
    env,
    ffi::OsStr,
    path::{Path, PathBuf},
};
use typehistory_codegen::ledger::{HistoryLedger, HistoryReadiness, SchemaIdentity};

use crate::{
    contract::{ToolContract, AXIS, STANDALONE},
    discover,
    inventory::{self, Admission, PackageInventory},
    Result,
};

/// Compile a standalone package's source authority from build.rs.
pub fn compile() {
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("Cargo package root"));
    let inventory = discover::read(&root, Admission::Ordinary)
        .unwrap_or_else(|error| panic!("Typehistory declarations: {error}"));
    compile_package(&root, &inventory, &STANDALONE)
        .unwrap_or_else(|error| panic!("Typehistory build: {error}"));
}

/// Enforce a complete ordinary source inventory and emit watched compiler authority.
pub fn compile_package<M: Clone + Eq + Serialize + DeserializeOwned>(
    root: &Path,
    source: &PackageInventory<M>,
    contract: &ToolContract,
) -> Result<()> {
    inventory::reject_conflicting_setup(root)?;
    let root = root.canonicalize()?;
    if source.root != root {
        return Err("history inventory root differs from the compiling package".into());
    }
    for path in &source.tracked_paths {
        println!("cargo::rerun-if-changed={}", path.display());
    }
    for name in [
        AXIS.strict_env,
        STANDALONE.strict_env,
        AXIS.export_env,
        STANDALONE.export_env,
    ] {
        println!("cargo::rerun-if-env-changed={name}");
    }
    println!("cargo::rerun-if-env-changed=PROFILE");
    let profile = env::var("PROFILE")?;
    let axis_strict = env::var_os(AXIS.strict_env);
    let standalone_strict = env::var_os(STANDALONE.strict_env);
    let strict = build_requires_frozen(
        &profile,
        axis_strict.as_deref(),
        standalone_strict.as_deref(),
    )?;
    let selected_export = env::var_os(contract.export_env);
    let export = enabled(contract.export_env, selected_export.as_deref())?;
    let other = if contract.authority_kind == AXIS.authority_kind {
        &STANDALONE
    } else {
        &AXIS
    };
    let other_export = env::var_os(other.export_env);
    // A dependency can use the other frontend. Its export switch does not select this package.
    enabled(other.export_env, other_export.as_deref())?;
    let ledger_path = root.join(contract.ledger_path);
    let ledger =
        HistoryLedger::read_file(&ledger_path, SchemaIdentity::new(contract.schema_id_prefix))?;
    inventory::validate(source, &ledger)?;
    for declaration in &source.declarations {
        if declaration.readiness == HistoryReadiness::Frozen {
            continue;
        }
        let kind = if declaration.readiness == HistoryReadiness::ResetDraft {
            "reset draft"
        } else {
            "draft"
        };
        let message = format!("{} V{} is a {kind}; freeze it with `cargo {} freeze --package {} --{} {} --version {}` before release", declaration.identity, declaration.version, contract.command, source.package, contract.selector, declaration.identity, declaration.version);
        if strict {
            return Err(message.into());
        }
        println!("cargo::warning={message}");
    }
    println!(
        "cargo::rustc-env=TYPEHISTORY_AUTHORITY_KIND={}",
        contract.authority_kind
    );
    println!(
        "cargo::rustc-env=TYPEHISTORY_SCHEMA_ID_PREFIX={}",
        contract.schema_id_prefix
    );
    println!(
        "cargo::rustc-env=TYPEHISTORY_LEDGER_PATH={}",
        ledger_path.display()
    );
    println!("cargo::rustc-check-cfg=cfg({})", contract.export_cfg);
    if export {
        println!("cargo::rustc-cfg={}", contract.export_cfg);
    }
    Ok(())
}

/// Apply both explicit switches to Cargo's consuming debug/release profile family.
pub fn build_requires_frozen(
    profile: &str,
    axis_strict: Option<&OsStr>,
    standalone_strict: Option<&OsStr>,
) -> Result<bool> {
    // Validate both values before combining them; a valid switch cannot hide an invalid one.
    let axis = enabled(AXIS.strict_env, axis_strict)?;
    let standalone = enabled(STANDALONE.strict_env, standalone_strict)?;
    match profile {
        "debug" => Ok(axis || standalone),
        "release" => Ok(true),
        _ => Err("Cargo PROFILE must identify the debug or release family".into()),
    }
}

fn enabled(name: &str, value: Option<&OsStr>) -> Result<bool> {
    match value {
        None => Ok(false),
        Some(value) if value == "1" => Ok(true),
        Some(_) => Err(format!(
            "{name} must be unset or exactly 1; it can only strengthen enforcement"
        )
        .into()),
    }
}

#[cfg(test)]
mod tests {
    use super::build_requires_frozen;
    use std::ffi::OsStr;

    #[test]
    fn both_strict_switches_only_strengthen_consumer_profile_policy() {
        assert!(!build_requires_frozen("debug", None, None).unwrap());
        assert!(build_requires_frozen("release", None, None).unwrap());
        let one = Some(OsStr::new("1"));
        for values in [(one, None), (None, one), (one, one)] {
            assert!(build_requires_frozen("debug", values.0, values.1).unwrap());
        }
        for value in ["", "0", "true", "false", " 1"] {
            let invalid = Some(OsStr::new(value));
            for profile in ["debug", "release"] {
                assert!(build_requires_frozen(profile, invalid, one).is_err());
                assert!(build_requires_frozen(profile, one, invalid).is_err());
            }
        }
        assert!(build_requires_frozen("unknown", None, None).is_err());
    }
}
