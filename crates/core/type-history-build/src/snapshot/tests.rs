//! Deterministic edit/copy/undo probes do not depend on filesystem timing.
use super::{cargo_cache_roots, cargo_config, declared_snapshot_inputs, Snapshot};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::Builder as TempBuilder;

fn fixture() -> Snapshot {
    let owner = TempBuilder::new()
        .prefix("history-copy-proof-")
        .tempdir()
        .unwrap();
    let root = owner.path().join("source");
    fs::create_dir_all(&root).unwrap();
    Snapshot {
        target: owner.path().join("target"),
        cargo_home: owner.path().join("home"),
        manifest: root.join("Cargo.toml"),
        configs: Vec::new(),
        toolchain: None,
        mirror: owner.path().join("mirror"),
        roots: vec![root],
        boundaries: Vec::new(),
        manifests: BTreeSet::new(),
        workspaces: BTreeSet::new(),
        extra: Vec::new(),
        excluded_target: owner.path().join("target"),
        excluded_caches: BTreeSet::new(),
        excluded_paths: BTreeSet::new(),
        before: BTreeMap::new(),
        environment: Vec::new(),
        compiler: Vec::new(),
        _owner: owner,
    }
}

#[test]
fn declared_inputs_are_relative_existing_paths() {
    let owner = TempBuilder::new()
        .prefix("declared-snapshot-inputs-")
        .tempdir()
        .unwrap();
    let root = owner.path().join("workspace/crates/domain");
    let input = owner.path().join("workspace/generated/schema.json");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::create_dir_all(input.parent().unwrap()).unwrap();
    fs::write(&input, "captured").unwrap();
    let document = toml::from_str(
        "[package.metadata.type-history]\nsnapshot-inputs = ['../../generated/schema.json']\n",
    )
    .unwrap();
    assert_eq!(
        declared_snapshot_inputs(&root, &document).unwrap(),
        [input.canonicalize().unwrap()]
    );

    for value in ["'/absolute/input'", "'../../missing'"] {
        let document = toml::from_str(&format!(
            "[package.metadata.type-history]\nsnapshot-inputs = [{value}]\n"
        ))
        .unwrap();
        assert!(declared_snapshot_inputs(&root, &document).is_err());
    }
}

#[test]
fn declared_input_bytes_remain_part_of_freshness_checks() {
    let mut snapshot = fixture();
    let input = snapshot._owner.path().join("generated/schema.json");
    fs::create_dir_all(input.parent().unwrap()).unwrap();
    fs::write(&input, "captured").unwrap();
    snapshot.roots.push(input.clone());
    snapshot.before = snapshot.fingerprint().unwrap();
    snapshot.copy_tree(&input).unwrap();
    snapshot.verify_complete_copy().unwrap();

    fs::write(&input, "changed").unwrap();
    assert_ne!(snapshot.before, snapshot.fingerprint().unwrap());
    assert!(snapshot
        .copy_tree(&input)
        .unwrap_err()
        .to_string()
        .contains("InputsChanged"));
}

#[test]
fn cargo_outputs_are_excluded_without_losing_source_named_target() {
    let mut snapshot = fixture();
    let root = snapshot.roots[0].clone();
    snapshot.excluded_target = root.join("build-output");
    let source = root.join("src/target/mod.rs");
    let artifact = snapshot.excluded_target.join("generated");
    for (path, contents) in [(&source, "pub struct Record;"), (&artifact, "output")] {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }
    let dependency = root.join("dependency");
    let cached = dependency.join("target/generated");
    fs::create_dir_all(cached.parent().unwrap()).unwrap();
    fs::write(&cached, "dependency output").unwrap();
    fs::write(
        dependency.join("target/CACHEDIR.TAG"),
        "Signature: 8a477f597d28d172789f06886806bc55\n# Cargo build cache\n",
    )
    .unwrap();
    let untagged = root.join("target/mod.rs");
    fs::create_dir_all(untagged.parent().unwrap()).unwrap();
    fs::write(&untagged, "pub struct Source;").unwrap();
    // A filename alone does not identify a disposable cache.
    fs::write(root.join("target/CACHEDIR.TAG"), "ordinary source marker").unwrap();
    snapshot.excluded_caches = cargo_cache_roots(&[root.clone(), dependency.clone()]);
    assert_eq!(
        snapshot.excluded_caches,
        BTreeSet::from([dependency.join("target")])
    );
    snapshot.before = snapshot.fingerprint().unwrap();
    snapshot.copy_tree(&root).unwrap();
    assert_eq!(
        fs::read_to_string(snapshot.mapped(&source).unwrap()).unwrap(),
        "pub struct Record;"
    );
    assert!(!snapshot.mapped(&artifact).unwrap().exists());
    assert!(!snapshot.mapped(&cached).unwrap().exists());
    assert_eq!(
        fs::read_to_string(snapshot.mapped(&untagged).unwrap()).unwrap(),
        "pub struct Source;"
    );
    fs::write(&artifact, "changed output").unwrap();
    fs::write(&cached, "changed dependency output").unwrap();
    assert_eq!(snapshot.before, snapshot.fingerprint().unwrap());
    fs::write(&source, "pub struct Changed;").unwrap();
    assert_ne!(snapshot.before, snapshot.fingerprint().unwrap());
}

#[test]
fn source_config_and_import_edit_copy_undo_are_rejected() {
    for name in ["src/lib.rs", ".cargo/config.toml", "import.json"] {
        let mut snapshot = fixture();
        let path = snapshot.roots[0].join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "captured").unwrap();
        snapshot.before = snapshot.fingerprint().unwrap();
        fs::write(&path, "transient").unwrap();
        let error = snapshot.copy_file(&path).unwrap_err();
        fs::write(&path, "captured").unwrap();
        assert_eq!(
            snapshot.before,
            snapshot.fingerprint().unwrap(),
            "live-only freshness misses this ABA"
        );
        assert!(error.to_string().contains("InputsChanged"));
        assert!(!snapshot.mapped(&path).unwrap().exists());
    }
}

#[test]
fn missing_copy_and_transient_copied_bytes_are_rejected_after_live_undo() {
    let mut snapshot = fixture();
    let path = snapshot.roots[0].join("input");
    fs::write(&path, "captured").unwrap();
    snapshot.before = snapshot.fingerprint().unwrap();
    assert!(snapshot
        .verify_complete_copy()
        .unwrap_err()
        .to_string()
        .contains("missing"));
    snapshot.copy_file(&path).unwrap();
    snapshot.verify_complete_copy().unwrap();
    fs::write(snapshot.mapped(&path).unwrap(), "transient").unwrap();
    assert!(snapshot
        .verify_complete_copy()
        .unwrap_err()
        .to_string()
        .contains("InputsChanged"));
}

#[test]
fn config_relative_override_stays_inside_the_checked_graph() {
    let mut snapshot = fixture();
    let root = &snapshot.roots[0];
    let config = root.join(".cargo/config.toml");
    fs::create_dir_all(config.parent().unwrap()).unwrap();
    fs::create_dir_all(root.join("local-types")).unwrap();
    let captured = b"paths = ['local-types']\n";
    fs::write(&config, captured).unwrap();
    snapshot.before = snapshot.fingerprint().unwrap();
    snapshot.copy_tree(&snapshot.roots[0]).unwrap();
    // Validation uses the checked bytes, even while the original says otherwise.
    fs::write(&config, "paths = ['/outside']").unwrap();
    cargo_config::validate(
        &config,
        &fs::read(snapshot.mapped(&config).unwrap()).unwrap(),
        &snapshot.roots,
        &snapshot.mirror,
    )
    .unwrap();
    for rejected in [
        "paths = ['/outside']",
        "paths = ['../../../../../../outside']",
        "paths = ['missing']",
    ] {
        assert!(cargo_config::validate(
            &config,
            rejected.as_bytes(),
            &snapshot.roots,
            &snapshot.mirror
        )
        .is_err());
    }
}

#[test]
fn configs_keep_order_legacy_preference_and_environment_conflicts_fail() {
    let home = Path::new("/home/me/.cargo");
    let invocation = Path::new("/work/project");
    let captured = [
        "/home/me/.cargo/config.toml",
        "/work/.cargo/config.toml",
        "/work/project/.cargo/config",
        "/work/project/.cargo/config.toml",
    ]
    .into_iter()
    .map(|path| (PathBuf::from(path), vec![1]))
    .collect();
    assert_eq!(
        cargo_config::ordered_configs(invocation, home, &captured),
        vec![
            home.join("config.toml"),
            PathBuf::from("/work/.cargo/config.toml"),
            invocation.join(".cargo/config")
        ]
    );
    let config = home.join("config.toml");
    let bytes = b"[profile.dev]\ndebug = 1\n";
    let environment = vec![(
        OsString::from("CARGO_PROFILE_DEV_DEBUG"),
        OsString::from("0"),
    )];
    assert!(
        cargo_config::check_environment(&config, bytes, &environment)
            .unwrap_err()
            .to_string()
            .contains("environment override")
    );
    cargo_config::check_environment(
        &config,
        b"[env]\nTYPE_HISTORY_REQUIRE_FROZEN = {value = '1', force = true}",
        &environment,
    )
    .unwrap();
}

#[cfg(unix)]
#[test]
fn cache_homes_share_lock_inodes_without_copying_credentials_or_overwriting_files() {
    let snapshot = fixture();
    let original = snapshot._owner.path().join("original-home");
    fs::create_dir_all(original.join("registry")).unwrap();
    fs::write(original.join("credentials.toml"), "secret").unwrap();
    cargo_config::share_caches(&original, &snapshot.cargo_home).unwrap();
    for name in [".package-cache", ".package-cache-mutate"] {
        assert_eq!(
            fs::metadata(original.join(name)).unwrap().ino(),
            fs::metadata(snapshot.cargo_home.join(name)).unwrap().ino()
        );
    }
    assert!(!snapshot.cargo_home.join("credentials.toml").exists());
    assert!(cargo_config::share_caches(&original, &snapshot.cargo_home).is_err());
}

#[test]
fn cargo_command_preserves_subcommand_and_uses_only_checked_configs() {
    let mut snapshot = fixture();
    snapshot
        .configs
        .push(snapshot.roots[0].join(".cargo/config.toml"));
    let mut command = Command::new("cargo");
    command.arg("test");
    snapshot.configure_cargo(&mut command).unwrap();
    assert_eq!(command.get_current_dir(), Some(Path::new("/")));
    let arguments: Vec<_> = command.get_args().collect();
    assert_eq!(arguments[0], "test");
    assert_eq!(arguments[3], "--config");
    assert_eq!(arguments[4], snapshot.mapped(&snapshot.configs[0]).unwrap());
}

#[test]
fn workspace_local_snapshot_owner_is_excluded_without_ignoring_neighboring_inputs() {
    let mut snapshot = fixture();
    let root = snapshot.roots[0].clone();
    let temporary = root.join("scratch");
    fs::create_dir_all(&temporary).unwrap();
    let owner = TempBuilder::new()
        .prefix("capture-")
        .tempdir_in(&temporary)
        .unwrap();
    // Retain the fixture source owner separately from the nested snapshot owner.
    let _source_owner = std::mem::replace(&mut snapshot._owner, owner);
    snapshot.mirror = snapshot._owner.path().join("tree");
    let neighbor = temporary.join("user-input.txt");
    fs::write(&neighbor, "captured").unwrap();
    fs::write(snapshot._owner.path().join("scratch-file"), "disposable").unwrap();
    snapshot.before = snapshot.fingerprint().unwrap();
    assert!(snapshot.before.contains_key(&neighbor));
    assert!(!snapshot
        .before
        .keys()
        .any(|path| path.starts_with(snapshot._owner.path())));
    snapshot.copy_tree(&root).unwrap();
    snapshot.verify_complete_copy().unwrap();
    assert_eq!(snapshot.before, snapshot.fingerprint().unwrap());
    fs::write(&neighbor, "changed").unwrap();
    assert_ne!(snapshot.before, snapshot.fingerprint().unwrap());
}

#[cfg(unix)]
#[test]
fn config_symlinks_keep_identity_contents_and_mapped_absolute_targets() {
    use std::os::unix::fs::symlink;

    for absolute in [false, true] {
        let mut snapshot = fixture();
        let root = snapshot.roots[0].clone();
        let config = root.join(".cargo/config.toml");
        let target = root.join("shared-config.toml");
        fs::create_dir_all(config.parent().unwrap()).unwrap();
        fs::write(&target, "[profile.dev]\ndebug = 1\n").unwrap();
        let link = if absolute {
            target.clone()
        } else {
            PathBuf::from("../shared-config.toml")
        };
        symlink(&link, &config).unwrap();
        snapshot.extra.push(config.clone());
        snapshot.configs.push(config.clone());
        snapshot.before = snapshot.fingerprint().unwrap();
        assert_eq!(snapshot.before[&config][0], 2);
        snapshot.copy_tree(&root).unwrap();
        snapshot.verify_complete_copy().unwrap();
        snapshot.check_config_preservation().unwrap();
        assert_eq!(snapshot.before, snapshot.fingerprint().unwrap());
        assert_eq!(
            fs::read_link(snapshot.mapped(&config).unwrap()).unwrap(),
            if absolute {
                snapshot.mapped(&target).unwrap()
            } else {
                link
            }
        );
        fs::write(&target, "[profile.dev]\ndebug = 2\n").unwrap();
        assert_ne!(snapshot.before, snapshot.fingerprint().unwrap());
        // A changed target cannot be copied even if the source is later restored.
        assert!(snapshot
            .copy_file(&target)
            .unwrap_err()
            .to_string()
            .contains("InputsChanged"));
        fs::write(&target, "[profile.dev]\ndebug = 1\n").unwrap();
        let copied = snapshot.mapped(&config).unwrap();
        fs::remove_file(&copied).unwrap();
        symlink("../missing-config.toml", &copied).unwrap();
        assert!(snapshot.verify_complete_copy().is_err());
    }
}

#[cfg(unix)]
#[test]
fn external_config_symlinks_capture_target_bytes_inside_or_outside_the_graph() {
    use std::os::unix::fs::symlink;

    for target_in_graph in [false, true] {
        let mut snapshot = fixture();
        let root = snapshot.roots[0].clone();
        let target = if target_in_graph {
            root.join("shared-config.toml")
        } else {
            snapshot._owner.path().join("dotfiles/cargo-config.toml")
        };
        let config = snapshot._owner.path().join("external/config.toml");
        fs::create_dir_all(config.parent().unwrap()).unwrap();
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        let captured = "[profile.dev]\ndebug = 1\n";
        fs::write(&target, captured).unwrap();
        symlink(&target, &config).unwrap();
        snapshot.extra.push(config.clone());
        snapshot.configs.push(config.clone());
        snapshot.before = snapshot.fingerprint().unwrap();
        assert_eq!(snapshot.before[&config][0], 1);
        snapshot.copy_tree(&root).unwrap();
        snapshot.copy_file(&config).unwrap();
        snapshot.verify_complete_copy().unwrap();
        snapshot.check_config_preservation().unwrap();
        let copied = snapshot.mapped(&config).unwrap();
        assert!(fs::symlink_metadata(&copied).unwrap().is_file());
        assert_eq!(fs::read_to_string(&copied).unwrap(), captured);
        assert_eq!(snapshot.before, snapshot.fingerprint().unwrap());

        fs::write(&target, "[profile.dev]\ndebug = 2\n").unwrap();
        assert_ne!(snapshot.before, snapshot.fingerprint().unwrap());
        let error = snapshot.copy_file(&config).unwrap_err();
        fs::write(&target, captured).unwrap();
        assert!(error.to_string().contains("InputsChanged"));
        assert_eq!(snapshot.before, snapshot.fingerprint().unwrap());
        assert_eq!(fs::read_to_string(&copied).unwrap(), captured);
    }
}
