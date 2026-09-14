use super::support::{failure, success, text, Fixture, LEDGER, STABLE_NAME, V1, V2};
use serde_json::json;
use std::env;
use toml::Value as TomlValue;

#[test]
fn standalone_frozen_inventory_and_warm_ledger_changes_are_authoritative() {
    let fixture = Fixture::frozen();
    let original = fixture.read(LEDGER);
    let entry = fixture.ledger()[STABLE_NAME]["1"].clone();
    let cases = [
        ("{broken".to_owned(), "invalid committed history authority"),
        (
            format!(r#"{{"{STABLE_NAME}":{{"1":{entry},"1":{entry}}}}}"#),
            "duplicate JSON key",
        ),
        (
            format!(r#"{{"{STABLE_NAME}":{{"01":{entry}}}}}"#),
            "version",
        ),
        (format!(r#"{{"{STABLE_NAME}":{{"0":{entry}}}}}"#), "version"),
        (
            format!(r#"{{"{STABLE_NAME}":{{"4294967296":{entry}}}}}"#),
            "number too large",
        ),
        (
            format!(r#"{{"{STABLE_NAME}":{{"1":{entry},"3":{entry}}}}}"#),
            "non-contiguous",
        ),
        (
            original.replace("urn:typehistory:schema:", "urn:wrong:schema:"),
            "schema",
        ),
        (
            original.replace("\"metadata\": {}", "\"metadata\": {\"role\":\"creation\"}"),
            "unknown field",
        ),
    ];
    for (ledger, diagnostic) in cases {
        assert_ne!(ledger, original, "test mutation must change authority");
        fixture.write(LEDGER, &ledger);
        failure(
            &fixture.cargo(&["build", "--locked", "--offline"]),
            diagnostic,
        );
        assert_eq!(fixture.read(LEDGER), ledger);
        fixture.write(LEDGER, &original);
        success(&fixture.cargo(&["build", "--locked", "--offline"]));
    }
    fixture.remove(LEDGER);
    failure(
        &fixture.cargo(&["build", "--locked", "--offline"]),
        "cannot read committed history authority",
    );
    assert!(!fixture.root().join(LEDGER).exists());
    fixture.write(LEDGER, &original);
    fixture.write("src/lib.rs", "");
    failure(
        &fixture.cargo(&["build", "--locked", "--offline"]),
        "committed history",
    );
    fixture.write("src/lib.rs", &V1.replace("count: u32", "count: u64"));
    for arguments in [
        vec!["build", "--locked", "--offline"],
        vec!["build", "--release", "--locked", "--offline"],
    ] {
        failure(&fixture.cargo(&arguments), "frozen");
    }
    assert_eq!(fixture.read(LEDGER), original);
    fixture.write("src/lib.rs", V1);
    success(&fixture.cargo(&["build", "--release", "--locked", "--offline"]));
}

#[test]
fn standalone_nested_shape_changes_invalidate_a_warm_consumer() {
    let fixture = Fixture::empty();
    success(&fixture.cli(&["init", "--package", "standalone-history-consumer"]));
    let nested = r#"
#[derive(history_api::Schema, Clone, PartialEq, Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Details { pub quantity: u32 }
"#;
    fixture.write("src/details.rs", nested);
    fixture.write(
        "src/lib.rs",
        r#"
mod details;
use details::Details;
#[history_api::versioned(stable_name = "billing.invoice.issued")]
pub struct Invoice { pub details: Details }
"#,
    );
    success(&fixture.cli(&["freeze", "--package", "standalone-history-consumer"]));
    let before = fixture.read(LEDGER);
    for command in ["check", "build"] {
        success(&fixture.cargo(&[command, "--locked", "--offline"]));
        fixture.write(
            "src/details.rs",
            &nested.replace("quantity: u32", "quantity: u64"),
        );
        failure(
            &fixture.cargo(&[command, "--locked", "--offline"]),
            "frozen wire shape",
        );
        assert_eq!(before, fixture.read(LEDGER));
        fixture.write("src/details.rs", nested);
    }
    // A valid ledger-only wire change must also invalidate the generated comparison.
    success(&fixture.cargo(&["build", "--locked", "--offline"]));
    let mut changed = fixture.ledger();
    changed[STABLE_NAME]["1"]["schema"]["properties"]["details"]["properties"]["quantity"] =
        json!({"type":"integer", "minimum":0, "format":"uint64"});
    fixture.set_ledger(&changed);
    failure(
        &fixture.cargo(&["build", "--locked", "--offline"]),
        "frozen wire shape",
    );

    // Move the same shape to an independent path package before changing that package alone.
    fixture.write(LEDGER, &before);
    let manifest = fixture.read("Cargo.toml");
    let parsed: TomlValue = toml::from_str(&manifest).unwrap();
    let runtime_path = parsed["dependencies"]["history_api"]["path"]
        .as_str()
        .unwrap();
    fixture.write(
        "../history-details/Cargo.toml",
        &format!(
            r#"[package]
name = "history-details"
version = "0.0.0"
edition = "2024"
publish = false
[workspace]
[dependencies]
history_api = {{ package = "type-history", path = {runtime_path:?} }}
serde = {{ version = "1.0", features = ["derive"] }}
"#
        ),
    );
    fixture.write("../history-details/src/lib.rs", nested);
    fixture.write("Cargo.toml", &manifest.replace("[dependencies]", "[dependencies]\nhistory_details = { package = \"history-details\", path = \"../history-details\" }"));
    fixture.write(
        "src/lib.rs",
        r#"
use history_details::Details;
#[history_api::versioned(stable_name = "billing.invoice.issued")]
pub struct Invoice { pub details: Details }
"#,
    );
    success(&fixture.cargo(&["generate-lockfile", "--offline"]));
    fixture.assert_independent_graph();
    for command in ["check", "build"] {
        success(&fixture.cargo(&[command, "--locked", "--offline"]));
        fixture.write(
            "../history-details/src/lib.rs",
            &nested.replace("quantity: u32", "quantity: u64"),
        );
        failure(
            &fixture.cargo(&[command, "--locked", "--offline"]),
            "frozen wire shape",
        );
        assert_eq!(fixture.read(LEDGER), before);
        fixture.write("../history-details/src/lib.rs", nested);
    }
    // An equivalent alias in that separate dependency leaves the frozen wire intact.
    let equivalent = format!(
        "pub type Quantity = u32;\n{}",
        nested.replace("quantity: u32", "quantity: Quantity")
    );
    fixture.write("../history-details/src/lib.rs", &equivalent);
    for command in ["check", "build"] {
        success(&fixture.cargo(&[command, "--locked", "--offline"]));
    }
    assert_eq!(fixture.read(LEDGER), before);
}

#[test]
fn standalone_build_policy_tracks_strict_setting_and_profile_families() {
    let fixture = Fixture::frozen();
    fixture.write("src/lib.rs", V2);
    let draft = fixture.cargo(&["build", "--locked", "--offline"]);
    success(&draft);
    assert!(text(&draft).contains("draft"));
    success(&fixture.cargo(&["build", "--profile", "dev_child", "--locked", "--offline"]));
    {
        let strict = "TYPE_HISTORY_REQUIRE_FROZEN";
        failure(
            &fixture.cargo_env(&["build", "--locked", "--offline"], &[(strict, Some("1"))]),
            "draft",
        );
        for invalid in ["", "0", "true", " 1"] {
            failure(
                &fixture.cargo_env(
                    &["build", "--locked", "--offline"],
                    &[(strict, Some(invalid))],
                ),
                "must be unset or exactly 1",
            );
            failure(
                &fixture.cargo_env(
                    &["build", "--release", "--locked", "--offline"],
                    &[(strict, Some(invalid))],
                ),
                "must be unset or exactly 1",
            );
        }
        success(&fixture.cargo(&["build", "--locked", "--offline"]));
    }
    failure(
        &fixture.cargo_env(
            &["build", "--locked", "--offline"],
            &[("TYPE_HISTORY_REQUIRE_FROZEN", Some("1"))],
        ),
        "draft",
    );
    for profile in ["release", "release_child"] {
        failure(
            &fixture.cargo_env(
                &["build", "--profile", profile, "--locked", "--offline"],
                &[("TYPE_HISTORY_SCHEMA_EXPORT", Some("1"))],
            ),
            "draft",
        );
    }
    let ledger = fixture.read(LEDGER);
    fixture.write(
        ".cargo/config.toml",
        "[env]\nTYPE_HISTORY_REQUIRE_FROZEN = { value = '1', force = true }\n",
    );
    failure(
        &fixture.cli(&["freeze", "--package", "standalone-history-consumer"]),
        "draft",
    );
    assert_eq!(fixture.read(LEDGER), ledger);
    fixture.remove(".cargo/config.toml");
    success(&fixture.cli(&["freeze", "--package", "standalone-history-consumer"]));
    success(&fixture.cargo_env(
        &["build", "--release", "--locked", "--offline"],
        &[("TYPE_HISTORY_REQUIRE_FROZEN", Some("1"))],
    ));
}

#[test]
fn standalone_setup_is_explicit_and_requires_the_build_hook() {
    let fixture = Fixture::empty();
    failure(
        &fixture.cargo(&["build", "--locked", "--offline"]),
        "authority",
    );
    assert!(!fixture.root().join(LEDGER).exists());
    fixture.write("src/lib.rs", V1);
    failure(
        &fixture.cli(&["init", "--package", "standalone-history-consumer"]),
        "without",
    );
    assert!(!fixture.root().join(LEDGER).exists());
    fixture.write("src/lib.rs", "");
    success(&fixture.cli(&["init", "--package", "standalone-history-consumer"]));
    fixture.write("src/lib.rs", V1);
    fixture.write("build.rs", "fn main() {}\n");
    failure(
        &fixture.cargo(&["build", "--locked", "--offline"]),
        "call type_history_build::compile()",
    );
    fixture.write("build.rs", "fn main() { history_build::compile(); }\n");
    success(&fixture.cargo(&["build", "--locked", "--offline"]));
}

#[test]
fn init_creates_only_the_ledger_and_preserves_existing_files() {
    let fixture = Fixture::empty();
    let init = ["init", "--package", "standalone-history-consumer"];
    assert!(!fixture.root().join("type-history.toml").exists());
    let path = env::join_paths(
        std::iter::once(fixture.root().parent().unwrap().to_owned())
            .chain(env::split_paths(&env::var_os("PATH").unwrap())),
    )
    .unwrap();
    success(&fixture.cargo_env(
        &[
            "type-history",
            "init",
            "--package",
            "standalone-history-consumer",
        ],
        &[("PATH", Some(path.to_str().unwrap()))],
    ));
    assert_eq!(fixture.read(LEDGER), "{}\n");
    assert!(!fixture.root().join("type-history.toml").exists());
    for contents in ["{}\n", "not a valid ledger"] {
        fixture.write(LEDGER, contents);
        failure(
            &fixture.cli(&init),
            "init refuses to overwrite an existing ledger",
        );
        assert_eq!(fixture.read(LEDGER), contents);
    }
    fixture.remove(LEDGER);
    for contents in ["format = 1\n", "not valid TOML ["] {
        // These are ordinary unrelated files, including a former configuration filename.
        for name in ["type-history.toml", "axis.toml"] {
            fixture.write(name, contents);
        }
        success(&fixture.cli(&init));
        assert_eq!(fixture.read(LEDGER), "{}\n");
        for name in ["type-history.toml", "axis.toml"] {
            assert_eq!(fixture.read(name), contents);
        }
        failure(&fixture.cli(&init), "overwrite");
        assert_eq!(fixture.read(LEDGER), "{}\n");
        fixture.remove(LEDGER);
    }
}

#[test]
fn unrelated_environment_and_names_do_not_change_build_or_lifecycle_policy() {
    let fixture = Fixture::empty();
    fixture.write("axis.toml", "not valid TOML [");
    let environment = [
        ("AXIS_REQUIRE_FROZEN_EVENTS", Some("invalid")),
        ("AXIS_SCHEMA_EXPORT", Some("invalid")),
    ];
    success(&fixture.cli_env(
        &["init", "--package", "standalone-history-consumer"],
        &environment,
    ));
    fixture.write(
        "src/lib.rs",
        &format!("{V1}\n#[allow(non_camel_case_types)] pub struct __axis_schema_package_user;\n"),
    );
    success(&fixture.cargo_env(&["build", "--locked", "--offline"], &environment));
    success(&fixture.cli_env(
        &["freeze", "--package", "standalone-history-consumer"],
        &environment,
    ));
    let frozen = fixture.read(LEDGER);
    fixture.write("src/lib.rs", V2);
    success(&fixture.cargo_env(&["build", "--locked", "--offline"], &environment));
    failure(
        &fixture.cargo_env(
            &["build", "--release", "--locked", "--offline"],
            &environment,
        ),
        "draft",
    );
    success(&fixture.cli_env(&["check"], &environment));
    assert_eq!(fixture.read(LEDGER), frozen);
    assert_eq!(fixture.read("axis.toml"), "not valid TOML [");
}
