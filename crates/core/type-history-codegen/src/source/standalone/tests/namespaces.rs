//! Facade, macro, and value namespace resolution.

use std::fs;

use syn::parse_quote;

use crate::source::{standalone::discover, SourceDiagnostic, SourceGraph};

#[test]
fn ordinary_external_roots_and_modules_preserve_facade_shadows() {
    let root = tempfile::tempdir().expect("package");
    let library = root.path().join("lib.rs");
    for imports in [
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
    ] {
        let source = format!("{imports} #[history_api::versioned(stable_name=\"ordinary.fake\")] pub struct Ordinary;");
        for source in [source.clone(), format!("mod nested {{ {source} }}")] {
            fs::write(&library, &source).unwrap();
            let found = discover(root.path(), &library, &["history_api".to_owned()]).unwrap();
            assert!(found.declarations.is_empty(), "{source}");
        }
    }
    for source in [
            "use ordinary as first; mod nested { use crate::first as history_api; #[history_api::versioned(stable_name=\"ordinary.fake\")] pub struct Ordinary; }",
            "use ordinary::attrs as first; mod nested { use super::first as history_api; #[history_api::versioned(stable_name=\"ordinary.fake\")] pub struct Ordinary; }",
        ] {
            fs::write(&library, source).unwrap();
            let found = discover(root.path(), &library, &["history_api".to_owned()]).unwrap();
            assert!(found.declarations.is_empty(), "{source}");
        }
}

#[test]
fn local_value_aliases_leave_the_facade_module_reachable() {
    let root = tempfile::tempdir().expect("package");
    let library = root.path().join("lib.rs");
    for value in [
        "fn helper() {}",
        "const helper: u32 = 1;",
        "static helper: u32 = 1;",
    ] {
        for imports in [
            "use helper as history_api;",
            "use self::helper as history_api;",
            "use helper as first; use first as history_api;",
            "use first as history_api; use helper as first;",
        ] {
            for imports in [
                format!("use ::history_api as history_api; {imports}"),
                format!("{imports} use ::history_api as history_api;"),
            ] {
                let source = format!("{value} {imports} #[history_api::versioned(stable_name=\"billing.invoice.issued\")] struct Invoice {{}}");
                for source in [source.clone(), format!("mod nested {{ {source} }}")] {
                    fs::write(&library, &source).unwrap();
                    let found =
                        discover(root.path(), &library, &["history_api".to_owned()]).unwrap();
                    assert_eq!(found.declarations.len(), 1, "{source}");
                }
            }
        }
        for imports in [
            "use crate::helper as history_api; use ::history_api as history_api;",
            "use ::history_api as history_api; use crate::helper as history_api;",
        ] {
            let source = format!("{value} mod nested {{ {imports} #[history_api::versioned(stable_name=\"billing.invoice.issued\")] struct Invoice {{}} }}");
            fs::write(&library, &source).unwrap();
            let found = discover(root.path(), &library, &["history_api".to_owned()]).unwrap();
            assert_eq!(found.declarations.len(), 1, "{source}");
        }
    }
}

#[test]
fn proven_modules_and_unrelated_macros_resolve_in_both_import_orders() {
    let root = tempfile::tempdir().expect("package");
    let library = root.path().join("lib.rs");
    for imports in [
            "use history_api as Error; use thiserror::Error;",
            "use thiserror::Error; use history_api as Error;",
            "mod exports { pub use history_api::versioned; } use exports as Error; use thiserror::Error;",
            "mod exports { pub use history_api::versioned; } use thiserror::Error; use exports as Error;",
            "mod Error { pub use history_api::versioned; } use thiserror::Error;",
            "use thiserror::Error; mod Error { pub use history_api::versioned; }",
        ] {
            fs::write(&library, format!("{imports} #[Error::versioned(stable_name=\"billing.invoice.issued\")] struct Invoice {{}}")).unwrap();
            let found = discover(root.path(), &library, &["history_api".to_owned()]).unwrap();
            assert_eq!(found.declarations.len(), 1, "{imports}");
        }
}

#[test]
fn ordinary_module_aliases_do_not_expose_a_shadowed_facade() {
    let root = tempfile::tempdir().expect("package");
    let library = root.path().join("lib.rs");
    for imports in [
        "mod ordinary {} use ordinary as history_api; use thiserror::Error as history_api;",
        "mod ordinary {} use thiserror::Error as history_api; use ordinary as history_api;",
        "extern crate ordinary as history_api; use thiserror::Error as history_api;",
        "use thiserror::Error as history_api; extern crate ordinary as history_api;",
    ] {
        fs::write(&library, format!("{imports} #[history_api::versioned(stable_name=\"billing.invoice.issued\")] struct Invoice {{}}")).unwrap();
        let found = discover(root.path(), &library, &["history_api".to_owned()]).unwrap();
        assert!(found.declarations.is_empty(), "{imports}");
    }
}

#[test]
fn explicit_aliases_through_glob_reexports_are_rejected() {
    let root = tempfile::tempdir().expect("package");
    let library = root.path().join("lib.rs");
    for (imports, attribute) in [
        (
            "mod exports { pub use history_api::*; } use crate::exports as api;",
            "api::versioned",
        ),
        (
            "mod exports { pub use history_api::*; } use exports::versioned as evolve;",
            "evolve",
        ),
    ] {
        fs::write(&library, format!("{imports} #[{attribute}(stable_name=\"billing.invoice.issued\")] struct Invoice {{}}")).unwrap();
        assert!(
            matches!(
                discover(root.path(), &library, &["history_api".to_owned()]),
                Err(SourceDiagnostic::GlobEntrypoint { .. })
            ),
            "{imports}"
        );
    }
}

#[test]
fn absolute_paths_bypass_local_and_conditional_facade_shadows() {
    let root = tempfile::tempdir().expect("package");
    let library = root.path().join("lib.rs");
    for shadow in [
        "mod history_api {}",
        "#[cfg(feature=\"optional\")] mod history_api {}",
        "#[cfg(feature=\"optional\")] use another_api as history_api;",
    ] {
        for declaration in [
                "#[::history_api::versioned(stable_name=\"billing.invoice.issued\")] struct Invoice {}",
                "use ::history_api::versioned; #[versioned(stable_name=\"billing.invoice.issued\")] struct Invoice {}",
                "use ::history_api::{self as api, versioned as evolve}; #[evolve(stable_name=\"billing.invoice.issued\")] struct Invoice {}",
                "mod records { use ::history_api::versioned; #[versioned(stable_name=\"billing.invoice.issued\")] struct Invoice {} }",
            ] {
                let source = format!("{shadow} {declaration}");
                fs::write(&library, &source).unwrap();
                let found = discover(root.path(), &library, &["history_api".to_owned()]).unwrap();
                assert_eq!(found.declarations.len(), 1, "{source}");
            }
    }
    // An absolute path to another dependency cannot use a local reexport.
    for declaration in [
            "#[::other::versioned(stable_name=\"billing.invoice.issued\")] struct Invoice {}",
            "use ::other::versioned; #[versioned(stable_name=\"billing.invoice.issued\")] struct Invoice {}",
        ] {
            fs::write(
                &library,
                format!("mod other {{ pub use history_api::versioned; }} {declaration}"),
            )
            .unwrap();
            assert!(
                discover(root.path(), &library, &["history_api".to_owned()])
                    .unwrap()
                    .declarations
                    .is_empty(),
                "{declaration}"
            );
        }
}

#[test]
fn explicit_prefix_reexports_resolve_independently_of_import_order() {
    let root = tempfile::tempdir().expect("package");
    let library = root.path().join("lib.rs");
    for source in [
            "pub use history_api as api; mod records { use crate::api; #[api::versioned(stable_name=\"billing.invoice.issued\")] struct Invoice {} }",
            "mod records { use crate::second as api; #[api::versioned(stable_name=\"billing.invoice.issued\")] struct Invoice {} } pub use crate::first as second; pub use history_api as first;",
            "pub use history_api as api; #[crate::api::versioned(stable_name=\"billing.invoice.issued\")] struct Invoice {}",
            "mod facade { pub use history_api::versioned; } use crate::facade::{self as api}; #[api::versioned(stable_name=\"billing.invoice.issued\")] struct Invoice {}",
            "use std::fmt::{self, Write as _}; use std::io::{self, Write as _}; use history_api::{self as api}; #[api::versioned(stable_name=\"billing.invoice.issued\")] struct Invoice {}",
        ] {
            fs::write(&library, source).unwrap();
            let found = discover(root.path(), &library, &["history_api".to_owned()]).unwrap();
            assert_eq!(found.declarations.len(), 1, "{source}");
        }
}

#[test]
fn namespace_regression_renamed_facade_and_macro_alias() {
    let root = tempfile::tempdir().expect("package");
    let library = root.path().join("lib.rs");
    for imports in [
            "use history_api::versioned as history_api;",
            "use history_api::versioned as history_api; use history_api::Schema;",
            "use ::history_api::{self as history_api, versioned as history_api, Schema};",
            "use ::history_api::{versioned as history_api, self as history_api, Schema};",
            "use crate::exports::history_api; mod exports { pub use history_api::versioned as history_api; }",
            "use crate::exports::history_api; use history_api::Schema; mod exports { pub use history_api::versioned as history_api; }",
        ] {
            fs::write(&library, format!("{imports} #[history_api(stable_name=\"billing.invoice.issued\")] struct Invoice {{}}")).unwrap();
            let found = discover(root.path(), &library, &["history_api".to_owned()]).unwrap();
            assert_eq!(found.declarations.len(), 1, "{imports}");
        }
}

#[test]
fn namespace_regression_grouped_crate_and_macro_imports() {
    let root = tempfile::tempdir().expect("package");
    let library = root.path().join("lib.rs");
    // Rename the dependency to match the macro and exercise both namespaces.
    for imports in [
            "use ::versioned::{self, versioned, Schema};",
            "use ::versioned::{versioned, self, Schema};",
            "use crate::exports::{versioned, Schema}; mod exports { pub use ::versioned::{self, versioned, Schema}; }",
            "use crate::exports::{versioned, Schema}; mod exports { pub use ::versioned::{versioned, self, Schema}; }",
        ] {
            fs::write(&library, format!("{imports} #[versioned(stable_name=\"billing.invoice.issued\")] struct Invoice {{}}")).unwrap();
            let facades = ["versioned".to_owned()];
            let found = discover(root.path(), &library, &facades).unwrap();
            assert_eq!(found.declarations.len(), 1, "{imports}");
            let graph = SourceGraph::read(root.path(), &library, &facades).unwrap();
            assert!(
                graph
                    .imports
                    .resolves_entrypoint(&[], &parse_quote!(Schema), "Schema")
                    .unwrap()
            );
        }
}

#[test]
fn duplicate_imports_in_the_same_namespace_are_rejected() {
    let root = tempfile::tempdir().expect("package");
    let library = root.path().join("lib.rs");
    for imports in [
        "use versioned::{versioned, versioned};",
        "use versioned::{self, self};",
        "use versioned::{self, History as versioned};",
        "use versioned::{versioned, Schema as versioned};",
        "use versioned as api; use versioned as api;",
        "mod api {} use versioned as api;",
        "mod api {} mod exports { pub use versioned::versioned; } use crate::exports as api;",
    ] {
        fs::write(&library, imports).unwrap();
        assert!(
            matches!(
                discover(root.path(), &library, &["versioned".to_owned()]),
                Err(SourceDiagnostic::AmbiguousImport { .. })
            ),
            "{imports}"
        );
    }
}

#[test]
fn imported_attribute_and_same_named_crate_keep_their_namespaces() {
    let root = tempfile::tempdir().expect("package");
    let library = root.path().join("lib.rs");
    for imports in [
            "use versioned::versioned; use versioned::Schema;",
            "use versioned::{versioned, Schema};",
            "use versioned::{Schema, versioned};",
            "use versioned::{self as api, versioned, Schema};",
            "use versioned::versioned; use versioned as api; use api::Schema;",
            "use crate::exports::{versioned, Schema}; mod exports { pub use versioned::{versioned, Schema}; }",
            "use crate::exports::versioned; use versioned::Schema; mod exports { pub use versioned::versioned; }",
        ] {
            let source = format!(
                "{imports} #[versioned(stable_name=\"billing.invoice.issued\")] struct Invoice {{}}"
            );
            fs::write(&library, &source).unwrap();
            let facades = ["versioned".to_owned()];
            let found = discover(root.path(), &library, &facades).unwrap();
            assert_eq!(found.declarations.len(), 1, "{source}");
            let graph = SourceGraph::read(root.path(), &library, &facades).unwrap();
            assert!(
                graph
                    .imports
                    .resolves_entrypoint(&[], &parse_quote!(Schema), "Schema")
                    .unwrap(),
                "the macro import must not shadow the crate prefix: {source}"
            );
        }
}

#[test]
fn same_named_macro_supports_aliases_qualified_paths_and_nested_imports() {
    let root = tempfile::tempdir().expect("package");
    let library = root.path().join("lib.rs");
    for source in [
            "use versioned::versioned; #[versioned::versioned(stable_name=\"billing.invoice.issued\")] struct Invoice {}",
            "use versioned::versioned; #[::versioned::versioned(stable_name=\"billing.invoice.issued\")] struct Invoice {}",
            "use versioned::versioned as evolve; #[evolve(stable_name=\"billing.invoice.issued\")] struct Invoice {}",
            "use versioned::versioned; use versioned as api; #[api::versioned(stable_name=\"billing.invoice.issued\")] struct Invoice {}",
            "mod records { use versioned::{versioned, Schema}; #[versioned(stable_name=\"billing.invoice.issued\")] struct Invoice {} }",
            "pub use versioned::versioned; mod records { use crate::versioned; #[versioned(stable_name=\"billing.invoice.issued\")] struct Invoice {} }",
        ] {
            fs::write(&library, source).unwrap();
            let found = discover(root.path(), &library, &["versioned".to_owned()]).unwrap();
            assert_eq!(found.declarations.len(), 1, "{source}");
        }
    fs::write(
            &library,
            "use history_api::{versioned, Schema}; #[versioned(stable_name=\"billing.invoice.issued\")] struct Invoice {}",
        )
        .unwrap();
    assert_eq!(
        discover(root.path(), &library, &["history_api".to_owned()])
            .unwrap()
            .declarations
            .len(),
        1
    );
}
