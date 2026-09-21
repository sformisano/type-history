use super::support::{failure, success, Fixture, LEDGER, STABLE_NAME, V1, V2};
use serde_json::{json, Value};
use std::env;
use std::fs::{self, OpenOptions, Permissions};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Output;
use tempfile::Builder as TempBuilder;

#[test]
fn source_module_named_target_survives_lifecycle_snapshots() {
    let fixture = Fixture::empty();
    success(&fixture.cli(&["init", "--package", "standalone-history-consumer"]));
    fixture.write("src/lib.rs", "pub mod target;\n");
    fixture.write("src/target/mod.rs", V1);
    success(&fixture.cargo(&["build", "--locked", "--offline"]));
    success(&fixture.cli(&["freeze", "--package", "standalone-history-consumer"]));
    success(&fixture.cli(&["check"]));
    let frozen = fixture.read(LEDGER);
    fixture.write(
        "src/target/mod.rs",
        &V1.replace("count: u32", "count: bool"),
    );
    failure(
        &fixture.cli(&["freeze", "--package", "standalone-history-consumer"]),
        "frozen",
    );
    assert_eq!(fixture.read(LEDGER), frozen);
    fixture.write("src/target/mod.rs", V1);
    success(&fixture.cargo(&["build", "--release", "--locked", "--offline"]));
}

#[test]
fn standalone_draft_reset_undo_exact_freeze_and_read_only_check() {
    let fixture = Fixture::frozen();
    let initial = fixture.read(LEDGER);
    failure(
        &fixture.cli(&["init", "--package", "standalone-history-consumer"]),
        "overwrite",
    );
    assert_eq!(fixture.read(LEDGER), initial);
    fixture.write("src/lib.rs", V2);
    success(&fixture.cargo(&["build", "--locked", "--offline"]));
    failure(
        &fixture.cli(&[
            "reset",
            "--package",
            "standalone-history-consumer",
            "--type",
            STABLE_NAME,
            "--version",
            "1",
        ]),
        "successor",
    );
    assert_eq!(fixture.read(LEDGER), initial);
    fixture.write("src/lib.rs", V1);
    success(&fixture.cargo(&["build", "--release", "--locked", "--offline"]));
    assert_eq!(
        fixture.read(LEDGER),
        initial,
        "source-based draft discard does not edit authority"
    );

    for args in [
        vec!["reset"],
        vec!["reset", "--package", "standalone-history-consumer"],
        vec![
            "reset",
            "--package",
            "standalone-history-consumer",
            "--type",
            STABLE_NAME,
        ],
        vec![
            "freeze",
            "--package",
            "standalone-history-consumer",
            "--type",
            STABLE_NAME,
            "--version",
            "0",
        ],
    ] {
        assert!(!fixture.cli(&args).status.success());
        assert_eq!(fixture.read(LEDGER), initial);
    }
    let reset = [
        "reset",
        "--package",
        "standalone-history-consumer",
        "--type",
        STABLE_NAME,
        "--version",
        "1",
    ];
    success(&fixture.cli(&reset));
    let reserved = fixture.read(LEDGER);
    assert_eq!(fixture.ledger()[STABLE_NAME]["1"]["reset_draft"], true);
    assert_eq!(
        fixture.ledger()[STABLE_NAME]
            .as_object()
            .unwrap()
            .keys()
            .collect::<Vec<_>>(),
        ["1"]
    );
    assert_eq!(
        fixture.ledger()[STABLE_NAME]["1"]["schema"],
        serde_json::from_str::<Value>(&initial).unwrap()[STABLE_NAME]["1"]["schema"]
    );
    failure(&fixture.cli(&reset), "already");
    fixture.write("src/lib.rs", &V1.replace("count: u32", "count: u64"));
    success(&fixture.cargo(&["build", "--locked", "--offline"]));
    failure(
        &fixture.cargo(&["build", "--release", "--locked", "--offline"]),
        "reset draft",
    );
    failure(
        &fixture.cli(&["freeze", "--package", "standalone-history-consumer"]),
        "reset reservations",
    );
    failure(
        &fixture.cli(&[
            "reset",
            "--undo",
            "--package",
            "standalone-history-consumer",
            "--type",
            STABLE_NAME,
            "--version",
            "1",
        ]),
        "frozen",
    );
    assert_eq!(fixture.read(LEDGER), reserved);
    fixture.write("src/lib.rs", V2);
    failure(&fixture.cargo(&["build", "--locked", "--offline"]), "reset");
    fixture.write("src/lib.rs", "");
    failure(
        &fixture.cargo(&["build", "--release", "--locked", "--offline"]),
        "committed history",
    );
    failure(
        &fixture.cli(&["freeze", "--package", "standalone-history-consumer"]),
        "retained",
    );
    assert_eq!(fixture.read(LEDGER), reserved);
    fixture.write("src/lib.rs", V1);
    success(&fixture.cli(&[
        "reset",
        "--undo",
        "--package",
        "standalone-history-consumer",
        "--type",
        STABLE_NAME,
        "--version",
        "1",
    ]));
    assert_eq!(fixture.read(LEDGER), initial);

    success(&fixture.cli(&reset));
    fixture.write("src/lib.rs", &V1.replace("count: u32", "count: u64"));
    success(&fixture.cli(&[
        "freeze",
        "--package",
        "standalone-history-consumer",
        "--type",
        STABLE_NAME,
        "--version",
        "1",
    ]));
    assert!(fixture.ledger()[STABLE_NAME]["1"]["reset_draft"].is_null());
    success(&fixture.cargo(&["build", "--release", "--locked", "--offline"]));
    fixture.remove("type-history/.schemas.lock");
    let frozen = fixture.read(LEDGER);
    success(&fixture.cli(&["check"]));
    assert!(!fixture.root().join("type-history/.schemas.lock").exists());
    assert_eq!(fixture.read(LEDGER), frozen);
}

#[test]
fn standalone_import_requires_complete_history_from_version_one() {
    let fixture = Fixture::frozen();
    let entry = fixture.ledger()[STABLE_NAME]["1"].clone();
    let original = fixture.read(LEDGER);
    let import = [
        "import",
        "--package",
        "standalone-history-consumer",
        "--from",
        "import.json",
    ];
    // Validate both initial admission and replacement of existing authority.
    for initialized in [false, true] {
        if initialized {
            fixture.write(LEDGER, &original);
        } else {
            fixture.remove(LEDGER);
        }
        for candidate in [
            json!({STABLE_NAME:{"4":entry}}),
            json!({STABLE_NAME:{"1":entry,"3":entry}}),
        ] {
            fixture.write(
                "import.json",
                &serde_json::to_string_pretty(&candidate).unwrap(),
            );
            failure(&fixture.cli(&import), "non-contiguous");
            if initialized {
                assert_eq!(fixture.read(LEDGER), original);
            } else {
                assert!(!fixture.root().join(LEDGER).exists());
            }
        }
    }
    fixture.remove(LEDGER);
    fixture.write("import.json", &original);
    success(&fixture.cli(&import));
    assert_eq!(
        fixture.ledger()[STABLE_NAME]
            .as_object()
            .unwrap()
            .keys()
            .collect::<Vec<_>>(),
        ["1"]
    );
    success(&fixture.cargo(&["build", "--release", "--locked", "--offline"]));
    success(&fixture.cli(&["check"]));
    fixture.write("src/lib.rs", V2);
    success(&fixture.cli(&["freeze", "--package", "standalone-history-consumer"]));
    let complete = fixture.read(LEDGER);
    for initialized in [true, false] {
        if initialized {
            fixture.write(LEDGER, &original);
        } else {
            fixture.remove(LEDGER);
        }
        fixture.write("import.json", &complete);
        success(&fixture.cli(&import));
        assert_eq!(fixture.read(LEDGER), complete);
    }
    success(&fixture.cargo(&["test", "--lib", "--release", "--locked", "--offline"]));
    let before = fixture.read(LEDGER);
    fixture.write("import.json", "{}");
    failure(&fixture.cli(&import), "existing authority");
    assert_eq!(fixture.read(LEDGER), before);
    let mut changed = fixture.ledger();
    changed[STABLE_NAME]["1"]["schema"]["properties"]["count"] = json!({"type": "boolean"});
    fixture.write(
        "import.json",
        &serde_json::to_string_pretty(&changed).unwrap(),
    );
    failure(&fixture.cli(&import), "existing authority");
    assert_eq!(fixture.read(LEDGER), before);
}

#[test]
fn standalone_busy_lock_and_invalid_candidates_preserve_exact_authority() {
    let fixture = Fixture::frozen();
    let original = fixture.read(LEDGER);
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .open(fixture.root().join("type-history/.schemas.lock"))
        .unwrap();
    lock.try_lock().expect("test owns package lock");
    failure(
        &fixture.cli(&["freeze", "--package", "standalone-history-consumer"]),
        "busy",
    );
    assert_eq!(fixture.read(LEDGER), original);
    drop(lock);
    fixture.write("src/lib.rs", &V1.replace("count: u32", "count: bool"));
    failure(
        &fixture.cli(&["freeze", "--package", "standalone-history-consumer"]),
        "frozen",
    );
    assert_eq!(fixture.read(LEDGER), original);
    fixture.write("src/lib.rs", V2);
    fixture.write(
        ".cargo/config.toml",
        "[env]\nTYPE_HISTORY_REQUIRE_FROZEN = { value = '1', force = true }\n",
    );
    failure(
        &fixture.cli(&["freeze", "--package", "standalone-history-consumer"]),
        "draft",
    );
    assert_eq!(fixture.read(LEDGER), original);
    fixture.remove(".cargo/config.toml");
    success(&fixture.cli(&["freeze", "--package", "standalone-history-consumer"]));
    let frozen = fixture.read(LEDGER);
    success(&fixture.cli(&["freeze", "--package", "standalone-history-consumer"]));
    assert_eq!(fixture.read(LEDGER), frozen);
    failure(
        &fixture.cli(&[
            "reset",
            "--package",
            "standalone-history-consumer",
            "--type",
            STABLE_NAME,
            "--version",
            "1",
        ]),
        "highest",
    );
    assert_eq!(fixture.read(LEDGER), frozen);
}

#[cfg(unix)]
#[test]
fn standalone_init_rejects_a_declaration_added_before_snapshot_capture() {
    let fixture = Fixture::empty();
    let controls = TempBuilder::new()
        .prefix("type-history-init-race-")
        .tempdir()
        .unwrap();
    let incoming = controls.path().join("incoming.rs");
    let marker = controls.path().join("inserted");
    fs::write(&incoming, V1).unwrap();
    let old_path = env::var_os("PATH").unwrap();
    let rustc = executable_on_path("rustc");
    let wrapper = controls.path().join("rustc");
    fs::write(
        &wrapper,
        format!(
            r#"#!/usr/bin/env bash
set -euo pipefail
if [[ ${{1-}} == -vV && -f {lock} && ! -e {marker} ]]; then
    cp {incoming} {source}
    touch {marker}
fi
exec {rustc} "$@"
"#,
            lock = quote(&fixture.root().join("type-history/.schemas.lock")),
            marker = quote(&marker),
            incoming = quote(&incoming),
            source = quote(&fixture.root().join("src/lib.rs")),
            rustc = quote(&rustc),
        ),
    )
    .unwrap();
    fs::set_permissions(&wrapper, Permissions::from_mode(0o755)).unwrap();
    let path = env::join_paths(
        std::iter::once(controls.path().to_owned()).chain(env::split_paths(&old_path)),
    )
    .unwrap();
    let output = fixture.cli_env(
        &["init", "--package", "standalone-history-consumer"],
        &[("PATH", Some(path.to_str().unwrap()))],
    );
    assert!(
        marker.exists(),
        "compiler probe must inject the declaration"
    );
    failure(
        &output,
        "init requires a package without history declarations",
    );
    assert_eq!(fixture.read("src/lib.rs"), V1);
    assert!(!fixture.root().join(LEDGER).exists());

    fixture.write("src/lib.rs", "");
    success(&fixture.cli(&["init", "--package", "standalone-history-consumer"]));
    assert_eq!(fixture.read(LEDGER), "{}\n");
    let initialized = fixture.read(LEDGER);
    failure(
        &fixture.cli(&["init", "--package", "standalone-history-consumer"]),
        "overwrite",
    );
    assert_eq!(fixture.read(LEDGER), initialized);
}

#[test]
fn standalone_draft_edits_rollback_removal_and_incomplete_discard() {
    let fixture = Fixture::frozen();
    let authority = fixture.read(LEDGER);

    for source in [V2.to_owned(), V2.replace("u64", "u128")] {
        fixture.write("src/lib.rs", &source);
        check_and_build(&fixture, None);
        assert_eq!(fixture.read(LEDGER), authority);
    }

    fixture.write(
        "src/lib.rs",
        &V1.replace("pub count: u32", "pub count: u128"),
    );
    check_and_build(&fixture, Some("frozen"));
    assert_eq!(fixture.read(LEDGER), authority);
    fixture.write("src/lib.rs", V2);
    failure(
        &fixture.cargo(&["build", "--release", "--locked", "--offline"]),
        "draft",
    );
    assert_eq!(fixture.read(LEDGER), authority);

    fixture.write("src/lib.rs", V1);
    check_and_build(&fixture, None);
    assert_eq!(fixture.read(LEDGER), authority);
    fixture.write(
        "src/lib.rs",
        &format!("{V1}\nfn current_alias(value: Invoice) -> u32 {{ value.count }}\n"),
    );
    check_and_build(&fixture, None);
    assert_eq!(fixture.read(LEDGER), authority);

    let removal = V1.replace(
        "pub legacy: String,",
        "#[history(removed_in = v2)]\n    pub legacy: String,",
    );
    fixture.write("src/lib.rs", &removal);
    check_and_build(&fixture, None);
    assert_eq!(fixture.read(LEDGER), authority);
    fixture.write("src/lib.rs", V1);
    check_and_build(&fixture, None);

    let successor = V2.replace(
        "pub revision: u32,",
        "pub revision: u32,\n    #[history(added_in = v3, backfill_value = false)]\n    pub ready: bool,",
    );
    fixture.write("src/lib.rs", &successor);
    check_and_build(&fixture, Some("successor"));
    assert_eq!(fixture.read(LEDGER), authority);
}

#[test]
fn standalone_selected_reset_preserves_earlier_and_unrelated_history() {
    const OTHER: &str = "billing.invoice.reviewed";
    let fixture = Fixture::empty();
    success(&fixture.cli(&["init", "--package", "standalone-history-consumer"]));
    fixture.write("src/lib.rs", &selected_reset_v1_source("bool"));
    success(&fixture.cli(&["freeze", "--package", "standalone-history-consumer"]));
    let v2_source = selected_reset_source("u32", "u64", "widen", "bool");
    fixture.write("src/lib.rs", &v2_source);
    success(&fixture.cli(&["freeze", "--package", "standalone-history-consumer"]));
    let saved = fixture.ledger();
    let before = fixture.read(LEDGER);

    for (source, expected) in [
        (
            selected_reset_source("u16", "u64", "widen", "bool"),
            "frozen",
        ),
        (
            selected_reset_source("u32", "u64", "widen", "u32"),
            "frozen",
        ),
        (
            selected_reset_source("u32", "u64", "missing_widen", "bool"),
            "missing_widen",
        ),
    ] {
        fixture.write("src/lib.rs", &source);
        failure(&selected(&fixture, "reset", "2"), expected);
        assert_eq!(fixture.read(LEDGER), before);
    }

    fixture.write("src/lib.rs", &v2_source.replace("u64", "u128"));
    success(&selected(&fixture, "reset", "2"));
    let reserved = fixture.ledger();
    assert_eq!(reserved[STABLE_NAME]["1"], saved[STABLE_NAME]["1"]);
    assert_eq!(reserved[OTHER], saved[OTHER]);
    assert_eq!(
        reserved[STABLE_NAME]["2"]["schema"],
        saved[STABLE_NAME]["2"]["schema"]
    );
    assert_eq!(reserved[STABLE_NAME]["2"]["reset_draft"], true);
    success(&selected(&fixture, "freeze", "2"));
    let frozen = fixture.ledger();
    assert_eq!(frozen[STABLE_NAME]["1"], saved[STABLE_NAME]["1"]);
    assert_eq!(frozen[OTHER], saved[OTHER]);
    assert_ne!(
        frozen[STABLE_NAME]["2"]["schema"],
        saved[STABLE_NAME]["2"]["schema"]
    );
}

#[test]
fn standalone_never_frozen_record_can_be_edited_and_discarded() {
    let fixture = Fixture::frozen();
    let authority = fixture.read(LEDGER);
    let scratch = r#"
#[history_api::versioned(stable_name = "billing.scratch")]
pub struct Scratch { pub value: u32 }
"#;
    for source in [
        format!("{V1}\n{scratch}"),
        format!("{V1}\n{}", scratch.replace("u32", "u64")),
        V1.to_owned(),
    ] {
        fixture.write("src/lib.rs", &source);
        check_and_build(&fixture, None);
        assert_eq!(fixture.read(LEDGER), authority);
    }
}

fn check_and_build(fixture: &Fixture, expected: Option<&str>) {
    for command in ["check", "build"] {
        let output = fixture.cargo(&[command, "--locked", "--offline"]);
        match expected {
            Some(diagnostic) => failure(&output, diagnostic),
            None => success(&output),
        }
    }
}

fn selected(fixture: &Fixture, action: &str, version: &str) -> Output {
    fixture.cli(&[
        action,
        "--package",
        "standalone-history-consumer",
        "--type",
        STABLE_NAME,
        "--version",
        version,
    ])
}

fn selected_reset_v1_source(other: &str) -> String {
    format!(
        r#"use history_api::versioned;
#[versioned(stable_name = "{STABLE_NAME}")]
pub struct Invoice {{ pub count: u32 }}
#[versioned(stable_name = "billing.invoice.reviewed")]
pub struct Reviewed {{ pub accepted: {other} }}
"#
    )
}

fn selected_reset_source(previous: &str, current: &str, callback: &str, other: &str) -> String {
    format!(
        r#"use history_api::versioned;
#[versioned(stable_name = "{STABLE_NAME}")]
pub struct Invoice {{
    #[history(updated_in = v2, previous_type = {previous}, backfill_fn = {callback})]
    pub count: {current},
}}
fn widen(previous: &InvoiceV1) -> Result<{current}, std::convert::Infallible> {{
    Ok(previous.count.into())
}}
#[versioned(stable_name = "billing.invoice.reviewed")]
pub struct Reviewed {{ pub accepted: {other} }}
"#
    )
}

#[cfg(unix)]
fn executable_on_path(name: &str) -> PathBuf {
    let candidate = env::split_paths(&env::var_os("PATH").unwrap())
        .map(|directory| directory.join(name))
        .find(|candidate| candidate.is_file())
        .unwrap();
    if candidate.is_absolute() {
        candidate
    } else {
        env::current_dir().unwrap().join(candidate)
    }
}

#[cfg(unix)]
fn quote(path: &Path) -> String {
    format!("'{}'", path.to_str().unwrap().replace('\'', "'\\''"))
}
