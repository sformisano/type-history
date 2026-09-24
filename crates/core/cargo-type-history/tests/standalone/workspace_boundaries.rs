use super::support::{failure, success, Fixture, LEDGER};
use std::fs;
use toml::Value;

#[test]
fn temporary_directory_configuration_cannot_capture_workspace_lookup() {
    let fixture = Fixture::frozen();
    fixture.write("scratch/.cargo/config.toml", "not valid TOML [");
    let temporary = fixture.root().join("scratch");
    // A private snapshot is the capture placed inside this workspace-local TMPDIR.
    success(&fixture.cli_env(
        &["check"],
        &[
            ("TMPDIR", Some(temporary.to_str().unwrap())),
            ("TYPE_HISTORY_PRIVATE_SNAPSHOT", Some("1")),
        ],
    ));
}

#[test]
fn excluded_package_detects_ancestor_workspace_changes_during_validation() {
    let fixture = Fixture::frozen();
    let ledger = fixture.read(LEDGER);
    let mut manifest: Value = toml::from_str(&fixture.read("Cargo.toml")).unwrap();
    manifest.as_table_mut().unwrap().remove("workspace");
    fixture.write("Cargo.toml", &toml::to_string(&manifest).unwrap());
    let parent = fixture.root().parent().unwrap();
    let workspace = parent.join("Cargo.toml");
    fs::write(
        &workspace,
        "[workspace]\nmembers=[]\nexclude=['consumer']\n",
    )
    .unwrap();
    success(&fixture.cargo(&["check", "--lib", "--locked", "--offline"]));
    let changed = "[workspace]\nmembers=['consumer']\n";
    fixture.write("build.rs", &format!(
        "fn main() {{ std::fs::write({workspace:?}, {changed:?}).unwrap(); history_build::compile(); }}"
    ));
    // The copied build script mutates only this fixture's live ancestor manifest.
    // Validation must detect that Cargo's captured membership decision is stale.
    failure(
        &fixture.cli_env(
            &["check"],
            &[
                ("TMPDIR", Some(parent.to_str().unwrap())),
                ("TYPE_HISTORY_PRIVATE_SNAPSHOT", Some("1")),
            ],
        ),
        "InputsChanged",
    );
    assert_eq!(fixture.read(LEDGER), ledger);
}
