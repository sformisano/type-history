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

#[derive(Clone, PartialEq, Eq)]
struct Symbol {
    path: Vec<String>,
    facade: bool,
}

#[derive(Clone, Copy)]
enum Namespace<'a> {
    Module,
    Macro(Option<&'a str>),
}

fn is_facade_macro(name: &str) -> bool {
    matches!(name, "typehistory" | "Schema")
}

struct ConditionalBinding {
    construct: &'static str,
    path: PathBuf,
    module: bool,
}

/// Finite explicit facade import and re-export resolution for source selectors.
pub struct ImportIndex {
    bindings: BTreeMap<Vec<String>, Vec<Symbol>>,
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
        let mut pending_imports = Vec::new();
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
                flatten_use(&item_use.tree, Vec::new(), &mut leaves)?;
                if unit.conditional {
                    // Preserve enough explicit resolution for selectors to reject declarations
                    // inside conditional units, without admitting those units as authority.
                    for leaf in leaves {
                        let target = output.normalize(&unit.module_path, &leaf.target, None)?;
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
                    let target = output.normalize(&unit.module_path, &leaf.target, None)?;
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
                    let targets = output.bindings.entry(key.clone()).or_default();
                    pending_imports.push((
                        key,
                        targets.len(),
                        unit.module_path.clone(),
                        leaf.target,
                    ));
                    targets.push(target);
                }
            }
        }
        // A same-named macro can arrive through a reexport declared later.
        // Revisit ordinary imports with the complete index so their crate
        // prefixes are independent of declaration and module traversal order.
        for _ in 0..=pending_imports.len() {
            let mut changed = false;
            for (key, index, module, raw) in &pending_imports {
                let target = output.normalize(module, raw, None)?;
                let stored = &mut output.bindings.get_mut(key).expect("pending import")[*index];
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

    fn validate_namespaces(&self) -> Result<(), SourceDiagnostic> {
        for (key, targets) in &self.bindings {
            if targets.len() < 2 && !self.local_definitions.contains(key) {
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

    fn normalize(
        &self,
        module: &[String],
        raw: &[String],
        attribute: Option<&str>,
    ) -> Result<Symbol, SourceDiagnostic> {
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
                if let Some(targets) = self.bindings.get(&local) {
                    // Prefixes use the module namespace. A bare attribute uses
                    // the macro namespace, regardless of its imported spelling.
                    let namespace = if attribute.is_some() && raw.len() == 1 {
                        Namespace::Macro(attribute)
                    } else {
                        Namespace::Module
                    };
                    for target in targets {
                        if !self.resolve(target.clone(), namespace, &mut BTreeSet::new())? {
                            continue;
                        }
                        let mut target = target.clone();
                        target.path.extend(raw.iter().skip(1).cloned());
                        return Ok(target);
                    }
                }
                if self.local_definitions.contains(&local) {
                    path.extend(raw.iter().cloned());
                    return Ok(Symbol {
                        path,
                        facade: false,
                    });
                }
                if attribute.is_some()
                    && raw.len() == 1
                    && self.conditional_unit_imports.contains_key(&local)
                {
                    return Ok(Symbol {
                        path: local,
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
        let symbol = self.normalize(module, raw, Some(entrypoint))?;
        self.resolve(
            symbol,
            Namespace::Macro(Some(entrypoint)),
            &mut BTreeSet::new(),
        )
    }

    fn resolve(
        &self,
        symbol: Symbol,
        namespace: Namespace<'_>,
        visiting: &mut BTreeSet<Vec<String>>,
    ) -> Result<bool, SourceDiagnostic> {
        if symbol.facade {
            return Ok(match namespace {
                Namespace::Module => symbol.path.len() != 2 || !is_facade_macro(&symbol.path[1]),
                Namespace::Macro(name) => {
                    symbol.path.len() == 2
                        && name.map_or_else(
                            || is_facade_macro(&symbol.path[1]),
                            |name| symbol.path[1] == name,
                        )
                }
            });
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
            let Some(targets) = self.bindings.get(prefix) else {
                continue;
            };
            // Track the binding, not its expanded path: a cyclic alias can grow
            // a suffix indefinitely without repeating the complete path.
            if !visiting.insert(prefix.to_vec()) {
                return Err(SourceDiagnostic::ImportCycle {
                    path: prefix.join("::"),
                });
            }
            let mut result = false;
            for target in targets {
                let mut target = target.clone();
                target.path.extend_from_slice(&symbol.path[length..]);
                result |= self.resolve(target, namespace, visiting)?;
            }
            visiting.remove(prefix);
            resolved = Some(result);
            break;
        }
        let resolved = match resolved {
            Some(resolved) => resolved,
            None => self.resolve_conditional_import(&symbol.path, namespace, visiting)?,
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
        namespace: Namespace<'_>,
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
                if self.resolve(target, namespace, visiting)? {
                    visiting.remove(prefix);
                    return Ok(true);
                }
            }
            visiting.remove(prefix);
        }
        Ok(matches!(namespace, Namespace::Module))
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
            if self.resolve(
                symbol,
                Namespace::Macro(Some(entrypoint)),
                &mut BTreeSet::new(),
            )? {
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
