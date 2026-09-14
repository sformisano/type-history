//! Build the finite import index from source units and flattened use trees.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use syn::{Item, UseTree};

use super::{ConditionalBinding, ImportIndex, Namespace, Symbol};
use crate::source::{has_shape_cfg, relative, source_ident, SourceDiagnostic, SourceUnit};

#[derive(Clone, Copy)]
enum ImportKind {
    Unconditional,
    Conditional,
    ConditionalUnit,
}

impl ImportIndex {
    pub(in crate::source) fn build(
        units: &[SourceUnit],
        facades: &BTreeSet<String>,
        package_root: &Path,
    ) -> Result<Self, SourceDiagnostic> {
        let mut output = Self {
            bindings: BTreeMap::new(),
            conditional_bindings: BTreeMap::new(),
            conditional_unit_imports: BTreeMap::new(),
            local_definitions: BTreeSet::new(),
            local_values: BTreeSet::new(),
            globs: BTreeMap::new(),
            facades: facades.clone(),
        };
        let mut pending_imports = Vec::new();
        for unit in units {
            for item in &unit.items {
                let value_name = match item {
                    Item::Fn(item) => Some(&item.sig.ident),
                    Item::Const(item) => Some(&item.ident),
                    Item::Static(item) => Some(&item.ident),
                    _ => None,
                };
                if let Some(name) = value_name {
                    let mut key = unit.module_path.clone();
                    key.push(source_ident(name));
                    output.local_values.insert(key);
                }
                match item {
                    Item::Mod(item_mod) => {
                        let mut key = unit.module_path.clone();
                        key.push(source_ident(&item_mod.ident));
                        if unit.conditional || has_shape_cfg(&item_mod.attrs)? {
                            output.conditional_bindings.entry(key).or_insert_with(|| {
                                ConditionalBinding {
                                    construct: "facade-shadowing module",
                                    path: relative(package_root, &unit.source_path),
                                    targets: Vec::new(),
                                }
                            });
                            continue;
                        }
                        if !output.local_definitions.insert(key.clone()) {
                            return Err(SourceDiagnostic::AmbiguousImport {
                                name: key.join("::"),
                            });
                        }
                    }
                    Item::ExternCrate(item_extern) => {
                        let local = item_extern
                            .rename
                            .as_ref()
                            .map_or(&item_extern.ident, |(_, rename)| rename);
                        if local == "_" {
                            continue;
                        }
                        let mut key = unit.module_path.clone();
                        key.push(source_ident(local));
                        let target = Symbol {
                            path: vec![source_ident(&item_extern.ident)],
                            external: true,
                        };
                        if unit.conditional || has_shape_cfg(&item_extern.attrs)? {
                            output
                                .conditional_bindings
                                .entry(key)
                                .or_insert_with(|| ConditionalBinding {
                                    construct: "facade-shadowing extern crate alias",
                                    path: relative(package_root, &unit.source_path),
                                    targets: Vec::new(),
                                })
                                .targets
                                .push(target);
                            continue;
                        }
                        if output.local_definitions.contains(&key)
                            || output.bindings.insert(key.clone(), vec![target]).is_some()
                        {
                            return Err(SourceDiagnostic::AmbiguousImport {
                                name: key.join("::"),
                            });
                        }
                    }
                    _ => {}
                }
            }
        }
        for unit in units {
            for item in &unit.items {
                let Item::Use(item_use) = item else { continue };
                let mut leaves = Vec::new();
                let external = item_use.leading_colon.is_some();
                flatten_use(&item_use.tree, Vec::new(), &mut leaves)?;
                let kind = if unit.conditional {
                    ImportKind::ConditionalUnit
                } else if has_shape_cfg(&item_use.attrs)? {
                    ImportKind::Conditional
                } else {
                    ImportKind::Unconditional
                };
                for leaf in leaves {
                    let target =
                        output.normalize(&unit.module_path, &leaf.target, external, None)?;
                    if leaf.glob {
                        output
                            .globs
                            .entry(unit.module_path.clone())
                            .or_default()
                            .push(target);
                        continue;
                    }
                    let mut key = unit.module_path.clone();
                    key.push(leaf.local.expect("non-glob leaf"));
                    if matches!(kind, ImportKind::Conditional) {
                        output
                            .conditional_bindings
                            .entry(key.clone())
                            .or_insert_with(|| ConditionalBinding {
                                construct: "facade-shadowing import",
                                path: relative(package_root, &unit.source_path),
                                targets: Vec::new(),
                            });
                    }
                    let targets = output.import_targets(&key, kind);
                    pending_imports.push((
                        key,
                        kind,
                        targets.len(),
                        unit.module_path.clone(),
                        leaf.target,
                        external,
                    ));
                    targets.push(target);
                }
            }
        }
        // A same-named macro can arrive through a reexport declared later.
        // Revisit every import with the complete index so its crate
        // prefixes are independent of declaration and module traversal order.
        for _ in 0..=pending_imports.len() {
            let mut changed = false;
            for (key, kind, index, module, raw, external) in &pending_imports {
                let target = output.normalize(module, raw, *external, None)?;
                let stored = &mut output.import_targets(key, *kind)[*index];
                if *stored != target {
                    *stored = target;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        output.validate_namespaces()?;
        Ok(output)
    }

    fn import_targets(&mut self, key: &[String], kind: ImportKind) -> &mut Vec<Symbol> {
        match kind {
            ImportKind::Unconditional => self.bindings.entry(key.to_vec()).or_default(),
            ImportKind::Conditional => {
                &mut self
                    .conditional_bindings
                    .get_mut(key)
                    .expect("conditional import metadata")
                    .targets
            }
            ImportKind::ConditionalUnit => self
                .conditional_unit_imports
                .entry(key.to_vec())
                .or_default(),
        }
    }

    fn validate_namespaces(&self) -> Result<(), SourceDiagnostic> {
        for (key, targets) in &self.bindings {
            if targets.len() < 2 && !self.local_definitions.contains(key) {
                continue;
            }
            // Rustc decides arbitrary external namespaces. Our finite model
            // only diagnoses collisions involving history authority.
            let mut relevant = false;
            for target in targets {
                relevant |= self
                    .history_relevance(target, &mut BTreeSet::new())?
                    .is_relevant();
            }
            if !relevant {
                continue;
            }
            let mut module = self.local_definitions.contains(key);
            let mut item = false;
            for target in targets {
                let imports_item =
                    self.resolve(target.clone(), Namespace::Macro(None), &mut BTreeSet::new())?;
                let imports_module =
                    self.resolve(target.clone(), Namespace::Module, &mut BTreeSet::new())?;
                if (item && imports_item) || (module && imports_module) {
                    return Err(SourceDiagnostic::AmbiguousImport {
                        name: key.join("::"),
                    });
                }
                item |= imports_item;
                module |= imports_module;
            }
        }
        Ok(())
    }
}

struct UseLeaf {
    target: Vec<String>,
    local: Option<String>,
    glob: bool,
}

fn flatten_use(
    tree: &UseTree,
    prefix: Vec<String>,
    output: &mut Vec<UseLeaf>,
) -> Result<(), SourceDiagnostic> {
    match tree {
        UseTree::Path(path) => {
            let mut prefix = prefix;
            prefix.push(source_ident(&path.ident));
            flatten_use(&path.tree, prefix, output)
        }
        UseTree::Name(name) => {
            let mut target = prefix;
            if name.ident != "self" || target.is_empty() {
                target.push(source_ident(&name.ident));
            }
            output.push(UseLeaf {
                local: target.last().cloned(),
                target,
                glob: false,
            });
            Ok(())
        }
        UseTree::Rename(rename) => {
            if rename.rename == "_" {
                return Ok(());
            }
            let mut target = prefix;
            if rename.ident != "self" || target.is_empty() {
                target.push(source_ident(&rename.ident));
            }
            output.push(UseLeaf {
                local: Some(source_ident(&rename.rename)),
                target,
                glob: false,
            });
            Ok(())
        }
        UseTree::Glob(_) => {
            output.push(UseLeaf {
                target: prefix,
                local: None,
                glob: true,
            });
            Ok(())
        }
        UseTree::Group(group) => {
            for item in &group.items {
                flatten_use(item, prefix.clone(), output)?;
            }
            Ok(())
        }
    }
}
