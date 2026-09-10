//! Finite explicit import and re-export resolution for schema entrypoints.
//!
//! This index deliberately does not emulate `macro_rules!`, `macro_use`,
//! `macro_export`, or `include!`. Rustc owns macro expansion and name
//! resolution. This module only identifies direct schema invocations through
//! ordinary item bindings.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use syn::{Item, UseTree};

use super::{
    graph::{has_shape_cfg, source_ident, SourceUnit},
    modules::relative,
    SourceDiagnostic,
};

#[derive(Clone)]
struct Symbol {
    path: Vec<String>,
    facade: bool,
}

struct ConditionalBinding {
    construct: &'static str,
    path: PathBuf,
    module: bool,
}

/// Finite explicit facade import and re-export resolution for source selectors.
pub struct ImportIndex {
    bindings: BTreeMap<Vec<String>, Symbol>,
    conditional_bindings: BTreeMap<Vec<String>, ConditionalBinding>,
    conditional_unit_imports: BTreeMap<Vec<String>, Vec<Symbol>>,
    local_definitions: BTreeSet<Vec<String>>,
    globs: BTreeMap<Vec<String>, Vec<Symbol>>,
    facades: BTreeSet<String>,
}

impl ImportIndex {
    pub(super) fn build(
        units: &[SourceUnit],
        facades: &BTreeSet<String>,
        package_root: &Path,
    ) -> Result<Self, SourceDiagnostic> {
        let mut output = Self {
            bindings: BTreeMap::new(),
            conditional_bindings: BTreeMap::new(),
            conditional_unit_imports: BTreeMap::new(),
            local_definitions: BTreeSet::new(),
            globs: BTreeMap::new(),
            facades: facades.clone(),
        };
        for unit in units.iter().filter(|unit| !unit.conditional) {
            for item in &unit.items {
                match item {
                    Item::Mod(item_mod) => {
                        let mut key = unit.module_path.clone();
                        key.push(source_ident(&item_mod.ident));
                        if has_shape_cfg(&item_mod.attrs)? {
                            output.conditional_bindings.entry(key).or_insert_with(|| {
                                ConditionalBinding {
                                    construct: "facade-shadowing module",
                                    path: relative(package_root, &unit.source_path),
                                    module: true,
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
                        if has_shape_cfg(&item_extern.attrs)? {
                            output.conditional_bindings.entry(key).or_insert_with(|| {
                                ConditionalBinding {
                                    construct: "facade-shadowing extern crate alias",
                                    path: relative(package_root, &unit.source_path),
                                    module: false,
                                }
                            });
                            continue;
                        }
                        let source = source_ident(&item_extern.ident);
                        let target = Symbol {
                            path: vec![source.clone()],
                            facade: output.facades.contains(&source),
                        };
                        if output.local_definitions.contains(&key)
                            || output.bindings.insert(key.clone(), target).is_some()
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
                flatten_use(&item_use.tree, Vec::new(), &mut leaves)?;
                if unit.conditional {
                    // Preserve enough explicit resolution for selectors to reject declarations
                    // inside conditional units, without admitting those units as authority.
                    for leaf in leaves {
                        let target = output.normalize(&unit.module_path, &leaf.target)?;
                        if leaf.glob {
                            output
                                .globs
                                .entry(unit.module_path.clone())
                                .or_default()
                                .push(target);
                        } else {
                            let mut key = unit.module_path.clone();
                            key.push(leaf.local.expect("non-glob leaf"));
                            output
                                .conditional_unit_imports
                                .entry(key)
                                .or_default()
                                .push(target);
                        }
                    }
                    continue;
                }
                if has_shape_cfg(&item_use.attrs)? {
                    for leaf in leaves.into_iter().filter(|leaf| !leaf.glob) {
                        let mut key = unit.module_path.clone();
                        key.push(leaf.local.expect("non-glob leaf"));
                        output.conditional_bindings.entry(key).or_insert_with(|| {
                            ConditionalBinding {
                                construct: "facade-shadowing import",
                                path: relative(package_root, &unit.source_path),
                                module: false,
                            }
                        });
                    }
                    continue;
                }
                for leaf in leaves {
                    let target = output.normalize(&unit.module_path, &leaf.target)?;
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
                    if output.local_definitions.contains(&key)
                        || output.bindings.insert(key.clone(), target).is_some()
                    {
                        return Err(SourceDiagnostic::AmbiguousImport {
                            name: key.join("::"),
                        });
                    }
                }
            }
        }
        Ok(output)
    }

    fn normalize(&self, module: &[String], raw: &[String]) -> Result<Symbol, SourceDiagnostic> {
        let Some(first) = raw.first() else {
            return Err(SourceDiagnostic::AmbiguousImport {
                name: "<empty>".to_owned(),
            });
        };
        let mut path = module.to_vec();
        let mut offset = 0;
        match first.as_str() {
            "crate" => {
                path.clear();
                offset = 1;
            }
            "self" => offset = 1,
            "super" => {
                while raw.get(offset).is_some_and(|part| part == "super") {
                    if path.pop().is_none() {
                        return Err(SourceDiagnostic::EscapingImport {
                            path: raw.join("::"),
                        });
                    }
                    offset += 1;
                }
            }
            _ => {
                let mut local = module.to_vec();
                local.push(first.clone());
                if let Some(conditional) = self.conditional_bindings.get(&local) {
                    return Err(SourceDiagnostic::ConditionalSchema {
                        construct: conditional.construct,
                        path: conditional.path.clone(),
                    });
                }
                if let Some(target) = self.bindings.get(&local) {
                    let mut target = target.clone();
                    target.path.extend(raw.iter().skip(1).cloned());
                    return Ok(target);
                }
                if self.local_definitions.contains(&local) {
                    path.extend(raw.iter().cloned());
                    return Ok(Symbol {
                        path,
                        facade: false,
                    });
                }
                if self.facades.contains(first) {
                    return Ok(Symbol {
                        path: raw.to_vec(),
                        facade: true,
                    });
                }
            }
        }
        path.extend(raw[offset..].iter().cloned());
        Ok(Symbol {
            path,
            facade: false,
        })
    }

    /// Whether a source path resolves through an explicit facade binding.
    pub fn resolves_entrypoint(
        &self,
        module: &[String],
        raw: &[String],
        entrypoint: &str,
    ) -> Result<bool, SourceDiagnostic> {
        let symbol = self.normalize(module, raw)?;
        self.resolve(symbol, entrypoint, &mut BTreeSet::new())
    }

    fn resolve(
        &self,
        symbol: Symbol,
        entrypoint: &str,
        visiting: &mut BTreeSet<Vec<String>>,
    ) -> Result<bool, SourceDiagnostic> {
        if symbol.facade {
            return Ok(symbol.path.last().is_some_and(|tail| tail == entrypoint));
        }
        let mut conditional_module = None;
        let mut resolved = None;
        for length in (1..=symbol.path.len()).rev() {
            let prefix = &symbol.path[..length];
            if let Some(conditional) = self.conditional_bindings.get(prefix) {
                if conditional.module {
                    conditional_module = Some(conditional);
                    continue;
                }
                return Err(SourceDiagnostic::ConditionalSchema {
                    construct: conditional.construct,
                    path: conditional.path.clone(),
                });
            }
            let Some(target) = self.bindings.get(prefix) else {
                continue;
            };
            // Track the binding, not its expanded path: a cyclic alias can grow
            // a suffix indefinitely without repeating the complete path.
            if !visiting.insert(prefix.to_vec()) {
                return Err(SourceDiagnostic::ImportCycle {
                    path: prefix.join("::"),
                });
            }
            let mut target = target.clone();
            target.path.extend_from_slice(&symbol.path[length..]);
            let result = self.resolve(target, entrypoint, visiting);
            visiting.remove(prefix);
            resolved = Some(result?);
            break;
        }
        let resolved = match resolved {
            Some(resolved) => resolved,
            None => self.resolve_conditional_import(&symbol.path, entrypoint, visiting)?,
        };
        if resolved {
            if let Some(conditional) = conditional_module {
                return Err(SourceDiagnostic::ConditionalSchema {
                    construct: conditional.construct,
                    path: conditional.path.clone(),
                });
            }
        }
        Ok(resolved)
    }

    fn resolve_conditional_import(
        &self,
        path: &[String],
        entrypoint: &str,
        visiting: &mut BTreeSet<Vec<String>>,
    ) -> Result<bool, SourceDiagnostic> {
        for length in (1..=path.len()).rev() {
            let Some(targets) = self.conditional_unit_imports.get(&path[..length]) else {
                continue;
            };
            let prefix = &path[..length];
            if !visiting.insert(prefix.to_vec()) {
                return Err(SourceDiagnostic::ImportCycle {
                    path: prefix.join("::"),
                });
            }
            for target in targets {
                let mut target = target.clone();
                target.path.extend_from_slice(&path[length..]);
                if self.resolve(target, entrypoint, visiting)? {
                    visiting.remove(prefix);
                    return Ok(true);
                }
            }
            visiting.remove(prefix);
        }
        Ok(false)
    }

    /// Whether an unsupported glob could supply an otherwise unresolved entrypoint.
    pub fn glob_can_supply(
        &self,
        module: &[String],
        entrypoint: &str,
    ) -> Result<bool, SourceDiagnostic> {
        for glob in self.globs.get(module).into_iter().flatten() {
            let mut symbol = glob.clone();
            symbol.path.push(entrypoint.to_owned());
            if self.resolve(symbol, entrypoint, &mut BTreeSet::new())? {
                return Ok(true);
            }
        }
        Ok(false)
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
