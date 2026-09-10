use super::support::{failure, success, Fixture, ID, LEDGER, V1, V2};
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
            ID,
        ],
        vec![
            "freeze",
            "--package",
            "standalone-history-consumer",
            "--type",
            ID,
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
        ID,
        "--version",
        "1",
    ];
    success(&fixture.cli(&reset));
    let reserved = fixture.read(LEDGER);
    assert_eq!(fixture.ledger()[ID]["1"]["reset_draft"], true);
    assert_eq!(
        fixture.ledger()[ID]["1"]["schema"],
        serde_json::from_str::<Value>(&initial).unwrap()[ID]["1"]["schema"]
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
            ID,
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
    assert_eq!(fixture.read(LEDGER), reserved);
    fixture.write("src/lib.rs", V1);
    success(&fixture.cli(&[
        "reset",
        "--undo",
        "--package",
        "standalone-history-consumer",
        "--type",
        ID,
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
        ID,
        "--version",
        "1",
    ]));
    assert!(fixture.ledger()[ID]["1"]["reset_draft"].is_null());
    success(&fixture.cargo(&["build", "--release", "--locked", "--offline"]));
    fixture.remove("type-history/.schemas.lock");
    let frozen = fixture.read(LEDGER);
    success(&fixture.cli(&["check"]));
    assert!(!fixture.root().join("type-history/.schemas.lock").exists());
    assert_eq!(fixture.read(LEDGER), frozen);
}

#[test]
fn standalone_import_requires_explicit_floor_and_complete_candidate() {
    let fixture = Fixture::frozen();
    let entry = fixture.ledger()[ID]["1"].clone();
    // Simulate a package whose real retained history starts at V4.
    fixture.remove(LEDGER);
    fixture.write(
        "src/lib.rs",
        &V1.replace(
            "stable_id = \"billing.invoice.issued\"",
            "stable_id = \"billing.invoice.issued\", retained_from = v4",
        ),
    );
    fixture.write(
        "import.json",
        &serde_json::to_string_pretty(&json!({ID:{"4":entry}})).unwrap(),
    );
    failure(
        &fixture.cli(&[
            "import",
            "--package",
            "standalone-history-consumer",
            "--from",
            "import.json",
        ]),
        "--floor",
    );
    assert!(!fixture.root().join(LEDGER).exists());
    failure(
        &fixture.cli(&[
            "import",
            "--package",
            "standalone-history-consumer",
            "--from",
            "import.json",
            "--floor",
            "billing.invoice.issued=3",
        ]),
        "floor",
    );
    success(&fixture.cli(&[
        "import",
        "--package",
        "standalone-history-consumer",
        "--from",
        "import.json",
        "--floor",
        "billing.invoice.issued=4",
    ]));
    assert_eq!(
        fixture.ledger()[ID]
            .as_object()
            .unwrap()
            .keys()
            .collect::<Vec<_>>(),
        ["4"]
    );
    success(&fixture.cargo(&["build", "--release", "--locked", "--offline"]));
    let before = fixture.read(LEDGER);
    fixture.write("import.json", "{}");
    failure(
        &fixture.cli(&[
            "import",
            "--package",
            "standalone-history-consumer",
            "--from",
            "import.json",
        ]),
        "existing authority",
    );
    assert_eq!(fixture.read(LEDGER), before);
    fixture.write("src/lib.rs", V1);
    failure(
        &fixture.cargo(&["build", "--locked", "--offline"]),
        "retained floor",
    );
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
        "[env]\nAXIS_REQUIRE_FROZEN_EVENTS = { value = '1', force = true }\n",
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
            ID,
            "--version",
            "1",
        ]),
        "highest",
    );
    assert_eq!(fixture.read(LEDGER), frozen);
}
