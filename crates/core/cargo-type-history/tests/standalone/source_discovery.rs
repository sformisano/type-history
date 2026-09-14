//! Supported Cargo and Rust import layouts retain the same history authority.

use super::support::{failure, success, Fixture, LEDGER, V1};
use toml::{Table, Value};

fn reject_draft_and_freeze(fixture: &Fixture) {
    let empty = fixture.read(LEDGER);
    {
        let strict = "TYPE_HISTORY_REQUIRE_FROZEN";
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

    #[api::versioned(stable_name = "billing.invoice.issued")]
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
fn ordinary_external_crate_shadows_preserve_frozen_history() {
    let fixture = Fixture::frozen();
    let frozen = fixture.read(LEDGER);
    fixture.write(
        "../ordinary-macros/Cargo.toml",
        "[package]\nname='ordinary-macros'\nversion='0.0.0'\nedition='2021'\n[workspace]\n[lib]\nproc-macro=true\n",
    );
    fixture.write(
        "../ordinary-macros/src/lib.rs",
        "use proc_macro::TokenStream; #[proc_macro_attribute] pub fn versioned(_: TokenStream, item: TokenStream) -> TokenStream { item }",
    );
    fixture.write(
        "../ordinary/Cargo.toml",
        "[package]\nname='ordinary'\nversion='0.0.0'\nedition='2021'\n[workspace]\n[dependencies]\nordinary-macros={path='../ordinary-macros'}\n",
    );
    fixture.write(
        "../ordinary/src/lib.rs",
        "pub use ordinary_macros::versioned; pub mod attrs { pub use ordinary_macros::versioned; }",
    );
    let mut manifest: Table = toml::from_str(&fixture.read("Cargo.toml")).unwrap();
    manifest["dependencies"].as_table_mut().unwrap().insert(
        "ordinary".into(),
        Table::from_iter([("path".into(), "../ordinary".into())]).into(),
    );
    fixture.write("Cargo.toml", &toml::to_string(&manifest).unwrap());
    success(&fixture.cargo(&["generate-lockfile", "--offline"]));
    let mut source = String::from(
        "use ordinary as history_api; use ordinary as first; #[history_api::versioned(stable_name=\"ordinary.root\")] pub struct Ordinary;\n",
    );
    for (index, imports) in [
        "use ordinary as history_api;",
        "use ordinary::{self as history_api};",
        "use ::ordinary as history_api;",
        "use ordinary as first; use first as history_api;",
        "use first as history_api; use ordinary as first;",
        "use ordinary::attrs as history_api;",
        "use ordinary::attrs as first; use first as history_api;",
        "use first as history_api; use ordinary::attrs as first;",
        "use ordinary as first; use first::attrs as history_api;",
        "use ordinary as first; use self::first::attrs as history_api;",
        "use crate::first as history_api;",
        "use super::first::attrs as history_api;",
    ]
    .iter()
    .enumerate()
    {
        source.push_str(&format!("mod case_{index} {{ {imports} #[history_api::versioned(stable_name=\"ordinary.nested\")] pub struct Ordinary; }}\n"));
    }
    source.push_str(&V1.replace(
        "use history_api::versioned;",
        "use ::history_api::versioned;",
    ));
    fixture.write("src/lib.rs", &source);
    success(&fixture.cargo(&["build", "--locked", "--offline"]));
    success(&fixture.cli(&["check"]));
    success(&fixture.cli(&["freeze", "--package", "standalone-history-consumer"]));
    assert_eq!(fixture.read(LEDGER), frozen);
}

#[test]
fn local_value_shadows_preserve_frozen_history() {
    let fixture = Fixture::empty();
    success(&fixture.cli(&["init", "--package", "standalone-history-consumer"]));
    let cases = [
        ("function", "fn helper() {} use helper as history_api;"),
        (
            "constant",
            "const helper: u32 = 1; use self::helper as history_api;",
        ),
        (
            "static_value",
            "static helper: u32 = 1; use self::helper as history_api;",
        ),
        ("crate_value", "use crate::helper as history_api;"),
        ("super_value", "use super::helper as history_api;"),
    ];
    let source = |with_aliases: bool, facade_first: bool| {
        let mut source = String::from("fn helper() {}\n");
        for (name, imports) in cases {
            let history = V1.replace("billing.invoice.issued", &format!("billing.invoice.{name}"));
            let history = if with_aliases {
                history
                    .replace("use history_api::versioned;", "")
                    .replace("#[versioned", "#[history_api::versioned")
            } else {
                history
            };
            let imports = if !with_aliases {
                String::new()
            } else if facade_first {
                format!("use ::history_api as history_api; {imports}")
            } else {
                format!("{imports} use ::history_api as history_api;")
            };
            source.push_str(&format!("mod {name} {{ {imports} {history} }}\n"));
        }
        source
    };
    fixture.write("src/lib.rs", &source(false, true));
    success(&fixture.cli(&["freeze", "--package", "standalone-history-consumer"]));
    let frozen = fixture.read(LEDGER);
    for facade_first in [true, false] {
        fixture.write("src/lib.rs", &source(true, facade_first));
        success(&fixture.cargo(&["build", "--locked", "--offline"]));
        success(&fixture.cli(&["check"]));
        success(&fixture.cli(&["freeze", "--package", "standalone-history-consumer"]));
        assert_eq!(fixture.read(LEDGER), frozen);
    }
}

#[test]
fn module_and_derive_imports_preserve_frozen_history_in_both_orders() {
    let fixture = Fixture::frozen();
    let frozen = fixture.read(LEDGER);
    let mut manifest: Table = toml::from_str(&fixture.read("Cargo.toml")).unwrap();
    manifest["dependencies"]
        .as_table_mut()
        .unwrap()
        .insert("thiserror".into(), "2.0.18".into());
    fixture.write("Cargo.toml", &toml::to_string(&manifest).unwrap());
    success(&fixture.cargo(&["generate-lockfile", "--offline"]));
    for imports in [
        "use history_api as Error; use thiserror::Error;",
        "use thiserror::Error; use history_api as Error;",
        "mod exports { pub use history_api::versioned; } use exports as Error; use thiserror::Error;",
        "mod exports { pub use history_api::versioned; } use thiserror::Error; use exports as Error;",
        "#[allow(non_snake_case)] mod Error { pub use history_api::versioned; } use thiserror::Error;",
        "use thiserror::Error; #[allow(non_snake_case)] mod Error { pub use history_api::versioned; }",
    ] {
        let history = V1
            .replace("use history_api::versioned;", "")
            .replace("#[versioned", "#[Error::versioned");
        fixture.write(
            "src/lib.rs",
            &format!(
                "{imports}\n#[derive(Debug, Error)] #[error(\"ordinary consumer error\")] pub struct Ordinary;\n{history}"
            ),
        );
        success(&fixture.cargo(&["build", "--locked", "--offline"]));
        success(&fixture.cli(&["check"]));
        success(&fixture.cli(&["freeze", "--package", "standalone-history-consumer"]));
        assert_eq!(fixture.read(LEDGER), frozen, "{imports}");
    }
}

#[test]
fn unrelated_macro_trait_and_type_value_imports_preserve_frozen_history() {
    let fixture = Fixture::frozen();
    let frozen = fixture.read(LEDGER);
    let mut manifest: Table = toml::from_str(&fixture.read("Cargo.toml")).unwrap();
    manifest["dependencies"]
        .as_table_mut()
        .unwrap()
        .insert("thiserror".into(), "2.0.18".into());
    fixture.write("Cargo.toml", &toml::to_string(&manifest).unwrap());
    success(&fixture.cargo(&["generate-lockfile", "--offline"]));
    for ordinary in [
        r#"
use thiserror::Error;
use std::error::Error;

#[derive(Debug, Error)]
#[error("ordinary consumer error")]
pub struct ConsumerError;

pub fn ordinary_error() -> Box<dyn Error> { Box::new(ConsumerError) }
"#,
        r#"
mod types { pub struct Shared { pub value: u32 } }
mod values { #[allow(non_snake_case)] pub fn Shared() -> u32 { 7 } }
use types::Shared;
use values::Shared;

pub fn ordinary_value() -> Shared { Shared { value: Shared() } }
"#,
        r#"
use history_api::History;
use values::History;
mod values { #[allow(non_snake_case)] pub fn History() {} }

pub fn ordinary_history() -> Option<History<()>> { History(); None }
"#,
        r#"
use history_api::HasHistory as Error;
use thiserror::Error;

#[derive(Debug, Error)]
#[error("ordinary consumer error")]
pub struct ConsumerError;

pub fn ordinary_history<T: Error>() {}
"#,
    ] {
        fixture.write("src/lib.rs", &format!("{ordinary}\n{V1}"));
        success(&fixture.cargo(&["build", "--locked", "--offline"]));
        success(&fixture.cli(&["check"]));
        success(&fixture.cli(&["freeze", "--package", "standalone-history-consumer"]));
        assert_eq!(fixture.read(LEDGER), frozen);
    }
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

#[__FACADE__(stable_name = "billing.invoice.issued")]
pub struct Invoice { pub details: Details }

#[__FACADE__::versioned(stable_name = "billing.invoice.copied")]
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
        "use optional_leaf::Value as LeafValue;\npub type Value = LeafValue;\n",
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
use optional_support::Value;
#[history_api::versioned(stable_name = "billing.invoice.issued")]
pub struct Invoice {
    pub value: Value,
}
"#;
    fixture.write(
        "src/lib.rs",
        &source
            .replace("use optional_support::Value;\n", "")
            .replace("pub value: Value", "pub value: String"),
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
