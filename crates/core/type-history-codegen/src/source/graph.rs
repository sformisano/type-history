//! Package-local source traversal shared by both declaration selectors.

use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};
use syn::{punctuated::Punctuated, Attribute, Ident, Item, Meta, Path as SynPath, Token, UseTree};

use super::imports::ImportIndex;
use super::modules::{canonical_inside, relative, ModuleDirectory};
use super::SourceDiagnostic;

/// Complete parsed source units and their shared explicit import index.
pub struct SourceGraph {
    /// Canonical package root used for source containment.
    pub package_root: PathBuf,
    /// Source units in traversal order.
    pub units: Vec<SourceUnit>,
    /// Existing authored files and directories covering absent module candidates.
    pub tracked_paths: Vec<PathBuf>,
    /// Finite explicit import/re-export resolution.
    pub imports: ImportIndex,
}

impl SourceGraph {
    /// Read one package-local library graph before selecting declarations.
    pub fn read(
        package_root: &Path,
        library_path: &Path,
        facade_names: &[String],
    ) -> Result<Self, SourceDiagnostic> {
        let package_root = package_root
            .canonicalize()
            .map_err(|source| SourceDiagnostic::Io {
                path: package_root.to_path_buf(),
                source,
            })?;
        let mut resolver = Resolver {
            package_root,
            visited: BTreeSet::new(),
            active: BTreeSet::new(),
            units: Vec::new(),
            tracked: BTreeSet::new(),
        };
        resolver.visit_file(
            library_path,
            Vec::new(),
            false,
            ModuleDirectory::root(library_path),
        )?;
        for unit in &resolver.units {
            for item in &unit.items {
                reject_reserved_generated_name(item, &unit.source_path, &resolver.package_root)?;
            }
        }
        let facades = facade_names.iter().cloned().collect();
        let imports = ImportIndex::build(&resolver.units, &facades, &resolver.package_root)?;
        Ok(Self {
            package_root: resolver.package_root,
            units: resolver.units,
            tracked_paths: resolver.tracked.into_iter().collect(),
            imports,
        })
    }
}

struct Resolver {
    package_root: PathBuf,
    visited: BTreeSet<(PathBuf, Vec<String>)>,
    active: BTreeSet<PathBuf>,
    units: Vec<SourceUnit>,
    tracked: BTreeSet<PathBuf>,
}

/// Authored items sharing a source path, module scope, and conditional status.
#[derive(Clone)]
pub struct SourceUnit {
    /// Canonical authored source path.
    pub source_path: PathBuf,
    /// Canonical crate-local module path.
    pub module_path: Vec<String>,
    /// Parsed items within this source unit.
    pub items: Vec<Item>,
    /// Whether configuration can change this unit's presence or shape.
    pub conditional: bool,
}

impl Resolver {
    fn visit_file(
        &mut self,
        path: &Path,
        module_path: Vec<String>,
        conditional: bool,
        module_dir: ModuleDirectory,
    ) -> Result<(), SourceDiagnostic> {
        // Rust resolves child modules beside the authored path, even when that
        // path is a symlink. Canonical paths identify files, not module directories.
        let canonical = canonical_inside(&self.package_root, path)?;
        self.tracked.insert(path.to_path_buf());
        let path = canonical.as_path();
        if self.active.contains(path) {
            return Err(SourceDiagnostic::ModuleCycle {
                path: relative(&self.package_root, path),
            });
        }
        if !self
            .visited
            .insert((path.to_path_buf(), module_path.clone()))
        {
            return Ok(());
        }
        self.active.insert(path.to_path_buf());
        self.tracked.insert(path.to_path_buf());
        let bytes = fs::read(path).map_err(|source| SourceDiagnostic::Io {
            path: path.to_path_buf(),
            source,
        })?;
        let source = std::str::from_utf8(&bytes).map_err(|_| SourceDiagnostic::NonUtf8 {
            path: relative(&self.package_root, path),
        })?;
        let file = syn::parse_file(source).map_err(|source| SourceDiagnostic::Parse {
            path: relative(&self.package_root, path),
            source,
        })?;
        let file_conditional = conditional || has_shape_cfg(&file.attrs)?;
        self.visit_items(&file.items, path, module_path, file_conditional, module_dir)?;
        self.active.remove(path);
        Ok(())
    }

    fn visit_items(
        &mut self,
        items: &[Item],
        source_path: &Path,
        module_scope: Vec<String>,
        conditional: bool,
        module_dir: ModuleDirectory,
    ) -> Result<(), SourceDiagnostic> {
        self.units.push(SourceUnit {
            source_path: source_path.to_path_buf(),
            module_path: module_scope.clone(),
            items: items.to_vec(),
            conditional,
        });
        for item in items {
            let Item::Mod(module) = item else { continue };
            if is_test_cfg(&module.attrs) {
                continue;
            }
            let child_conditional = conditional || has_shape_cfg(&module.attrs)?;
            let mut child_path = module_scope.clone();
            child_path.push(source_ident(&module.ident));
            if let Some((_, nested)) = &module.content {
                self.visit_items(
                    nested,
                    source_path,
                    child_path,
                    child_conditional,
                    module_dir.inline(module)?,
                )?;
                continue;
            }
            let candidates = module_dir.candidates(module)?;
            for candidate in &candidates {
                // Cargo treats a missing watched file as changed on every build.
                // An existing ancestor also notices creation of either candidate.
                if let Some(existing) = candidate.ancestors().find(|path| path.exists()) {
                    self.tracked.insert(existing.to_path_buf());
                }
            }
            if child_conditional && !candidates.iter().any(|candidate| candidate.is_file()) {
                continue;
            }
            let next = module_dir.resolve(&self.package_root, module)?;
            let child_dir = ModuleDirectory::file(&next, module);
            self.visit_file(&next, child_path, child_conditional, child_dir)?;
        }
        Ok(())
    }
}

fn reject_reserved_generated_name(
    item: &Item,
    source_path: &Path,
    package_root: &Path,
) -> Result<(), SourceDiagnostic> {
    let name = match item {
        Item::Macro(item) => item.ident.as_ref().map(ToString::to_string),
        Item::Mod(item) => Some(item.ident.to_string()),
        Item::Struct(item) => Some(item.ident.to_string()),
        Item::Enum(item) => Some(item.ident.to_string()),
        Item::Union(item) => Some(item.ident.to_string()),
        Item::Trait(item) => Some(item.ident.to_string()),
        Item::Type(item) => Some(item.ident.to_string()),
        Item::Use(item) => reserved_use_binding(&item.tree),
        _ => None,
    };
    if name.as_deref().and_then(reserved_name).is_some() {
        return Err(SourceDiagnostic::ReservedGeneratedName {
            name: name.expect("matched name"),
            path: relative(package_root, source_path),
        });
    }
    Ok(())
}

fn reserved_use_binding(tree: &UseTree) -> Option<String> {
    match tree {
        UseTree::Name(name) => reserved_name(&name.ident.to_string()),
        UseTree::Rename(rename) => reserved_name(&rename.rename.to_string()),
        UseTree::Path(path) => reserved_use_binding(&path.tree),
        UseTree::Group(group) => group.items.iter().find_map(reserved_use_binding),
        UseTree::Glob(_) => None,
    }
}

fn reserved_name(name: &str) -> Option<String> {
    let name = name.strip_prefix("r#").unwrap_or(name);
    (name.starts_with("__type_history_") || name.starts_with("__TypeHistory"))
        .then(|| name.to_owned())
}

fn is_test_cfg(attributes: &[Attribute]) -> bool {
    attributes.iter().any(|attribute| {
        attribute.path().is_ident("cfg")
            && attribute
                .meta
                .require_list()
                .is_ok_and(|list| list.tokens.to_string().replace(' ', "") == "test")
    })
}

/// Canonical identifier segments from one Rust path.
pub fn path_segments(path: &SynPath) -> Vec<String> {
    path.segments
        .iter()
        .map(|segment| source_ident(&segment.ident))
        .collect()
}

/// Canonical source spelling without the raw-identifier marker.
pub fn source_ident(identifier: &Ident) -> String {
    let value = identifier.to_string();
    value.strip_prefix("r#").unwrap_or(&value).to_owned()
}

/// Reject attributes that can change a selected declaration's presence or shape.
pub fn reject_shape_cfg(
    attributes: &[Attribute],
    construct: &'static str,
    source: &Path,
    package_root: &Path,
) -> Result<(), SourceDiagnostic> {
    if has_shape_cfg(attributes)? {
        Err(SourceDiagnostic::ConditionalSchema {
            construct,
            path: relative(package_root, source),
        })
    } else {
        Ok(())
    }
}

/// Whether cfg or cfg_attr can change persisted structure or declaration presence.
pub fn has_shape_cfg(attributes: &[Attribute]) -> Result<bool, SourceDiagnostic> {
    for attribute in attributes {
        if attribute.path().is_ident("cfg") {
            return Ok(true);
        }
        if attribute.path().is_ident("cfg_attr") {
            let nested = attribute
                .parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
                .map_err(|_| SourceDiagnostic::InvalidCfgAttr)?;
            if nested.len() < 2 || nested.iter().skip(1).any(cfg_attr_changes_shape) {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

fn cfg_attr_changes_shape(meta: &Meta) -> bool {
    let Some(name) = meta.path().get_ident().map(ToString::to_string) else {
        return true;
    };
    !matches!(
        (name.as_str(), meta),
        ("doc", Meta::NameValue(_)) | ("allow" | "warn" | "deny", Meta::List(_))
    )
}

#[cfg(test)]
mod tests {
    use super::has_shape_cfg;
    use syn::{parse_quote, Attribute};

    #[test]
    fn conditional_attributes_allow_only_documentation_and_lints() {
        let supported: Attribute = parse_quote!(
            #[cfg_attr(all(), doc = "record", allow(dead_code), warn(unused), deny(deprecated))]
        );
        assert!(!has_shape_cfg(&[supported]).expect("supported attributes"));
        for attribute in [
            parse_quote!(#[cfg_attr(all(), unknown)]),
            parse_quote!(#[cfg_attr(all(), unknown("value"))]),
            parse_quote!(#[cfg_attr(all(), unknown = "value")]),
        ] {
            assert!(has_shape_cfg(&[attribute]).expect("unsupported conditional attribute"));
        }
    }
}
