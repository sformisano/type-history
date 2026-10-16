//! Filesystem regressions use real Cargo commands and acknowledged boundaries.
#[cfg(unix)]
mod support;

#[cfg(unix)]
#[test]
fn ledger_transactions_reject_stale_authority_and_preserve_source_inputs() {
    use std::fs;
    use std::os::unix::fs::symlink;
    use support::{text, Fixture, LEDGER};

    let fixture = Fixture::new();
    fixture.package("a");
    fixture.package("b");
    fixture.workspace(false);
    fixture.lock();
    fixture.cli(&["freeze", "-p", "a"], true);

    let original = fixture.read("a/type-history/schemas.json");
    let stale = fixture.interleave(&["freeze", "-p", "a"], "locate", || {
        fixture.write("a/type-history/schemas.json", "MALFORMED\n");
    });
    assert!(!stale.status.success(), "stale no-op: {}", text(&stale));
    assert_eq!(fixture.read("a/type-history/schemas.json"), "MALFORMED\n");
    fixture.write("a/type-history/schemas.json", &original);

    fixture.write(
        "b/src/lib.rs",
        "compile_error!(\"new member must be checked\");\n",
    );
    fixture.cli(&["check"], true);
    let membership = fixture.interleave(&["check", "--format", "json"], "metadata", || {
        fixture.workspace(true);
        fixture.lock();
    });
    assert!(
        !membership.status.success(),
        "stale selection: {}",
        text(&membership)
    );
    fixture.cli(&["check"], false);
    fixture.write("b/src/lib.rs", "");
    fixture.cli(&["check"], true);

    let aggregate = fixture.interleave(&["check", "--format", "json"], "second-package", || {
        fixture.write(
            "a/src/lib.rs",
            "compile_error!(\"earlier package changed\");\n",
        );
    });
    assert!(
        !aggregate.status.success(),
        "stale aggregate: {}",
        text(&aggregate)
    );
    fixture.cli(&["check"], false);
    fixture.write("a/src/lib.rs", "");

    fixture.write(
        "a/build.rs",
        "use std::path::Path; fn main() { println!(\"cargo::rustc-check-cfg=cfg(input_present)\"); if Path::new(\"inputs/node_modules/flag\").exists() { println!(\"cargo::rustc-cfg=input_present\"); } }\n",
    );
    fixture.write(
        "a/src/lib.rs",
        "#[cfg(input_present)] compile_error!(\"required build input must be captured\");\n",
    );
    fixture.cli(&["check", "-p", "a"], true);
    fixture.write("a/inputs/node_modules/flag", "present");
    fixture.cargo(
        &["check", "--lib", "-p", "a", "--locked", "--offline"],
        false,
    );
    fixture.cli(&["check", "-p", "a"], false);
    fixture.write("a/build.rs", "fn main() {}\n");
    fixture.write("a/src/lib.rs", "");

    // Initialization after the initial metadata response must enter selection.
    fs::remove_file(fixture.root.join("b").join(LEDGER)).unwrap();
    fixture.cli(&["check"], true);
    let initialized = fixture.interleave(&["check"], "metadata", || {
        fixture.write("b/type-history/schemas.json", "MALFORMED\n");
    });
    assert!(
        !initialized.status.success(),
        "omitted ledger: {}",
        text(&initialized)
    );
    fixture.write("b/type-history/schemas.json", "{}\n");

    for directory in ["ordinary", "node_modules", ".next", ".git", ".schemas.lock"] {
        fixture.write(
            "a/src/lib.rs",
            &format!("#[path = \"{directory}/mod.rs\"] pub mod input;\n"),
        );
        fixture.write(
            &format!("a/src/{directory}/mod.rs"),
            "pub const INPUT: u32 = 1;\n",
        );
        fixture.cargo(
            &["check", "--lib", "-p", "a", "--locked", "--offline"],
            true,
        );
        fixture.cli(&["check", "-p", "a"], true);
    }
    fixture.write("a/src/lib.rs", "");

    let lock = fixture.root.join("a/type-history/.schemas.lock");
    fs::remove_file(&lock).unwrap();
    symlink("../unowned-file", &lock).unwrap();
    fixture.cli(&["freeze", "-p", "a"], false);
    assert!(!fixture.root.join("a/unowned-file").exists());
    fixture.write("a/type-history/schemas.json", "MALFORMED\n");
    fixture.cli(&["freeze", "-p", "a"], false);
    assert!(!fixture.root.join("a/unowned-file").exists());
    fs::remove_file(lock).unwrap();
    fixture.write("a/type-history/schemas.json", &original);
    fixture.cli(&["freeze", "-p", "a"], true);
    assert_eq!(fixture.read("a/type-history/schemas.json"), original);
    assert_eq!(fs::read_dir(&fixture.scratch).unwrap().count(), 0);
}
