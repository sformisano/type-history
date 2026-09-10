//! Supported Cargo and Rust import layouts retain the same history authority.

use super::support::{failure, success, Fixture, LEDGER, V1};
use toml::{Table, Value};

fn reject_draft_and_freeze(fixture: &Fixture) {
    let empty = fixture.read(LEDGER);
    for strict in ["TYPE_HISTORY_REQUIRE_FROZEN", "AXIS_REQUIRE_FROZEN_EVENTS"] {
        failure(
            &fixture.cargo_env(&["build", "--locked", "--offline"], &[(strict, Some("1"))]),
            "draft",
        );
    }
    failure(
        &fixture.cargo(&["build", "--release", "--locked", "--offline"]),
        "draft",
    );
    assert_eq!(fixture.read(LEDGER), empty);
    success(&fixture.cli(&["freeze", "--package", "standalone-history-consumer"]));
    success(&fixture.cli(&["check"]));
    success(&fixture.cargo(&["build", "--release", "--locked", "--offline"]));
    let frozen = fixture.read(LEDGER);
    fixture.write("src/lib.rs", "");
    failure(
        &fixture.cargo(&["build", "--locked", "--offline"]),
        "missing",
    );
    assert_eq!(fixture.read(LEDGER), frozen);
}

#[test]
fn reexports_and_anonymous_imports_preserve_release_authority() {
    let fixture = Fixture::empty();
    success(&fixture.cli(&["init", "--package", "standalone-history-consumer"]));
    fixture.write(
        "src/lib.rs",
        r#"
mod records {
    use crate::api;
    #[allow(unused_imports)]
    use std::fmt::{self, Write as _};
    #[allow(unused_imports)]
    use std::io::{self, Write as _};

    #[api::versioned(stable_id = "billing.invoice.issued")]
    pub struct Invoice {
        pub value: String,
    }
}
pub use history_api as api;
"#,
    );
    success(&fixture.cargo(&["build", "--locked", "--offline"]));
    reject_draft_and_freeze(&fixture);
}

#[test]
fn macro_and_crate_namespaces_survive_aliases_and_grouped_imports() {
    let fixture = Fixture::empty();
    success(&fixture.cli(&["init", "--package", "standalone-history-consumer"]));
    let original: Value = toml::from_str(&fixture.read("Cargo.toml")).unwrap();
    let mut frozen = None;
    // Keep exercising same-name crate and macro imports through Cargo aliases.
    for (facade, imports) in [
        (
            "history_api",
            "use history_api::versioned as history_api; use history_api::Schema;",
        ),
        (
            "history_api",
            "use history_api::Schema; use history_api::versioned as history_api;",
        ),
        ("versioned", "use versioned::{versioned, Schema};"),
        ("versioned", "use versioned::{Schema, versioned};"),
        ("versioned", "use ::versioned::{self, versioned, Schema};"),
        ("versioned", "use ::versioned::{Schema, versioned, self};"),
    ] {
        let mut manifest = original.clone();
        let dependencies = manifest["dependencies"].as_table_mut().unwrap();
        let runtime = dependencies.remove("history_api").unwrap();
        dependencies.insert(facade.into(), runtime);
        fixture.write("Cargo.toml", &toml::to_string(&manifest).unwrap());
        success(&fixture.cargo(&["generate-lockfile", "--offline"]));
        let source = r#"
__IMPORTS__
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, Schema)]
#[serde(deny_unknown_fields)]
pub struct Details { pub amount: u32 }

#[__FACADE__(stable_id = "billing.invoice.issued")]
pub struct Invoice { pub details: Details }

#[__FACADE__::versioned(stable_id = "billing.invoice.copied")]
pub struct InvoiceCopy { pub amount: u32 }

#[test]
fn imported_and_qualified_macros_both_generate_readable_types() {
    use serde_json::{from_slice, to_vec};

    let invoice = Invoice { details: Details { amount: 7 } };
    let bytes = to_vec(&invoice.into_versioned()).unwrap();
    let restored = Invoice::from_versioned(from_slice(&bytes).unwrap()).unwrap();
    assert_eq!(restored.details.amount, 7);

    let copied = InvoiceCopy { amount: 9 };
    let bytes = to_vec(&copied.into_versioned()).unwrap();
    let restored = InvoiceCopy::from_versioned(from_slice(&bytes).unwrap()).unwrap();
    assert_eq!(restored.amount, 9);
}
"#
        .replace("__IMPORTS__", imports)
        .replace("__FACADE__", facade);
        fixture.write("src/lib.rs", &source);
        success(&fixture.cli(&["freeze", "--package", "standalone-history-consumer"]));
        success(&fixture.cargo(&["test", "--lib", "--locked", "--offline"]));
        if let Some(expected) = &frozen {
            assert_eq!(&fixture.read(LEDGER), expected);
        } else {
            frozen = Some(fixture.read(LEDGER));
        }
    }
}

#[test]
fn explicit_workspace_inheritance_preserves_release_authority() {
    let fixture = Fixture::empty();
    let mut manifest: Table = toml::from_str(&fixture.read("Cargo.toml")).unwrap();
    manifest.remove("workspace").unwrap();
    manifest["package"]
        .as_table_mut()
        .unwrap()
        .insert("workspace".into(), "../workspace".into());
    let dependencies = manifest["dependencies"].as_table_mut().unwrap();
    let facade = dependencies.remove("history_api").unwrap();
    dependencies.insert(
        "history_api".into(),
        Table::from_iter([("workspace".into(), true.into())]).into(),
    );
    let mut workspace = Table::from_iter([(
        "workspace".into(),
        Table::from_iter([
            ("members".into(), Value::Array(vec!["../consumer".into()])),
            ("resolver".into(), "2".into()),
            (
                "dependencies".into(),
                Table::from_iter([("history_api".into(), facade)]).into(),
            ),
        ])
        .into(),
    )]);
    workspace.insert("profile".into(), manifest.remove("profile").unwrap());
    fixture.write("Cargo.toml", &toml::to_string(&manifest).unwrap());
    fixture.write(
        "../workspace/Cargo.toml",
        &toml::to_string(&workspace).unwrap(),
    );
    success(&fixture.cargo(&["generate-lockfile", "--offline"]));
    success(&fixture.cli(&["init", "--package", "standalone-history-consumer"]));
    fixture.write("src/lib.rs", V1);
    reject_draft_and_freeze(&fixture);
}

#[test]
fn feature_selected_path_dependencies_are_captured_and_checked() {
    let fixture = Fixture::empty();
    success(&fixture.cli(&["init", "--package", "standalone-history-consumer"]));
    fixture.write(
        "../optional/Cargo.toml",
        "[package]\nname = 'optional-support'\nversion = '0.0.0'\nedition = '2024'\n[workspace]\n[dependencies]\noptional-leaf = { path = '../optional-leaf' }\n",
    );
    fixture.write(
        "../optional/src/lib.rs",
        "pub type Value = optional_leaf::Value;\n",
    );
    fixture.write(
        "../optional-leaf/Cargo.toml",
        "[package]\nname = 'optional-leaf'\nversion = '0.0.0'\nedition = '2024'\n[workspace]\n",
    );
    fixture.write("../optional-leaf/src/lib.rs", "pub type Value = String;\n");
    let mut manifest: Table = toml::from_str(&fixture.read("Cargo.toml")).unwrap();
    manifest["dependencies"].as_table_mut().unwrap().insert(
        "optional-support".into(),
        Table::from_iter([
            ("path".into(), "../optional".into()),
            ("optional".into(), true.into()),
        ])
        .into(),
    );
    manifest.insert(
        "features".into(),
        Table::from_iter([(
            "alternate".into(),
            Value::Array(vec!["dep:optional-support".into()]),
        )])
        .into(),
    );
    fixture.write("Cargo.toml", &toml::to_string(&manifest).unwrap());
    success(&fixture.cargo(&["generate-lockfile", "--offline"]));
    let source = r#"
#[history_api::versioned(stable_id = "billing.invoice.issued")]
pub struct Invoice {
    pub value: optional_support::Value,
}
"#;
    fixture.write(
        "src/lib.rs",
        &source.replace("optional_support::Value", "String"),
    );
    success(&fixture.cli(&["freeze", "--package", "standalone-history-consumer"]));
    success(&fixture.cli(&["check"]));
    fixture.write("src/lib.rs", source);
    success(&fixture.cargo(&["build", "--locked", "--offline", "--features", "alternate"]));
    let freeze = [
        "freeze",
        "--package",
        "standalone-history-consumer",
        "--features",
        "alternate",
    ];
    success(&fixture.cli(&freeze));
    success(&fixture.cli(&["check", "--features", "alternate"]));
    let frozen = fixture.read(LEDGER);
    fixture.write("../optional-leaf/src/lib.rs", "pub type Value = bool;\n");
    failure(
        &fixture.cargo(&["build", "--locked", "--offline", "--features", "alternate"]),
        "frozen",
    );
    failure(&fixture.cli(&freeze), "frozen");
    assert_eq!(fixture.read(LEDGER), frozen);
    fixture.write("../optional-leaf/src/lib.rs", "pub type Value = String;\n");
    success(&fixture.cargo(&[
        "build",
        "--release",
        "--locked",
        "--offline",
        "--features",
        "alternate",
    ]));
}
