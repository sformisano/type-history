//! Execute the source setup using the published snippets and exact workspace lock.

use super::support::{failure, success, Fixture, LEDGER};
use super::tutorial::documented_block;
use serde_json::json;
use std::path::Path;
use toml::Value;

const PACKAGE: &str = "my-shop-demo-project";

#[derive(Clone, Copy)]
enum Setup {
    Ordinary,
    Renamed,
}

#[test]
fn documented_source_checkout_installs_initializes_and_freezes() {
    exercise(Setup::Ordinary);
}

#[test]
fn documented_renamed_dependency_uses_the_same_checkout() {
    exercise(Setup::Renamed);
}

fn exercise(setup: Setup) {
    let fixture = Fixture::source_checkout();
    let parent = fixture.root().parent().expect("owned setup parent");
    install(&fixture, parent, "setup:install.sh");
    let mut creation = documented_block("setup:create.sh", "sh").lines();
    let create = creation.next().expect("documented Cargo creation");
    let args = cargo_arguments(create);
    assert_eq!(args, ["new", "--lib", PACKAGE, "--edition", "2024"]);
    success(&fixture.cargo_at(parent, &args));
    assert_eq!(creation.next(), Some("cd my-shop-demo-project"));
    assert!(creation.next().is_none());

    let manifest = manifest(setup);
    println!("Documented consumer manifest:\n{manifest}");
    fixture.write("Cargo.toml", &manifest);
    fixture.write("build.rs", documented_block("setup:build.rs", "rust"));
    fixture.write("src/lib.rs", "");
    for command in documented_block("setup:initialize.sh", "sh").lines() {
        println!("Executing documented command: {command}");
        let args = cargo_arguments(command);
        if args.first() == Some(&"type-history") {
            // This is the binary installed above, without depending on the user's PATH.
            success(&fixture.cli(&args[1..]));
        } else {
            success(&fixture.cargo(&args));
        }
    }
    assert_eq!(fixture.ledger(), json!({}));
    let empty = fixture.read(LEDGER);
    failure(
        &fixture.cli(&["init", "--package", PACKAGE]),
        "init refuses to overwrite an existing ledger",
    );
    assert_eq!(fixture.read(LEDGER), empty);

    let mut source = documented_block("quick-example:v1.rs", "rust").to_owned();
    if let Setup::Renamed = setup {
        assert!(source.contains("type_history::"));
        source = source.replace("type_history::", "history_api::");
    }
    fixture.write("src/lib.rs", &source);
    success(&fixture.cargo(&["build", "--locked", "--offline"]));
    success(&fixture.cli(&["freeze", "--package", PACKAGE]));
    let frozen = fixture.read(LEDGER);
    assert!(fixture.ledger()["shop.receipt.created"]["1"].is_object());
    success(&fixture.cli(&["check", "--package", PACKAGE]));
    assert_eq!(fixture.read(LEDGER), frozen);

    // Both documented path dependencies must be live inputs to Cargo resolution.
    for dependency in ["type-history", "type-history-build"] {
        let path = format!("../type-history/crates/core/{dependency}\"");
        assert_eq!(manifest.matches(&path).count(), 1);
        fixture.write(
            "Cargo.toml",
            &manifest.replace(&path, "../missing-checkout/crate\""),
        );
        failure(
            &fixture.cargo(&["metadata", "--format-version", "1", "--locked", "--offline"]),
            "missing-checkout",
        );
        assert_eq!(fixture.read(LEDGER), frozen);
    }
    fixture.write("Cargo.toml", &manifest);
    success(&fixture.cli(&["check", "--package", PACKAGE]));
    assert_eq!(fixture.read(LEDGER), frozen);
}

fn install(fixture: &Fixture, directory: &Path, marker: &str) {
    let command = documented_block(marker, "sh").trim();
    let mut args = cargo_arguments(command);
    let root = fixture.root().parent().unwrap().join("install");
    args.extend([
        "--root",
        root.to_str().expect("UTF-8 install root"),
        "--offline",
    ]);
    println!("Installing from the exact candidate checkout and lock: {command}");
    success(&fixture.cargo_at(directory, &args));
}

fn cargo_arguments(command: &str) -> Vec<&str> {
    let mut arguments = command.split_whitespace();
    assert_eq!(arguments.next(), Some("cargo"));
    arguments.collect()
}

fn manifest(setup: Setup) -> String {
    let original = documented_block("setup:Cargo.toml", "toml");
    if let Setup::Ordinary = setup {
        return original.to_owned();
    }
    let mut manifest: Value = toml::from_str(original).expect("documented setup manifest");
    let fragment: Value = toml::from_str(documented_block("integration:Cargo.toml", "toml"))
        .expect("documented dependency variation");
    manifest["dependencies"]
        .as_table_mut()
        .unwrap()
        .remove("type-history");
    for (section, value) in fragment.as_table().unwrap() {
        manifest[section]
            .as_table_mut()
            .expect("existing dependency section")
            .extend(value.as_table().unwrap().clone());
    }
    toml::to_string(&manifest).expect("manifest containing the documented dependencies")
}
