//! Standalone declaration selection and source tracking.

use std::fs;

use crate::source::{standalone::discover, SourceDiagnostic};

#[test]
fn attribute_aliases_and_literal_module_paths_share_discovery() {
    let root = tempfile::tempdir().expect("package");
    fs::create_dir(root.path().join("src")).unwrap();
    let library = root.path().join("src/lib.rs");
    fs::write(
        &library,
        "use history_api::versioned as evolve; #[path=\"invoice.rs\"] mod invoice;",
    )
    .unwrap();
    fs::write(root.path().join("src/invoice.rs"), "use crate::evolve; #[evolve(derive_debug=false, stable_name=\"billing.invoice.issued\", derive_partial_eq=false,)] pub struct Invoice { number: String }").unwrap();
    let source = discover(root.path(), &library, &["history_api".to_owned()]).unwrap();
    assert_eq!(source.declarations.len(), 1);
    assert_eq!(source.declarations[0].stable_name, "billing.invoice.issued");
    assert_eq!(source.declarations[0].module_path, ["invoice"]);
    assert!(!source.declarations[0].input.derives.debug);
    assert!(!source.declarations[0].input.derives.partial_eq);
    assert!(source
        .tracked_paths
        .contains(&root.path().join("src/invoice.rs")));
}

#[test]
fn crate_names_old_attributes_and_macro_prefixes_are_not_entrypoints() {
    let root = tempfile::tempdir().expect("package");
    let library = root.path().join("lib.rs");
    for source in [
            "#[versioned(stable_name=\"billing.invoice.issued\")] struct Invoice {}",
            "use versioned::history; #[history(stable_name=\"billing.invoice.issued\")] struct Invoice {}",
            "#[versioned::history(stable_name=\"billing.invoice.issued\")] struct Invoice {}",
            "use versioned::typehistory; #[typehistory(stable_name=\"billing.invoice.issued\")] struct Invoice {}",
            "#[versioned::typehistory(stable_name=\"billing.invoice.issued\")] struct Invoice {}",
            "#[versioned::nested::versioned(stable_name=\"billing.invoice.issued\")] struct Invoice {}",
            "use versioned::versioned as evolve; #[evolve::versioned(stable_name=\"billing.invoice.issued\")] struct Invoice {}",
            "mod versioned {} #[versioned::versioned(stable_name=\"billing.invoice.issued\")] struct Invoice {}",
            "use versioned::History as versioned; #[versioned::versioned(stable_name=\"billing.invoice.issued\")] struct Invoice {}",
        ] {
            fs::write(&library, source).unwrap();
            assert!(
                discover(root.path(), &library, &["versioned".to_owned()])
                    .unwrap()
                    .declarations
                    .is_empty(),
                "{source}"
            );
        }
}

#[test]
fn conditional_glob_duplicate_and_unsupported_declarations_fail() {
    let root = tempfile::tempdir().expect("package");
    let library = root.path().join("lib.rs");
    for source in [
            "#[cfg(any())] #[history_api::versioned(stable_name=\"billing.invoice.issued\")] struct Invoice {}",
            "use history_api::*; #[versioned(stable_name=\"billing.invoice.issued\")] struct Invoice {}",
            "#[history_api::versioned(stable_name=\"billing.invoice.issued\")] enum Invoice {}",
            "#[history_api::versioned(stable_name=\"billing.invoice.issued\")] struct Invoice {} #[history_api::versioned(stable_name=\"billing.invoice.issued\")] struct Other {}",
        ] {
            fs::write(&library, source).unwrap();
            assert!(
                discover(root.path(), &library, &["history_api".to_owned()]).is_err(),
                "{source}"
            );
        }
    fs::write(&library, "mod history_api {} #[history_api::versioned(stable_name=\"billing.invoice.issued\")] struct Invoice {}").unwrap();
    assert!(discover(root.path(), &library, &["history_api".to_owned()])
        .unwrap()
        .declarations
        .is_empty());
    fs::write(&library, "struct __type_history_forged;").unwrap();
    assert!(matches!(
        discover(root.path(), &library, &[]),
        Err(SourceDiagnostic::ReservedGeneratedName { .. })
    ));
}
