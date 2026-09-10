//! Reject configuration overrides whose precedence or paths would change.
use super::unsupported;
use crate::options::named_target;
use crate::Result;
use std::env;
use std::ffi::OsString;
use std::path::Path;
use toml::Value;

/// `--config FILE` has higher precedence than environment settings. Reject
/// conflicts instead of silently changing the caller's effective configuration.
pub(crate) fn check_environment(
    config: &Path,
    bytes: &[u8],
    environment: &[(OsString, OsString)],
) -> Result<()> {
    let document: Value = toml::from_slice(bytes)?;
    check_conflicts(config, "", &document, environment)?;
    for (name, value) in environment {
        if name == "PATH" && env::split_paths(value).any(|path| !path.is_absolute()) {
            return Err(unsupported(
                config,
                "PATH",
                "relative executable search paths cannot keep their meaning in the isolated working directory",
            ));
        }
        let name = name.to_string_lossy();
        let value = value.to_string_lossy();
        if name == "RUST_TARGET_PATH" {
            return Err(unsupported(
                config,
                &name,
                "custom target search paths are not supported by history snapshots",
            ));
        }
        if name == "CARGO_BUILD_TARGET" {
            named_target(&value)?;
        }

        let tool = matches!(
            name.as_ref(),
            "RUSTC" | "RUSTDOC" | "RUSTC_WRAPPER" | "RUSTC_WORKSPACE_WRAPPER"
        ) || name.ends_with("_LINKER")
            || name.ends_with("_RUNNER");
        let flags = name.contains("RUSTFLAGS") || name.contains("RUSTDOCFLAGS");
        if (tool && value.contains('/') && !Path::new(value.as_ref()).is_absolute())
            || (flags && value.contains('/'))
            || (name.starts_with("CARGO_")
                && (name.ends_with("_DIRECTORY")
                    || name.ends_with("_LOCAL_REGISTRY")
                    || name == "CARGO_BUILD_BUILD_DIR"))
        {
            return Err(unsupported(
                config,
                &name,
                "ambient path settings cannot keep their meaning in the isolated working directory",
            ));
        }
    }
    Ok(())
}

fn check_conflicts(
    config: &Path,
    key: &str,
    value: &Value,
    environment: &[(OsString, OsString)],
) -> Result<()> {
    if key == "env" || key == "build.target-dir" {
        return Ok(());
    }
    if let Some(table) = value.as_table() {
        for (name, value) in table {
            let child = if key.is_empty() {
                name.clone()
            } else {
                format!("{key}.{name}")
            };
            check_conflicts(config, &child, value, environment)?;
        }
        return Ok(());
    }
    let variable = format!("CARGO_{}", key.replace(['.', '-'], "_").to_uppercase());
    let aliases: &[&str] = match key {
        "build.rustflags" => &["RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS"],
        "build.rustdocflags" => &["RUSTDOCFLAGS", "CARGO_ENCODED_RUSTDOCFLAGS"],
        "build.rustc" => &["RUSTC"],
        "build.rustdoc" => &["RUSTDOC"],
        "build.rustc-wrapper" => &["RUSTC_WRAPPER"],
        "build.rustc-workspace-wrapper" => &["RUSTC_WORKSPACE_WRAPPER"],
        key if key.ends_with(".incremental") => &["CARGO_INCREMENTAL"],
        _ => &[],
    };
    if environment.iter().any(|(name, _)| {
        name == variable.as_str() || aliases.iter().any(|alias| name.to_str() == Some(*alias))
    }) {
        return Err(unsupported(
            config,
            key,
            "an environment override conflicts with this checked config setting; remove the duplicate setting deliberately before retrying",
        ));
    }
    Ok(())
}
