//! Public lifecycle coverage for captured Cargo inputs and read-only consumers.

use super::support::{failure, success, Fixture, LEDGER, STABLE_NAME, V1, V2};
use serde_json::json;
use std::env;
use std::ffi::OsString;
use std::fs::{self, Permissions};
#[cfg(unix)]
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};
use tempfile::Builder as TempBuilder;

const PACKAGE: &str = "standalone-history-consumer";

struct RestorePermissions {
    entries: Vec<(PathBuf, Permissions)>,
}

impl RestorePermissions {
    fn protect(path: &Path) -> Self {
        let original = fs::metadata(path).unwrap().permissions();
        let mut readonly = original.clone();
        readonly.set_readonly(true);
        fs::set_permissions(path, readonly).unwrap();
        Self {
            entries: vec![(path.to_owned(), original)],
        }
    }

    fn add(&mut self, path: &Path) {
        let original = fs::metadata(path).unwrap().permissions();
        let mut readonly = original.clone();
        readonly.set_readonly(true);
        fs::set_permissions(path, readonly).unwrap();
        self.entries.push((path.to_owned(), original));
    }

    #[cfg(unix)]
    fn protect_mode(path: &Path, mode: u32) -> Self {
        let original = fs::metadata(path).unwrap().permissions();
        fs::set_permissions(path, Permissions::from_mode(mode)).unwrap();
        Self {
            entries: vec![(path.to_owned(), original)],
        }
    }
}

impl Drop for RestorePermissions {
    fn drop(&mut self) {
        for (path, permissions) in self.entries.iter().rev() {
            let _ = fs::set_permissions(path, permissions.clone());
        }
    }
}

struct RestoreFile {
    from: PathBuf,
    to: PathBuf,
}

impl Drop for RestoreFile {
    fn drop(&mut self) {
        if self.from.exists() {
            let _ = fs::rename(&self.from, &self.to);
        }
    }
}

#[test]
fn read_only_check_and_consumer_build_create_no_live_authority_files() {
    let fixture = Fixture::frozen();
    let authority = fixture.root().join("type-history");
    let ledger = fixture.root().join(LEDGER);
    let lock = authority.join(".schemas.lock");
    if lock.exists() {
        fs::remove_file(&lock).unwrap();
    }
    let before_ledger = fs::read(&ledger).unwrap();
    let before_entries = entry_names(&authority);

    #[cfg(unix)]
    {
        let protected = RestorePermissions::protect_mode(&authority, 0o500);
        let checked = fixture.cli(&["check", "--package", PACKAGE]);
        drop(protected);
        success(&checked);
        assert_eq!(entry_names(&authority), before_entries);
        assert_eq!(fs::read(&ledger).unwrap(), before_ledger);
        assert!(!lock.exists());
    }

    let source = fixture.root().join("src/lib.rs");
    let before_source = fs::read(&source).unwrap();
    let mut protected = RestorePermissions::protect(&source);
    protected.add(&ledger);
    assert!(!fixture.root().join(".git").exists());
    assert!(!fixture.root().parent().unwrap().join(".git").exists());
    let hidden_cli = fixture.cli_path().with_extension("unavailable");
    fs::rename(fixture.cli_path(), &hidden_cli).unwrap();
    let restore_cli = RestoreFile {
        from: hidden_cli,
        to: fixture.cli_path().to_owned(),
    };
    success(&fixture.cargo(&["check", "--release", "--locked", "--offline"]));
    assert_eq!(fs::read(&source).unwrap(), before_source);
    assert_eq!(fs::read(&ledger).unwrap(), before_ledger);
    drop(restore_cli);
    drop(protected);

    fixture.remove(LEDGER);
    fixture.write("src/lib.rs", "");
    failure(
        &fixture.cargo(&["check", "--locked", "--offline"]),
        "cannot read committed history authority",
    );
}

#[test]
fn forced_package_and_ancestor_configs_preserve_exact_live_bytes() {
    let fixture = Fixture::frozen();
    fixture.write("src/lib.rs", V2);
    let ledger = fixture.read(LEDGER);
    let config = "[env]\nTYPE_HISTORY_REQUIRE_FROZEN = { value = '1', force = true }\n";

    for (path, root) in [
        (".cargo/config.toml", fixture.root().to_owned()),
        ("nested/.cargo/config.toml", fixture.root().join("nested")),
    ] {
        fixture.write(path, config);
        failure(
            &fixture.cli_at(
                &root,
                &["freeze", "--package", "standalone-history-consumer"],
            ),
            "Cargo configuration restores",
        );
        assert_eq!(fixture.read(LEDGER), ledger);
        assert_eq!(fixture.read(path), config);
        fixture.remove(path);
    }
}

#[test]
fn non_force_package_config_preserves_source_config_and_authority() {
    let fixture = Fixture::frozen();
    fixture.write("src/lib.rs", V2);
    let source = fixture.read("src/lib.rs");
    let ledger = fixture.read(LEDGER);
    let config = "[env]\nTYPE_HISTORY_REQUIRE_FROZEN = '1'\n";
    fixture.write(".cargo/config.toml", config);

    failure(
        &fixture.cli(&["freeze", "--package", PACKAGE]),
        "Cargo configuration restores",
    );
    assert_eq!(fixture.read("src/lib.rs"), source);
    assert_eq!(fixture.read(".cargo/config.toml"), config);
    assert_eq!(fixture.read(LEDGER), ledger);
}

#[cfg(unix)]
#[test]
fn cargo_home_forced_config_preserves_exact_live_bytes() {
    let fixture = Fixture::frozen();
    fixture.write("src/lib.rs", V2);
    let ledger = fixture.read(LEDGER);
    let owner = TempBuilder::new()
        .prefix("type-history-cargo-home-")
        .tempdir()
        .unwrap();
    let home = owner.path().join("cargo-home");
    fs::create_dir(&home).unwrap();
    let original = cargo_home();
    for name in ["registry", "git", ".package-cache", ".package-cache-mutate"] {
        let source = original.join(name);
        if source.exists() {
            symlink(source, home.join(name)).unwrap();
        }
    }
    let config = "[env]\nTYPE_HISTORY_REQUIRE_FROZEN = { value = '1', force = true }\n";
    fs::write(home.join("config.toml"), config).unwrap();
    failure(
        &fixture.cli_env(
            &["freeze", "--package", PACKAGE],
            &[("CARGO_HOME", Some(home.to_str().unwrap()))],
        ),
        "Cargo configuration restores",
    );
    assert_eq!(fixture.read(LEDGER), ledger);
    assert_eq!(
        fs::read_to_string(home.join("config.toml")).unwrap(),
        config
    );
}

#[cfg(unix)]
#[test]
fn live_cargo_home_source_and_config_aba_uses_captured_shape() {
    let fixture = Fixture::empty();
    success(&fixture.cli(&["init", "--package", PACKAGE]));
    let source = conditional_source();
    fixture.write("src/lib.rs", source);
    fixture.write(
        "build.rs",
        r#"fn main() {
    println!("cargo::rustc-check-cfg=cfg(wide)");
    if std::env::var("TYPE_HISTORY_FIXTURE_WIDE").as_deref() == Ok("1") {
        println!("cargo::rustc-cfg=wide");
    }
    history_build::compile();
}
"#,
    );
    let owner = TempBuilder::new()
        .prefix("type-history-config-aba-")
        .tempdir()
        .unwrap();
    let home = owner.path().join("cargo-home");
    fs::create_dir(&home).unwrap();
    for name in ["registry", "git", ".package-cache", ".package-cache-mutate"] {
        let original = cargo_home().join(name);
        if original.exists() {
            symlink(original, home.join(name)).unwrap();
        }
    }
    let config = home.join("config.toml");
    let captured = "[env]\nTYPE_HISTORY_FIXTURE_WIDE = { value = '0', force = true }\n";
    fs::write(&config, captured).unwrap();
    let live_source = fixture.root().join("src/lib.rs");
    let saved_source = owner.path().join("saved.rs");
    fs::write(&saved_source, source).unwrap();
    let saved_config = owner.path().join("saved-config.toml");
    fs::write(&saved_config, captured).unwrap();
    let observed_source = owner.path().join("observed-source.rs");
    let observed_config = owner.path().join("observed-config.toml");
    let wrapper_ran = owner.path().join("wrapper-ran");
    let wrapper = owner.path().join("cargo");
    fs::write(
        &wrapper,
        format!(
            r#"#!/usr/bin/env bash
set -euo pipefail
if [[ ${{1-}} == test ]]; then
  trap 'cp -- {saved_source} {live_source}; cp -- {saved_config} {config}' EXIT
  printf '%s\n' '[env]' "TYPE_HISTORY_FIXTURE_WIDE = {{ value = '1', force = true }}" > {config}
  sed 's/type Count = u32/type Count = u64/' {saved_source} > {live_source}
  cp -- {live_source} {observed_source}
  cp -- {config} {observed_config}
  touch {wrapper_ran}
  {cargo} "$@"
else
  exec {cargo} "$@"
fi
"#,
            saved_source = quote(&saved_source),
            live_source = quote(&live_source),
            saved_config = quote(&saved_config),
            config = quote(&config),
            observed_source = quote(&observed_source),
            observed_config = quote(&observed_config),
            wrapper_ran = quote(&wrapper_ran),
            cargo = quote(Path::new(env!("CARGO")))
        ),
    )
    .unwrap();
    fs::set_permissions(&wrapper, Permissions::from_mode(0o755)).unwrap();
    let path = env::join_paths(
        std::iter::once(owner.path().to_owned())
            .chain(env::split_paths(&env::var_os("PATH").unwrap())),
    )
    .unwrap();
    success(&fixture.cli_env(
        &["freeze", "--package", PACKAGE],
        &[
            ("CARGO_HOME", Some(home.to_str().unwrap())),
            ("PATH", Some(path.to_str().unwrap())),
        ],
    ));
    let ledger = fixture.read(LEDGER);
    assert!(ledger.contains("uint32"), "{ledger}");
    assert!(!ledger.contains("uint64"), "{ledger}");
    assert!(wrapper_ran.exists(), "Cargo wrapper test branch must run");
    let mutated_source = source.replace("type Count = u32", "type Count = u64");
    assert_ne!(mutated_source, source);
    assert_eq!(fs::read_to_string(observed_source).unwrap(), mutated_source);
    assert_eq!(
        fs::read_to_string(observed_config).unwrap(),
        "[env]\nTYPE_HISTORY_FIXTURE_WIDE = { value = '1', force = true }\n"
    );
    assert_eq!(fs::read_to_string(config).unwrap(), captured);
    assert_eq!(fs::read_to_string(live_source).unwrap(), source);
}

#[test]
fn relative_config_path_outside_graph_preserves_live_inputs() {
    let fixture = Fixture::frozen();
    let source = fixture.read("src/lib.rs");
    let ledger = fixture.read(LEDGER);
    let config = "[env]\nEXTERNAL_INPUT = { value = '../untracked-input', relative = true }\n";
    fixture.write(".cargo/config.toml", config);
    failure(
        &fixture.cli(&["freeze", "--package", PACKAGE]),
        "relative path is missing from the checked local graph",
    );
    assert_eq!(fixture.read("src/lib.rs"), source);
    assert_eq!(fixture.read(".cargo/config.toml"), config);
    assert_eq!(fixture.read(LEDGER), ledger);
}

#[test]
fn custom_target_sources_are_rejected_without_live_mutation() {
    let fixture = Fixture::frozen();
    let source = fixture.read("src/lib.rs");
    let ledger = fixture.read(LEDGER);
    fixture.write("custom.json", "{}");
    let absolute = fixture.root().join("custom.json");
    for target in [
        "custom.json",
        "targets/custom.json",
        absolute.to_str().unwrap(),
    ] {
        failure(
            &fixture.cli(&["freeze", "--package", PACKAGE, "--target", target]),
            "custom target specifications are not supported",
        );
        assert_eq!(fixture.read(LEDGER), ledger);
    }
    for (name, value) in [
        ("CARGO_BUILD_TARGET", "custom.json"),
        ("CARGO_BUILD_TARGET", absolute.to_str().unwrap()),
        ("RUST_TARGET_PATH", fixture.root().to_str().unwrap()),
    ] {
        failure(
            &fixture.cli_env(&["freeze", "--package", PACKAGE], &[(name, Some(value))]),
            "custom target",
        );
        assert_eq!(fixture.read(LEDGER), ledger);
    }
    for config in [
        "[build]\ntarget = 'custom.json'\n",
        "[env]\nCARGO_BUILD_TARGET = 'custom.json'\n",
        "[env]\nRUST_TARGET_PATH = 'targets'\n",
    ] {
        fixture.write(".cargo/config.toml", config);
        failure(
            &fixture.cli(&["freeze", "--package", PACKAGE]),
            "custom target",
        );
        assert_eq!(fixture.read(".cargo/config.toml"), config);
        assert_eq!(fixture.read(LEDGER), ledger);
    }
    assert_eq!(fixture.read("src/lib.rs"), source);
}

fn entry_names(directory: &Path) -> Vec<OsString> {
    let mut names = fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect::<Vec<_>>();
    names.sort();
    names
}

fn conditional_source() -> &'static str {
    r#"use history_api::versioned;
#[cfg(wide)] type Count = u64;
#[cfg(not(wide))] type Count = u32;
#[versioned(stable_name = "billing.invoice.issued")]
pub struct Invoice { pub count: Count }
"#
}

fn cargo_home() -> PathBuf {
    env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env::var_os("HOME").unwrap()).join(".cargo"))
}

#[cfg(unix)]
fn quote(path: &Path) -> String {
    format!("'{}'", path.to_str().unwrap().replace('\'', "'\\''"))
}

#[test]
fn lifecycle_snapshots_reuse_a_locked_target_without_hiding_changes() {
    let fixture = Fixture::frozen();
    let ledger = fixture.read(LEDGER);
    // A target inside the workspace isolates this reuse state and its copies.
    let target = fixture.root().join("reuse-target");
    let root = target.join("type-history-snapshot");
    let check = |environment: &[(&str, Option<&str>)]| {
        let mut all = vec![("CARGO_TARGET_DIR", Some(target.to_str().unwrap()))];
        all.extend_from_slice(environment);
        fixture.cli_env(&["check"], &all)
    };
    let entries = || {
        let mut names: Vec<OsString> = fs::read_dir(&root)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        names.sort();
        names
    };
    // Only a target directory that Cargo already created is reused.
    success(&check(&[]));
    assert!(!target.exists());
    // Like the strict switches, the setting only accepts `1`.
    failure(
        &check(&[("TYPE_HISTORY_PRIVATE_SNAPSHOT", Some("0"))]),
        "TYPE_HISTORY_PRIVATE_SNAPSHOT must be unset or exactly 1",
    );
    fs::create_dir(&target).unwrap();
    success(&check(&[]));
    // Copies and the isolated Cargo home end with the operation; output stays.
    assert_eq!(entries(), ["lock", "target"]);
    let state = |package: &str| {
        fs::read_dir(root.join("target/debug/.fingerprint"))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .find(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .and_then(|name| name.rsplit_once('-'))
                    .is_some_and(|(name, _)| name == package)
            })
            .unwrap_or_else(|| panic!("Cargo state for {package}"))
            .join("reuse-marker")
    };
    let local = state(PACKAGE);
    let dependency = state("serde");
    fs::write(&local, "").unwrap();
    fs::write(&dependency, "").unwrap();

    // The warm target still observes edited source and ledger authority.
    fixture.write("src/lib.rs", &V1.replace("u32", "String"));
    failure(&check(&[]), "frozen");
    // Local packages rebuild from every new copy; dependencies are reused.
    assert!(!local.exists());
    assert!(dependency.exists());
    fixture.write("src/lib.rs", V1);
    success(&check(&[]));
    let mut changed = fixture.ledger();
    changed[STABLE_NAME]["1"]["schema"]["properties"]["count"] =
        json!({"type": "integer", "minimum": 0, "format": "uint64"});
    fixture.set_ledger(&changed);
    failure(&check(&[]), "frozen");
    fixture.write(LEDGER, &ledger);
    success(&check(&[]));

    // A busy target gives the operation a private snapshot instead of waiting.
    let lock = fs::File::options()
        .read(true)
        .write(true)
        .open(root.join("lock"))
        .unwrap();
    lock.try_lock().unwrap();
    // Copies left by an interrupted operation stay untouched while locked.
    for leftover in ["tree", "cargo-home"] {
        fs::create_dir(root.join(leftover)).unwrap();
        fs::write(root.join(leftover).join("stale"), "interrupted").unwrap();
    }
    let scratch = TempBuilder::new()
        .prefix("reuse-scratch-")
        .tempdir()
        .unwrap();
    success(&check(&[(
        "TMPDIR",
        Some(scratch.path().to_str().unwrap()),
    )]));
    assert_eq!(fs::read_dir(scratch.path()).unwrap().count(), 0);
    for leftover in ["tree", "cargo-home"] {
        assert_eq!(
            fs::read_to_string(root.join(leftover).join("stale")).unwrap(),
            "interrupted"
        );
    }
    drop(lock);
    // The next holder removes them before capturing again.
    success(&check(&[]));
    assert_eq!(entries(), ["lock", "target"]);
    assert_eq!(fixture.read(LEDGER), ledger);
}
