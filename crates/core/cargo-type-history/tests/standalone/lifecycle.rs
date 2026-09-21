use super::support::{failure, success, Fixture, LEDGER, STABLE_NAME, V1, V2};
use serde_json::{json, Value};
use std::fs::OpenOptions;

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
