use super::support::{failure, success, text, Fixture, ID, LEDGER, V1, V2};
use serde_json::json;
use toml::Value as TomlValue;

#[test]
fn standalone_frozen_inventory_and_warm_ledger_changes_are_authoritative() {
    let fixture = Fixture::frozen();
    let original = fixture.read(LEDGER);
    let entry = fixture.ledger()[ID]["1"].clone();
    let cases = [
        ("{broken".to_owned(), "invalid committed history authority"),
        (
            format!(r#"{{"{ID}":{{"1":{entry},"1":{entry}}}}}"#),
            "duplicate JSON key",
        ),
        (format!(r#"{{"{ID}":{{"01":{entry}}}}}"#), "version"),
        (format!(r#"{{"{ID}":{{"0":{entry}}}}}"#), "version"),
        (
            format!(r#"{{"{ID}":{{"4294967296":{entry}}}}}"#),
            "number too large",
        ),
        (
            format!(r#"{{"{ID}":{{"1":{entry},"3":{entry}}}}}"#),
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
#[history_api::history(stable_id = "billing.invoice.issued")]
pub struct Invoice { pub details: details::Details }
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
    changed[ID]["1"]["schema"]["properties"]["details"]["properties"]["quantity"] =
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
history_api = {{ package = "typehistory", path = {runtime_path:?} }}
serde = {{ version = "1.0", features = ["derive"] }}
"#
        ),
    );
    fixture.write("../history-details/src/lib.rs", nested);
    fixture.write("Cargo.toml", &manifest.replace("[dependencies]", "[dependencies]\nhistory_details = { package = \"history-details\", path = \"../history-details\" }"));
    fixture.write(
        "src/lib.rs",
        r#"
#[history_api::history(stable_id = "billing.invoice.issued")]
pub struct Invoice { pub details: history_details::Details }
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
fn standalone_build_policy_tracks_both_strict_settings_and_profile_families() {
    let fixture = Fixture::frozen();
    fixture.write("src/lib.rs", V2);
    let draft = fixture.cargo(&["build", "--locked", "--offline"]);
    success(&draft);
    assert!(text(&draft).contains("draft"));
    success(&fixture.cargo(&["build", "--profile", "dev_child", "--locked", "--offline"]));
    for strict in ["TYPEHISTORY_REQUIRE_FROZEN", "AXIS_REQUIRE_FROZEN_EVENTS"] {
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
            &[
                ("TYPEHISTORY_REQUIRE_FROZEN", Some("1")),
                ("AXIS_REQUIRE_FROZEN_EVENTS", Some("1")),
            ],
        ),
        "draft",
    );
    for profile in ["release", "release_child"] {
        failure(
            &fixture.cargo_env(
                &["build", "--profile", profile, "--locked", "--offline"],
                &[("TYPEHISTORY_SCHEMA_EXPORT", Some("1"))],
            ),
            "draft",
        );
    }
    let ledger = fixture.read(LEDGER);
    fixture.write(
        ".cargo/config.toml",
        "[env]\nTYPEHISTORY_REQUIRE_FROZEN = { value = '1', force = true }\n",
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
        &[
            ("TYPEHISTORY_REQUIRE_FROZEN", Some("1")),
            ("AXIS_REQUIRE_FROZEN_EVENTS", Some("1")),
        ],
    ));
}

#[test]
fn standalone_setup_is_explicit_and_rejects_conflicting_authority() {
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
        "call typehistory_build::compile()",
    );
    fixture.write("build.rs", "fn main() { history_build::compile(); }\n");
    for config in ["format = 2\n", "format = 1\nledger = 'elsewhere.json'\n"] {
        fixture.write("typehistory.toml", config);
        failure(
            &fixture.cargo(&["build", "--locked", "--offline"]),
            "typehistory",
        );
    }
    fixture.write("typehistory.toml", "format = 1\n");
    fixture.write("axis.toml", "schema_namespace = 'billing'\n");
    failure(
        &fixture.cargo(&["build", "--locked", "--offline"]),
        "axis.toml",
    );
    fixture.remove("axis.toml");
    success(&fixture.cargo(&["build", "--locked", "--offline"]));
}
