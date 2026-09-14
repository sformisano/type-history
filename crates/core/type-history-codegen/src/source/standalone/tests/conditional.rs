//! Conditional history selection through imports, reexports, and module paths.

use std::fs;

use crate::source::{standalone::discover, SourceDiagnostic};

#[test]
fn unrelated_namespace_pairs_and_conditional_attributes_are_ignored() {
    let root = tempfile::tempdir().expect("package");
    let library = root.path().join("lib.rs");
    for ordinary in [
            "use thiserror::Error; use std::error::Error;",
            "use history_api::History; use values::History; mod values { pub fn History() {} }",
            "use history_api::HasHistory as Error; use thiserror::Error;",
            "use history_api::DecodeContext as Ordinary; use std::error::Error as Ordinary;",
            "use types::Shared; use values::Shared; mod types { pub struct Shared { pub value: u32 } } mod values { pub fn Shared() -> u32 { 7 } }",
            "#[cfg(feature=\"trace\")] use tracing::instrument; #[cfg_attr(feature=\"trace\", instrument)] fn ordinary() {}",
            "#[cfg(feature=\"trace\")] use tracing::instrument as trace; use trace as instrument; #[cfg_attr(feature=\"trace\", instrument)] fn ordinary() {}",
            "mod exports { #[cfg(feature=\"trace\")] pub use tracing::instrument; } use exports::instrument; #[cfg_attr(feature=\"trace\", instrument)] fn ordinary() {}",
            "#[cfg(feature=\"trace\")] use tracing::instrument; #[cfg_attr(feature=\"trace\", cfg_attr(unix, instrument))] fn ordinary() {}",
            "#[cfg(feature=\"trace\")] extern crate tracing as instrumentation; #[cfg_attr(feature=\"trace\", instrumentation::instrument)] fn ordinary() {}",
            "#[cfg(any())] use history_api::decode as instrument; #[cfg_attr(any(), instrument)] fn ordinary() {}",
            "mod exports { pub use tracing::*; } #[cfg(any())] use exports::instrument; #[cfg_attr(any(), instrument)] fn ordinary() {}",
            "mod exports { pub use tracing::*; pub use history_api::*; } #[cfg(any())] use exports::instrument; #[cfg_attr(any(), instrument)] fn ordinary() {}",
        ] {
            let source = format!(
                "{ordinary} #[history_api::versioned(stable_name=\"billing.invoice.issued\")] struct Invoice {{}}"
            );
            fs::write(&library, &source).unwrap();
            let found = discover(root.path(), &library, &["history_api".to_owned()]).unwrap();
            assert_eq!(found.declarations.len(), 1, "{source}");
        }
}

#[test]
fn conditional_facade_fallback_requires_a_history_entrypoint_suffix() {
    let root = tempfile::tempdir().expect("package");
    let library = root.path().join("lib.rs");
    for imports in [
        "#[cfg(any())] use tracing as history_api;",
        "#[cfg(any())] mod history_api {}",
    ] {
        fs::write(&library, format!("{imports} #[cfg_attr(any(), history_api::instrument)] fn ordinary() {{}} #[::history_api::versioned(stable_name=\"billing.invoice.issued\")] struct Invoice {{}}")).unwrap();
        let found = discover(root.path(), &library, &["history_api".to_owned()]).unwrap();
        assert_eq!(found.declarations.len(), 1, "{imports}");
    }
}

#[test]
fn conditional_aliases_through_glob_reexports_are_rejected() {
    let root = tempfile::tempdir().expect("package");
    let library = root.path().join("lib.rs");
    for (imports, attribute) in [
            ("mod exports { pub use history_api::*; } #[cfg(all())] use crate::exports as api;", "api::versioned"),
            ("mod exports { pub use history_api::*; } #[cfg(all())] use exports::versioned as evolve;", "evolve"),
            ("mod exports { pub use history_api::*; } mod forwarding { pub use crate::exports::*; } #[cfg(all())] use forwarding::versioned as evolve;", "evolve"),
        ] {
            fs::write(&library, format!("{imports} #[{attribute}(stable_name=\"billing.invoice.issued\")] struct Invoice {{}}")).unwrap();
            assert!(matches!(
                discover(root.path(), &library, &["history_api".to_owned()]),
                Err(SourceDiagnostic::ConditionalSchema { .. })
            ), "{imports}");
        }
}

#[test]
fn conditional_import_relevance_keeps_every_target_and_facade_fallback() {
    let root = tempfile::tempdir().expect("package");
    let library = root.path().join("lib.rs");
    for imports in [
            "#[cfg(any())] use ordinary::decorate as evolve; #[cfg(not(any()))] use history_api::versioned as evolve;",
            "#[cfg(any())] use history_api::versioned as evolve; #[cfg(not(any()))] use ordinary::decorate as evolve;",
            "mod exports { #[cfg(any())] pub use ordinary::decorate as evolve; #[cfg(not(any()))] pub use history_api::versioned as evolve; } use exports::evolve;",
            "mod exports { #[cfg(any())] pub use history_api::versioned as evolve; } use exports::evolve;",
            "#[cfg(any())] use other as history_api; use history_api::versioned as evolve;",
            "#[cfg(any())] extern crate other as history_api; use history_api::versioned as evolve;",
            "#[cfg(any())] extern crate history_api as api; use api::versioned as evolve;",
            "#[cfg(any())] use crate::later::api as selected; use selected::versioned as evolve; mod later { pub use history_api as api; }",
        ] {
            let source = format!(
                "{imports} #[cfg_attr(any(), cfg_attr(unix, evolve(stable_name=\"billing.invoice.issued\")))] struct Invoice {{}}"
            );
            fs::write(&library, &source).unwrap();
            assert!(
                matches!(
                    discover(root.path(), &library, &["history_api".to_owned()]),
                    Err(SourceDiagnostic::ConditionalSchema { .. })
                ),
                "{source}"
            );
        }
}

#[test]
fn unrelated_conditional_imports_do_not_select_history_branches() {
    let root = tempfile::tempdir().expect("package");
    let library = root.path().join("lib.rs");
    for imports in [
            "#[cfg(unix)] mod platform { pub struct Thing; } use platform::Thing;",
            "#[cfg(unix)] mod platform { pub struct Thing; } #[cfg(not(unix))] mod platform { pub struct Thing; } use platform::Thing;",
            "#[cfg(unix)] mod platform { pub struct Thing; } use crate::platform::Thing;",
            "#[cfg(unix)] mod platform { pub struct Thing; } use platform as selected; use selected::Thing;",
            "#[cfg(unix)] mod platform { pub struct Thing; } use platform::*;",
            "#[cfg(unix)] use other::Thing as Native; use Native as Thing;",
        ] {
            let source = format!(
                "{imports} #[history_api::versioned(stable_name=\"billing.invoice.issued\")] struct Invoice {{ value: u32 }}"
            );
            fs::write(&library, &source).unwrap();
            let found = discover(root.path(), &library, &["history_api".to_owned()]).unwrap();
            assert_eq!(found.declarations.len(), 1, "{source}");
        }
}

#[test]
fn conditional_facade_selection_still_fails_through_aliases_and_globs() {
    let root = tempfile::tempdir().expect("package");
    let library = root.path().join("lib.rs");
    for source in [
            "#[cfg(any())] mod history_api {} #[history_api::versioned(stable_name=\"billing.invoice.issued\")] struct Invoice {}",
            "#[cfg(any())] mod history_api {} use history_api::versioned as evolve; #[evolve(stable_name=\"billing.invoice.issued\")] struct Invoice {}",
            "#[cfg(any())] mod history_api {} use history_api as api; use api::versioned as evolve; #[evolve(stable_name=\"billing.invoice.issued\")] struct Invoice {}",
            "mod records { #[cfg(any())] mod history_api {} use history_api::versioned as evolve; #[evolve(stable_name=\"billing.invoice.issued\")] struct Invoice {} }",
            "#[cfg(any())] mod history_api {} use history_api::*; #[versioned(stable_name=\"billing.invoice.issued\")] struct Invoice {}",
            "#[cfg(any())] use another_api as history_api; use history_api::versioned as evolve; #[evolve(stable_name=\"billing.invoice.issued\")] struct Invoice {}",
            "#[cfg(any())] mod api { pub use history_api::versioned; } use api::versioned as evolve; #[evolve(stable_name=\"billing.invoice.issued\")] struct Invoice {}",
            "#[cfg(any())] use history_api::versioned as evolve; use evolve as selected; #[selected(stable_name=\"billing.invoice.issued\")] struct Invoice {}",
        ] {
            fs::write(&library, source).unwrap();
            assert!(
                matches!(
                    discover(root.path(), &library, &["history_api".to_owned()]),
                    Err(SourceDiagnostic::ConditionalSchema { .. })
                ),
                "{source}"
            );
        }
}

#[test]
fn conditional_attribute_wrappers_and_imported_modules_are_rejected() {
    let root = tempfile::tempdir().expect("package");
    let library = root.path().join("lib.rs");
    for source in [
            "#[cfg_attr(feature=\"optional\", history_api::versioned(stable_name=\"billing.invoice.issued\"))] struct Invoice {}",
            "use history_api::versioned as evolve; #[cfg_attr(feature=\"optional\", evolve(stable_name=\"billing.invoice.issued\"))] struct Invoice {}",
            "#[cfg_attr(feature=\"optional\", cfg_attr(unix, history_api::versioned(stable_name=\"billing.invoice.issued\")))] struct Invoice {}",
            "#[cfg(feature=\"optional\")] mod records { use history_api::versioned; #[versioned(stable_name=\"billing.invoice.issued\")] struct Invoice {} }",
            "#[cfg(feature=\"optional\")] mod records { use history_api as api; #[api::versioned(stable_name=\"billing.invoice.issued\")] struct Invoice {} }",
            "#[cfg(feature=\"optional\")] mod records { extern crate history_api as api; #[api::versioned(stable_name=\"billing.invoice.issued\")] struct Invoice {} }",
            "#![cfg(feature=\"optional\")] use history_api::versioned as evolve; #[evolve(stable_name=\"billing.invoice.issued\")] struct Invoice {}",
        ] {
            fs::write(&library, source).unwrap();
            assert!(
                matches!(
                    discover(root.path(), &library, &["history_api".to_owned()]),
                    Err(SourceDiagnostic::ConditionalSchema { .. })
                ),
                "{source}"
            );
        }
    fs::write(&library, "#[cfg(feature=\"optional\")] mod records;").unwrap();
    fs::write(root.path().join("records.rs"), "use history_api::versioned as evolve; #[evolve(stable_name=\"billing.invoice.issued\")] struct Invoice {}").unwrap();
    assert!(matches!(
        discover(root.path(), &library, &["history_api".to_owned()]),
        Err(SourceDiagnostic::ConditionalSchema { .. })
    ));
}

#[test]
fn conditional_test_units_and_unrelated_attributes_stay_ignored() {
    let root = tempfile::tempdir().expect("package");
    let library = root.path().join("lib.rs");
    fs::write(
        &library,
        r#"
            #[cfg(test)] mod tests {
                use history_api::versioned;
                #[versioned(stable_name="billing.invoice.issued")] struct Invoice {}
            }
            #[cfg(feature="optional")] mod unrelated {
                use another_api::decorate;
                #[decorate] struct Ordinary {}
            }
            #[cfg_attr(feature="optional", doc="ordinary docs")] struct Public;
        "#,
    )
    .unwrap();
    assert!(discover(root.path(), &library, &["history_api".to_owned()])
        .unwrap()
        .declarations
        .is_empty());
}

#[test]
fn same_named_imports_in_conditional_modules_are_rejected() {
    let root = tempfile::tempdir().expect("package");
    let library = root.path().join("lib.rs");
    for source in [
            "#[cfg(feature=\"optional\")] mod records { use versioned::versioned; #[versioned(stable_name=\"billing.invoice.issued\")] struct Invoice {} }",
            "#[cfg(feature=\"optional\")] mod records { use versioned::{versioned, Schema}; #[versioned(stable_name=\"billing.invoice.issued\")] struct Invoice {} }",
            "#[cfg(feature=\"optional\")] use versioned::versioned; #[versioned(stable_name=\"billing.invoice.issued\")] struct Invoice {}",
        ] {
            fs::write(&library, source).unwrap();
            assert!(
                matches!(
                    discover(root.path(), &library, &["versioned".to_owned()]),
                    Err(SourceDiagnostic::ConditionalSchema { .. })
                ),
                "{source}"
            );
        }
}

#[test]
fn prefix_cycles_and_conditional_reexports_fail_before_compilation() {
    let root = tempfile::tempdir().expect("package");
    let library = root.path().join("lib.rs");
    for source in [
            "use crate::b as a; use crate::a as b; #[a::versioned(stable_name=\"billing.invoice.issued\")] struct Invoice {}",
            "use crate::a::nested as a; #[a::versioned(stable_name=\"billing.invoice.issued\")] struct Invoice {}",
            "mod a { pub use crate::b::*; } mod b { pub use crate::a::*; } #[cfg(any())] use a::versioned as evolve; #[evolve(stable_name=\"billing.invoice.issued\")] struct Invoice {}",
            "#[cfg(feature=\"optional\")] pub use history_api as api; #[crate::api::versioned(stable_name=\"billing.invoice.issued\")] struct Invoice {}",
            "#[cfg(feature=\"optional\")] mod api { pub use history_api::versioned; } #[crate::api::versioned(stable_name=\"billing.invoice.issued\")] struct Invoice {}",
        ] {
            fs::write(&library, source).unwrap();
            assert!(
                discover(root.path(), &library, &["history_api".to_owned()]).is_err(),
                "{source}"
            );
        }
}
