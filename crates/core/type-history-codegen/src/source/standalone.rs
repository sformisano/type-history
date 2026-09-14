//! Standalone history selection over the shared package source graph.

use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

use syn::{punctuated::Punctuated, Attribute, Error, Item, Meta, Path as SynPath, Token};

use super::{reject_shape_cfg, relative, SourceDiagnostic, SourceGraph, SourceUnit};
use crate::{
    input::{self, Arguments},
    model::RecordInput,
};

/// One supported standalone history declaration before authority checks.
pub struct RecordDeclaration {
    /// Explicit stable name shared by every version of this type.
    pub stable_name: String,
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
    /// Existing files and directories covering module candidates watched by Cargo.
    pub tracked_paths: Vec<PathBuf>,
}

/// Select Type History attributes through the package module and import graph.
pub fn discover(
    root: &Path,
    library: &Path,
    facade_names: &[String],
) -> Result<RecordSource, SourceDiagnostic> {
    let graph = SourceGraph::read(root, library, facade_names)?;
    let mut declarations = Vec::new();
    let mut stable_names = BTreeSet::new();
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
            let (input, stable_name) = input::parse(arguments, record)
                .map_err(|source| declaration_error(&graph, unit, source))?;
            if !stable_names.insert(stable_name.clone()) {
                return Err(declaration_error(
                    &graph,
                    unit,
                    Error::new_spanned(&input.name, "duplicate history stable name"),
                ));
            }
            declarations.push(RecordDeclaration {
                stable_name,
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
    if graph
        .imports
        .resolves_entrypoint(&unit.module_path, path, "versioned")?
    {
        return Ok(true);
    }
    if path.is_ident("versioned")
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
    mod conditional;
    mod declarations;
    mod namespaces;
}
