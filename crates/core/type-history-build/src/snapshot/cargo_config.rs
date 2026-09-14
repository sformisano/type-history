//! Preserve Cargo's config-relative paths without rewriting configuration.
//!
//! Cargo resolves config paths relative to the config directory's parent, even
//! for CARGO_HOME. The mirror retains that topology. Settings that cannot keep
//! their meaning inside the checked local graph fail before a child is started.

use std::collections::BTreeMap;
use std::error::Error;
use std::fs::{self, OpenOptions};
use std::path::{Component, Path, PathBuf};

use toml::Value;

use super::mirrored;
use crate::options::named_target;
use crate::Result;

pub(super) fn ordered_configs(
    invocation: &Path,
    home: &Path,
    captured: &BTreeMap<PathBuf, Vec<u8>>,
) -> Vec<PathBuf> {
    let select = |directory: &Path| {
        ["config", "config.toml"]
            .into_iter()
            .map(|name| directory.join(name))
            .find(|path| captured.get(path).is_some_and(|bytes| bytes != &[0]))
    };
    let mut ancestors: Vec<_> = invocation.ancestors().collect();
    ancestors.reverse();
    let mut configs: Vec<_> = ancestors
        .into_iter()
        .filter_map(|path| select(&path.join(".cargo")))
        .collect();
    if let Some(home) = select(home) {
        if !configs.contains(&home) {
            configs.insert(0, home);
        }
    }
    configs
}

pub(super) fn check_root() -> Result<()> {
    for name in ["config", "config.toml"] {
        let path = Path::new("/").join(".cargo").join(name);
        if fs::symlink_metadata(&path).is_ok() {
            return Err(format!("cannot isolate Cargo configuration while {} exists; validation requires a configuration-free root working directory", path.display()).into());
        }
    }
    Ok(())
}

mod environment;
pub(super) use environment::check_environment;

pub(super) fn validate(
    config: &Path,
    bytes: &[u8],
    roots: &[PathBuf],
    mirror: &Path,
) -> Result<()> {
    let document: Value = toml::from_slice(bytes)?;
    if document.get("include").is_some() {
        return Err(format!("cannot snapshot Cargo configuration includes in {}; use an equivalent non-included configuration", config.display()).into());
    }
    if let Some(paths) = document.get("paths") {
        for path in paths.as_array().ok_or("Cargo paths must be an array")? {
            reference(
                config,
                "paths",
                path.as_str().ok_or("Cargo paths must contain strings")?,
                roots,
                mirror,
                false,
            )?;
        }
    }
    for section in ["patch", "source"] {
        if let Some(value) = document.get(section) {
            source_paths(config, section, value, roots, mirror)?;
        }
    }
    if let Some(build) = document.get("build").and_then(Value::as_table) {
        for (key, value) in build {
            match key.as_str() {
                // The existing --target-dir argument always overrides this output setting.
                "target-dir" => {}
                "build-dir" | "dep-info-basedir" => {
                    return Err(unsupported(
                        config,
                        &format!("build.{key}"),
                        "this path setting cannot be preserved by the isolated build",
                    ));
                }
                "rustc" | "rustc-wrapper" | "rustc-workspace-wrapper" | "rustdoc" => {
                    executable(config, &format!("build.{key}"), value, roots, mirror)?;
                }
                "rustflags" | "rustdocflags" => flags(config, key, value)?,
                "target" => {
                    for value in strings(value)? {
                        named_target(value)?;
                    }
                }
                _ => {}
            }
        }
    }
    if let Some(targets) = document.get("target").and_then(Value::as_table) {
        for target in targets.values().filter_map(Value::as_table) {
            for (key, value) in target {
                match key.as_str() {
                    "linker" | "runner" => executable(config, key, value, roots, mirror)?,
                    "rustflags" | "rustdocflags" => flags(config, key, value)?,
                    // Build-script overrides may carry search paths and arbitrary tool
                    // metadata. Reject them instead of interpreting a second language.
                    _ if value.is_table() => {
                        return Err(unsupported(
                            config,
                            key,
                            "target build-script overrides are not supported in a checked snapshot",
                        ));
                    }
                    _ => {}
                }
            }
        }
    }
    if let Some(environment) = document.get("env").and_then(Value::as_table) {
        if environment.contains_key("CARGO_HOME") {
            return Err(unsupported(
                config,
                "env.CARGO_HOME",
                "Cargo home must remain the checked configuration copy",
            ));
        }
        for (key, value) in environment {
            if key == "RUST_TARGET_PATH" {
                return Err(unsupported(
                    config,
                    key,
                    "custom target search paths are not supported by history snapshots",
                ));
            }
            if key == "CARGO_BUILD_TARGET" {
                named_target(
                    value
                        .as_str()
                        .or_else(|| value.get("value").and_then(Value::as_str))
                        .ok_or("Cargo target environment setting requires a string value")?,
                )?;
            }
            if value.get("relative").and_then(Value::as_bool) == Some(true) {
                reference(
                    config,
                    &format!("env.{key}"),
                    value
                        .get("value")
                        .and_then(Value::as_str)
                        .ok_or("relative Cargo env requires a string value")?,
                    roots,
                    mirror,
                    false,
                )?;
            }
        }
    }
    if let Some(http) = document.get("http") {
        for key in ["cainfo", "proxy-cainfo"] {
            if let Some(value) = http.get(key).and_then(Value::as_str) {
                reference(config, key, value, roots, mirror, false)?;
            }
        }
    }
    if document.get("credential-alias").is_some() {
        return Err(unsupported(
            config,
            "credential-alias",
            "external credential commands are not needed by offline schema validation",
        ));
    }
    // These sections can name credential executables or file:// registries.
    for key in ["registry", "registries"] {
        if let Some(value) = document.get(key) {
            registry(config, key, value)?;
        }
    }
    Ok(())
}

fn source_paths(
    config: &Path,
    key: &str,
    value: &Value,
    roots: &[PathBuf],
    mirror: &Path,
) -> Result<()> {
    if let Some(table) = value.as_table() {
        for (name, value) in table {
            let key = format!("{key}.{name}");
            if matches!(name.as_str(), "path" | "directory" | "local-registry") {
                reference(
                    config,
                    &key,
                    value
                        .as_str()
                        .ok_or("Cargo local source path must be a string")?,
                    roots,
                    mirror,
                    false,
                )?;
            } else if let Some(value) = value.as_str().filter(|value| value.starts_with("file:")) {
                return Err(unsupported(
                    config,
                    &key,
                    &format!("local URL {value} would read outside the checked copy"),
                ));
            } else {
                source_paths(config, &key, value, roots, mirror)?;
            }
        }
    }
    Ok(())
}

fn registry(config: &Path, key: &str, value: &Value) -> Result<()> {
    if let Some(table) = value.as_table() {
        for (name, value) in table {
            let key = format!("{key}.{name}");
            if matches!(
                name.as_str(),
                "credential-provider" | "global-credential-providers"
            ) {
                if strings(value)?
                    .iter()
                    .any(|value| !value.starts_with("cargo:"))
                {
                    return Err(unsupported(
                        config,
                        &key,
                        "external credential commands cannot be preserved in an offline snapshot",
                    ));
                }
            } else if value
                .as_str()
                .is_some_and(|value| value.starts_with("file:"))
            {
                return Err(unsupported(
                    config,
                    &key,
                    "local registry URLs cannot be preserved without rewriting configuration",
                ));
            } else {
                registry(config, &key, value)?;
            }
        }
    }
    Ok(())
}

fn flags(config: &Path, key: &str, value: &Value) -> Result<()> {
    if strings(value)?.iter().any(|value| value.contains('/')) {
        return Err(unsupported(
            config,
            key,
            "compiler flag paths cannot be preserved without interpreting or rewriting flags",
        ));
    }
    Ok(())
}

fn executable(
    config: &Path,
    key: &str,
    value: &Value,
    roots: &[PathBuf],
    mirror: &Path,
) -> Result<()> {
    let values = strings(value)?;
    let first = values.first().ok_or("Cargo executable must not be empty")?;
    // Cargo permits a string containing an executable plus arguments. Its
    // argument language is not rewritten here; path-bearing commands fail closed.
    if values.len() == 1 && first.contains(char::is_whitespace) && first.contains('/') {
        return Err(unsupported(
            config,
            key,
            "path-bearing executable arguments cannot be preserved",
        ));
    }
    reference(config, key, first, roots, mirror, true)?;
    if values.iter().skip(1).any(|value| value.contains('/')) {
        return Err(unsupported(
            config,
            key,
            "path-bearing executable arguments cannot be preserved",
        ));
    }
    Ok(())
}

fn strings(value: &Value) -> Result<Vec<&str>> {
    if let Some(value) = value.as_str() {
        return Ok(vec![value]);
    }
    value
        .as_array()
        .ok_or("Cargo setting requires a string or string array")?
        .iter()
        .map(|value| {
            value
                .as_str()
                .ok_or_else(|| "Cargo setting requires strings".into())
        })
        .collect()
}

fn reference(
    config: &Path,
    key: &str,
    value: &str,
    roots: &[PathBuf],
    mirror: &Path,
    executable: bool,
) -> Result<()> {
    if executable && !value.contains('/') {
        return Ok(());
    }
    let path = Path::new(value);
    if path.is_absolute() {
        return Err(unsupported(
            config,
            key,
            "absolute local paths would escape the checked copy; use a relative path inside the resolved local graph",
        ));
    }
    let base = config
        .parent()
        .and_then(Path::parent)
        .ok_or("Cargo configuration has no relative base")?;
    let original = normalize(&base.join(path));
    let copied = normalize(&mirrored(mirror, base)?.join(path));
    if copied != mirrored(mirror, &original)? {
        return Err(unsupported(
            config,
            key,
            "relative path traverses beyond the preserved filesystem root",
        ));
    }
    let resolved = copied.canonicalize().map_err(|_| {
        unsupported(
            config,
            key,
            "relative path is missing from the checked local graph",
        )
    })?;
    if !roots
        .iter()
        .any(|root| mirrored(mirror, root).is_ok_and(|root| resolved.starts_with(root)))
    {
        return Err(unsupported(
            config,
            key,
            "relative path resolves outside the checked local graph",
        ));
    }
    Ok(())
}

fn normalize(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                normalized.pop();
            }
            Component::CurDir => {}
            component => normalized.push(component.as_os_str()),
        }
    }
    normalized
}

fn unsupported(config: &Path, key: &str, reason: &str) -> Box<dyn Error> {
    format!("cannot preserve effective Cargo configuration {} ({key}): {reason}; source and ledger were not changed", config.display()).into()
}

pub(super) fn share_caches(original: &Path, copied: &Path) -> Result<()> {
    fs::create_dir_all(copied)?;
    // Cargo serializes cache readers/cleaners with these same inode locks.
    // A private home with private locks would not protect the shared caches.
    for name in ["registry", "git", ".package-cache", ".package-cache-mutate"] {
        let destination = copied.join(name);
        if fs::symlink_metadata(&destination).is_ok() {
            return Err(format!("Cargo home cache {} overlaps checked source; use a Cargo home outside the local source graph", destination.display()).into());
        }
        let source = original.join(name);
        if name.starts_with('.') {
            OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(&source)?;
        } else if !source.exists() {
            continue;
        }
        #[cfg(unix)]
        std::os::unix::fs::symlink(source, destination)?;
        #[cfg(not(unix))]
        return Err("isolated Cargo caches require supported Unix symlink semantics".into());
    }
    Ok(())
}
