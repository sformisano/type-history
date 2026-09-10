//! Standalone history selection over the shared package source graph.

use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

use syn::{punctuated::Punctuated, Attribute, Error, Item, Meta, Path as SynPath, Token};

use super::{path_segments, reject_shape_cfg, relative, SourceDiagnostic, SourceGraph, SourceUnit};
use crate::{
    input::{self, Arguments},
    model::RecordInput,
};

/// One supported standalone history declaration before authority checks.
pub struct RecordDeclaration {
    /// Explicit stable logical identity.
    pub identity: String,
    /// Complete authored inventory shared with the attribute macro.
    pub input: RecordInput,
    /// Canonical containing module path.
    pub module_path: Vec<String>,
    /// Package-relative authored source.
    pub source_path: PathBuf,
}

/// Parsed declarations and tracked module candidates for a standalone package.
pub struct RecordSource {
    /// Every authored standalone history.
    pub declarations: Vec<RecordDeclaration>,
    /// Authored files and all module candidates watched by Cargo.
    pub tracked_paths: Vec<PathBuf>,
}

/// Select standalone attributes through the same graph/import implementation as Axis.
pub fn discover(
    root: &Path,
    library: &Path,
    facade_names: &[String],
) -> Result<RecordSource, SourceDiagnostic> {
    let graph = SourceGraph::read(root, library, facade_names)?;
    let mut declarations = Vec::new();
    let mut identities = BTreeSet::new();
    for unit in &graph.units {
        for item in &unit.items {
            let attributes = attributes(item);
            let mut selected = None;
            for (position, attribute) in attributes.iter().enumerate() {
                if conditional_versioned(&attribute.meta, unit, &graph)? {
                    return Err(SourceDiagnostic::ConditionalSchema {
                        construct: "versioned attribute",
                        path: relative(&graph.package_root, &unit.source_path),
                    });
                }
                if !is_versioned(attribute.path(), unit, &graph)? {
                    continue;
                }
                if selected.replace(position).is_some() {
                    return Err(declaration_error(
                        &graph,
                        unit,
                        Error::new_spanned(attribute, "duplicate history declaration attribute"),
                    ));
                }
            }
            let Some(position) = selected else { continue };
            if unit.conditional {
                return Err(SourceDiagnostic::ConditionalSchema {
                    construct: "history module",
                    path: relative(&graph.package_root, &unit.source_path),
                });
            }
            reject_shape_cfg(
                attributes,
                "history record",
                &unit.source_path,
                &graph.package_root,
            )?;
            let Item::Struct(record) = item else {
                return Err(declaration_error(
                    &graph,
                    unit,
                    Error::new_spanned(item, "history requires a concrete named record"),
                ));
            };
            let arguments = attributes[position]
                .parse_args::<Arguments>()
                .map_err(|source| declaration_error(&graph, unit, source))?;
            let mut record = record.clone();
            record.attrs.remove(position);
            let (input, identity) = input::parse(arguments, record)
                .map_err(|source| declaration_error(&graph, unit, source))?;
            if !identities.insert(identity.clone()) {
                return Err(declaration_error(
                    &graph,
                    unit,
                    Error::new_spanned(&input.name, "duplicate history identity"),
                ));
            }
            declarations.push(RecordDeclaration {
                identity,
                input,
                module_path: unit.module_path.clone(),
                source_path: relative(&graph.package_root, &unit.source_path),
            });
        }
    }
    Ok(RecordSource {
        declarations,
        tracked_paths: graph.tracked_paths,
    })
}

fn is_versioned(
    path: &SynPath,
    unit: &SourceUnit,
    graph: &SourceGraph,
) -> Result<bool, SourceDiagnostic> {
    let path = path_segments(path);
    if graph
        .imports
        .resolves_entrypoint(&unit.module_path, &path, "versioned")?
    {
        return Ok(true);
    }
    if path.len() == 1
        && path[0] == "versioned"
        && graph
            .imports
            .glob_can_supply(&unit.module_path, "versioned")?
    {
        return Err(SourceDiagnostic::GlobEntrypoint {
            name: "versioned".to_owned(),
            rewrite: "use <type-history-dependency>::versioned;".to_owned(),
        });
    }
    Ok(false)
}

fn conditional_versioned(
    meta: &Meta,
    unit: &SourceUnit,
    graph: &SourceGraph,
) -> Result<bool, SourceDiagnostic> {
    if !meta.path().is_ident("cfg_attr") {
        return Ok(false);
    }
    let Meta::List(list) = meta else {
        return Err(SourceDiagnostic::InvalidCfgAttr);
    };
    let nested = list
        .parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
        .map_err(|_| SourceDiagnostic::InvalidCfgAttr)?;
    for attribute in nested.iter().skip(1) {
        if is_versioned(attribute.path(), unit, graph)?
            || conditional_versioned(attribute, unit, graph)?
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn attributes(item: &Item) -> &[Attribute] {
    match item {
        Item::Const(item) => &item.attrs,
        Item::Enum(item) => &item.attrs,
        Item::ExternCrate(item) => &item.attrs,
        Item::Fn(item) => &item.attrs,
        Item::ForeignMod(item) => &item.attrs,
        Item::Impl(item) => &item.attrs,
        Item::Macro(item) => &item.attrs,
        Item::Mod(item) => &item.attrs,
        Item::Static(item) => &item.attrs,
        Item::Struct(item) => &item.attrs,
        Item::Trait(item) => &item.attrs,
        Item::TraitAlias(item) => &item.attrs,
        Item::Type(item) => &item.attrs,
        Item::Union(item) => &item.attrs,
        Item::Use(item) => &item.attrs,
        _ => &[],
    }
}

fn declaration_error(graph: &SourceGraph, unit: &SourceUnit, source: Error) -> SourceDiagnostic {
    SourceDiagnostic::Declaration {
        path: relative(&graph.package_root, &unit.source_path),
        source,
    }
}

#[cfg(test)]
mod tests {
    use super::discover;
    use crate::source::{SourceDiagnostic, SourceGraph};
    use std::fs;

    #[test]
    fn conditional_attribute_wrappers_and_imported_modules_are_rejected() {
        let root = tempfile::tempdir().expect("package");
        let library = root.path().join("lib.rs");
        for source in [
            "#[cfg_attr(feature=\"optional\", history_api::versioned(stable_id=\"billing.invoice.issued\"))] struct Invoice {}",
            "use history_api::versioned as evolve; #[cfg_attr(feature=\"optional\", evolve(stable_id=\"billing.invoice.issued\"))] struct Invoice {}",
            "#[cfg_attr(feature=\"optional\", cfg_attr(unix, history_api::versioned(stable_id=\"billing.invoice.issued\")))] struct Invoice {}",
            "#[cfg(feature=\"optional\")] mod records { use history_api::versioned; #[versioned(stable_id=\"billing.invoice.issued\")] struct Invoice {} }",
            "#[cfg(feature=\"optional\")] mod records { use history_api as api; #[api::versioned(stable_id=\"billing.invoice.issued\")] struct Invoice {} }",
            "#![cfg(feature=\"optional\")] use history_api::versioned as evolve; #[evolve(stable_id=\"billing.invoice.issued\")] struct Invoice {}",
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
        fs::write(root.path().join("records.rs"), "use history_api::versioned as evolve; #[evolve(stable_id=\"billing.invoice.issued\")] struct Invoice {}").unwrap();
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
                #[versioned(stable_id="billing.invoice.issued")] struct Invoice {}
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
    fn attribute_aliases_and_literal_module_paths_share_discovery() {
        let root = tempfile::tempdir().expect("package");
        fs::create_dir(root.path().join("src")).unwrap();
        let library = root.path().join("src/lib.rs");
        fs::write(
            &library,
            "use history_api::versioned as evolve; #[path=\"invoice.rs\"] mod invoice;",
        )
        .unwrap();
        fs::write(root.path().join("src/invoice.rs"), "use crate::evolve; #[evolve(stable_id=\"billing.invoice.issued\")] pub struct Invoice { number: String }").unwrap();
        let source = discover(root.path(), &library, &["history_api".to_owned()]).unwrap();
        assert_eq!(source.declarations.len(), 1);
        assert_eq!(source.declarations[0].identity, "billing.invoice.issued");
        assert_eq!(source.declarations[0].module_path, ["invoice"]);
        assert!(source
            .tracked_paths
            .contains(&root.path().join("src/invoice.rs")));
    }

    #[test]
    fn explicit_prefix_reexports_resolve_independently_of_import_order() {
        let root = tempfile::tempdir().expect("package");
        let library = root.path().join("lib.rs");
        for source in [
            "pub use history_api as api; mod records { use crate::api; #[api::versioned(stable_id=\"billing.invoice.issued\")] struct Invoice {} }",
            "mod records { use crate::second as api; #[api::versioned(stable_id=\"billing.invoice.issued\")] struct Invoice {} } pub use crate::first as second; pub use history_api as first;",
            "pub use history_api as api; #[crate::api::versioned(stable_id=\"billing.invoice.issued\")] struct Invoice {}",
            "mod facade { pub use history_api::versioned; } use crate::facade::{self as api}; #[api::versioned(stable_id=\"billing.invoice.issued\")] struct Invoice {}",
            "use std::fmt::{self, Write as _}; use std::io::{self, Write as _}; use history_api::{self as api}; #[api::versioned(stable_id=\"billing.invoice.issued\")] struct Invoice {}",
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
            fs::write(&library, format!("{imports} #[history_api(stable_id=\"billing.invoice.issued\")] struct Invoice {{}}")).unwrap();
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
            fs::write(&library, format!("{imports} #[versioned(stable_id=\"billing.invoice.issued\")] struct Invoice {{}}")).unwrap();
            let facades = ["versioned".to_owned()];
            let found = discover(root.path(), &library, &facades).unwrap();
            assert_eq!(found.declarations.len(), 1, "{imports}");
            let graph = SourceGraph::read(root.path(), &library, &facades).unwrap();
            assert!(
                graph
                    .imports
                    .resolves_entrypoint(&[], &["Schema".to_owned()], "Schema")
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
                "{imports} #[versioned(stable_id=\"billing.invoice.issued\")] struct Invoice {{}}"
            );
            fs::write(&library, &source).unwrap();
            let facades = ["versioned".to_owned()];
            let found = discover(root.path(), &library, &facades).unwrap();
            assert_eq!(found.declarations.len(), 1, "{source}");
            let graph = SourceGraph::read(root.path(), &library, &facades).unwrap();
            assert!(
                graph
                    .imports
                    .resolves_entrypoint(&[], &["Schema".to_owned()], "Schema")
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
            "use versioned::versioned; #[versioned::versioned(stable_id=\"billing.invoice.issued\")] struct Invoice {}",
            "use versioned::versioned; #[::versioned::versioned(stable_id=\"billing.invoice.issued\")] struct Invoice {}",
            "use versioned::versioned as evolve; #[evolve(stable_id=\"billing.invoice.issued\")] struct Invoice {}",
            "use versioned::versioned; use versioned as api; #[api::versioned(stable_id=\"billing.invoice.issued\")] struct Invoice {}",
            "mod records { use versioned::{versioned, Schema}; #[versioned(stable_id=\"billing.invoice.issued\")] struct Invoice {} }",
            "pub use versioned::versioned; mod records { use crate::versioned; #[versioned(stable_id=\"billing.invoice.issued\")] struct Invoice {} }",
        ] {
            fs::write(&library, source).unwrap();
            let found = discover(root.path(), &library, &["versioned".to_owned()]).unwrap();
            assert_eq!(found.declarations.len(), 1, "{source}");
        }
        fs::write(
            &library,
            "use history_api::{versioned, Schema}; #[versioned(stable_id=\"billing.invoice.issued\")] struct Invoice {}",
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

    #[test]
    fn crate_names_old_attributes_and_macro_prefixes_are_not_entrypoints() {
        let root = tempfile::tempdir().expect("package");
        let library = root.path().join("lib.rs");
        for source in [
            "#[versioned(stable_id=\"billing.invoice.issued\")] struct Invoice {}",
            "use versioned::history; #[history(stable_id=\"billing.invoice.issued\")] struct Invoice {}",
            "#[versioned::history(stable_id=\"billing.invoice.issued\")] struct Invoice {}",
            "use versioned::typehistory; #[typehistory(stable_id=\"billing.invoice.issued\")] struct Invoice {}",
            "#[versioned::typehistory(stable_id=\"billing.invoice.issued\")] struct Invoice {}",
            "#[versioned::nested::versioned(stable_id=\"billing.invoice.issued\")] struct Invoice {}",
            "use versioned::versioned as evolve; #[evolve::versioned(stable_id=\"billing.invoice.issued\")] struct Invoice {}",
            "mod versioned {} #[versioned::versioned(stable_id=\"billing.invoice.issued\")] struct Invoice {}",
            "use versioned::History as versioned; #[versioned::versioned(stable_id=\"billing.invoice.issued\")] struct Invoice {}",
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
    fn same_named_imports_in_conditional_modules_are_rejected() {
        let root = tempfile::tempdir().expect("package");
        let library = root.path().join("lib.rs");
        for source in [
            "#[cfg(feature=\"optional\")] mod records { use versioned::versioned; #[versioned(stable_id=\"billing.invoice.issued\")] struct Invoice {} }",
            "#[cfg(feature=\"optional\")] mod records { use versioned::{versioned, Schema}; #[versioned(stable_id=\"billing.invoice.issued\")] struct Invoice {} }",
            "#[cfg(feature=\"optional\")] use versioned::versioned; #[versioned(stable_id=\"billing.invoice.issued\")] struct Invoice {}",
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
            "use crate::b as a; use crate::a as b; #[a::versioned(stable_id=\"billing.invoice.issued\")] struct Invoice {}",
            "use crate::a::nested as a; #[a::versioned(stable_id=\"billing.invoice.issued\")] struct Invoice {}",
            "#[cfg(feature=\"optional\")] pub use history_api as api; #[crate::api::versioned(stable_id=\"billing.invoice.issued\")] struct Invoice {}",
            "#[cfg(feature=\"optional\")] mod api { pub use history_api::versioned; } #[crate::api::versioned(stable_id=\"billing.invoice.issued\")] struct Invoice {}",
        ] {
            fs::write(&library, source).unwrap();
            assert!(
                discover(root.path(), &library, &["history_api".to_owned()]).is_err(),
                "{source}"
            );
        }
    }

    #[test]
    fn conditional_glob_duplicate_and_unsupported_declarations_fail() {
        let root = tempfile::tempdir().expect("package");
        let library = root.path().join("lib.rs");
        for source in [
            "#[cfg(any())] #[history_api::versioned(stable_id=\"billing.invoice.issued\")] struct Invoice {}",
            "use history_api::*; #[versioned(stable_id=\"billing.invoice.issued\")] struct Invoice {}",
            "#[history_api::versioned(stable_id=\"billing.invoice.issued\")] enum Invoice {}",
            "#[history_api::versioned(stable_id=\"billing.invoice.issued\")] struct Invoice {} #[history_api::versioned(stable_id=\"billing.invoice.issued\")] struct Other {}",
        ] {
            fs::write(&library, source).unwrap();
            assert!(
                discover(root.path(), &library, &["history_api".to_owned()]).is_err(),
                "{source}"
            );
        }
        fs::write(&library, "mod history_api {} #[history_api::versioned(stable_id=\"billing.invoice.issued\")] struct Invoice {}").unwrap();
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
}
