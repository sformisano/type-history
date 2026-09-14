//! Ordinary build enforcement for Type History packages.

use serde::{de::DeserializeOwned, Serialize};
use std::{
    env,
    ffi::OsStr,
    path::{Path, PathBuf},
};
use type_history_codegen::{
    admission::Invocation,
    ledger::{HistoryLedger, HistoryReadiness, SchemaIdentity},
};

use crate::{
    contract::{ToolContract, STANDALONE},
    discover,
    inventory::{self, Admission, PackageInventory},
    Result,
};

/// Check this package's histories and enable frozen schema checks from `build.rs`.
///
/// Call this after initializing `type-history/schemas.json`. It discovers the
/// package's declarations, checks them against the ledger, and tells Cargo which
/// files to watch. The generated code then checks compiler-resolved field schemas.
/// Drafts warn in development and fail in release-derived profiles; setting
/// `TYPE_HISTORY_REQUIRE_FROZEN=1` also rejects drafts in development.
/// Every expansion must match a discovered module-level declaration. Undiscovered
/// macro-generated, included, or function-local histories are rejected. Matching
/// expansions still receive the current ledger, frozen-shape, and strict checks.
///
/// # Panics
///
/// Panics when discovery or validation fails, causing Cargo to stop the build.
pub fn compile() {
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("Cargo package root"));
    let (inventory, invocations) = discover::read_with_admission(&root, Admission::Ordinary)
        .unwrap_or_else(|error| panic!("Type History declarations: {error}"));
    compile_with_admission(&root, &inventory, &STANDALONE, Some(invocations))
        .unwrap_or_else(|error| panic!("Type History build: {error}"));
}

/// Check a discovered package against its ledger and configure compiler checks.
///
/// Custom integrations supply the complete source inventory and command settings.
/// Applications normally call [`compile()`]. A custom contract whose authority
/// kind is `standalone` also uses checked standalone source discovery and requires
/// a matching complete inventory. Both routes publish mandatory compiler admission
/// in Cargo's `OUT_DIR`; other authority kinds retain their own macro frontend.
pub fn compile_package<M: Clone + Eq + Serialize + DeserializeOwned>(
    root: &Path,
    source: &PackageInventory<M>,
    contract: &ToolContract,
) -> Result<()> {
    compile_with_admission(root, source, contract, None)
}

fn compile_with_admission<M: Clone + Eq + Serialize + DeserializeOwned>(
    root: &Path,
    source: &PackageInventory<M>,
    contract: &ToolContract,
    invocations: Option<Vec<Invocation>>,
) -> Result<()> {
    let root = root.canonicalize()?;
    if source.root != root {
        return Err("history inventory root differs from the compiling package".into());
    }
    for path in &source.tracked_paths {
        println!("cargo::rerun-if-changed={}", path.display());
    }
    for name in [contract.strict_env, contract.export_env] {
        println!("cargo::rerun-if-env-changed={name}");
    }
    println!("cargo::rerun-if-env-changed=PROFILE");
    let profile = env::var("PROFILE")?;
    let strict_setting = env::var_os(contract.strict_env);
    let strict = requires_frozen(&profile, contract.strict_env, strict_setting.as_deref())?;
    let selected_export = env::var_os(contract.export_env);
    let export = enabled(contract.export_env, selected_export.as_deref())?;
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
        let message = format!("{} V{} is a {kind}; freeze it with `cargo {} freeze --package {} --{} {} --version {}` before release", declaration.stable_name, declaration.version, contract.command, source.package, contract.selector, declaration.stable_name, declaration.version);
        if strict {
            return Err(message.into());
        }
        println!("cargo::warning={message}");
    }
    if contract.authority_kind == "standalone" {
        let invocations = match invocations {
            Some(invocations) => invocations,
            None => crate::admission::discover(&root, source)?,
        };
        crate::admission::publish(strict, invocations)?;
    }
    println!(
        "cargo::rustc-env=TYPE_HISTORY_AUTHORITY_KIND={}",
        contract.authority_kind
    );
    println!(
        "cargo::rustc-env=TYPE_HISTORY_SCHEMA_ID_PREFIX={}",
        contract.schema_id_prefix
    );
    println!(
        "cargo::rustc-env=TYPE_HISTORY_LEDGER_PATH={}",
        ledger_path.display()
    );
    println!("cargo::rustc-check-cfg=cfg({})", contract.export_cfg);
    if export {
        println!("cargo::rustc-cfg={}", contract.export_cfg);
    }
    Ok(())
}

/// Apply the strict switch to Cargo's consuming debug/release profile family.
pub fn build_requires_frozen(profile: &str, strict_setting: Option<&OsStr>) -> Result<bool> {
    requires_frozen(profile, STANDALONE.strict_env, strict_setting)
}

fn requires_frozen(
    profile: &str,
    strict_env: &str,
    strict_setting: Option<&OsStr>,
) -> Result<bool> {
    let strict = enabled(strict_env, strict_setting)?;
    match profile {
        "debug" => Ok(strict),
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
    fn strict_switch_only_strengthens_consumer_profile_policy() {
        assert!(!build_requires_frozen("debug", None).unwrap());
        assert!(build_requires_frozen("release", None).unwrap());
        assert!(build_requires_frozen("debug", Some(OsStr::new("1"))).unwrap());
        for value in ["", "0", "true", "false", " 1"] {
            for profile in ["debug", "release"] {
                assert!(build_requires_frozen(profile, Some(OsStr::new(value))).is_err());
            }
        }
        assert!(build_requires_frozen("unknown", None).is_err());
    }
}
