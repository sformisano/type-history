//! Finite explicit import and re-export resolution for schema entrypoints.
//!
//! This index deliberately does not emulate `macro_rules!`, `macro_use`,
//! `macro_export`, or `include!`. Rustc owns macro expansion and name
//! resolution. This module only identifies direct schema invocations through
//! ordinary item bindings.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};

use syn::Path as SynPath;

use super::{graph::path_segments, SourceDiagnostic};

mod collection;
mod relevance;
use relevance::HistoryRelevance;

#[derive(Clone, PartialEq, Eq)]
struct Symbol {
    path: Vec<String>,
    external: bool,
}

#[derive(Clone, Copy)]
enum Namespace<'a> {
    Module,
    Value,
    Macro(Option<&'a str>),
}

fn is_facade_macro(name: &str) -> bool {
    matches!(name, "versioned" | "Schema")
}

// Direct types, traits, and modules exported by the history facade. Values
// such as `decode` and arbitrary external items have no proven type namespace.
fn is_facade_type_namespace(name: &str) -> bool {
    matches!(
        name,
        "DecodeContext"
            | "DecodeError"
            | "DecodeFailureKind"
            | "FieldSchema"
            | "HasHistory"
            | "History"
            | "InvalidPayloadVersion"
            | "InvalidStableName"
            | "JsonSchemaField"
            | "PayloadVersion"
            | "ReadError"
            | "ResolvedSchema"
            | "StableName"
            | "Versioned"
            | "__private"
    )
}

struct ConditionalBinding {
    construct: &'static str,
    path: PathBuf,
    targets: Vec<Symbol>,
}

/// Finite explicit facade import and re-export resolution for source selectors.
pub struct ImportIndex {
    bindings: BTreeMap<Vec<String>, Vec<Symbol>>,
    conditional_bindings: BTreeMap<Vec<String>, ConditionalBinding>,
    conditional_unit_imports: BTreeMap<Vec<String>, Vec<Symbol>>,
    local_definitions: BTreeSet<Vec<String>>,
    local_values: BTreeSet<Vec<String>>,
    globs: BTreeMap<Vec<String>, Vec<Symbol>>,
    facades: BTreeSet<String>,
}

impl ImportIndex {
    fn normalize(
        &self,
        module: &[String],
        raw: &[String],
        external: bool,
        attribute: Option<&str>,
    ) -> Result<Symbol, SourceDiagnostic> {
        let Some(first) = raw.first() else {
            return Err(SourceDiagnostic::AmbiguousImport {
                name: "<empty>".to_owned(),
            });
        };
        // In Rust 2018+, a leading `::` selects the extern prelude even
        // when a local module or import has the same name.
        if external {
            return Ok(Symbol {
                path: raw.to_vec(),
                external: true,
            });
        }
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
                if self.conditional_bindings.contains_key(&local) {
                    // Index ordinary imports without choosing a cfg branch.
                    // Entrypoint selection checks the retained binding later.
                    let mut path = local;
                    path.extend_from_slice(&raw[1..]);
                    return Ok(Symbol {
                        path,
                        external: false,
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
                        // Module resolution admits only known module/type
                        // bindings. An unrelated external macro is unknown,
                        // so it cannot win because it was imported first.
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
                        external: false,
                    });
                }
                if attribute.is_none() || raw.len() > 1 {
                    // Unknown external items may be modules. Preserve their
                    // shadow only after proven imported/local modules have
                    // had priority. Known macros and values cannot hide the
                    // facade's module namespace.
                    for target in self.bindings.get(&local).into_iter().flatten() {
                        if self.resolve(
                            target.clone(),
                            Namespace::Macro(None),
                            &mut BTreeSet::new(),
                        )? || self.resolve(
                            target.clone(),
                            Namespace::Value,
                            &mut BTreeSet::new(),
                        )? {
                            continue;
                        }
                        let mut target = target.clone();
                        target.path.extend(raw.iter().skip(1).cloned());
                        return Ok(target);
                    }
                }
                if attribute.is_some()
                    && raw.len() == 1
                    && self.conditional_unit_imports.contains_key(&local)
                {
                    return Ok(Symbol {
                        path: local,
                        external: false,
                    });
                }
                if self.facades.contains(first) {
                    return Ok(Symbol {
                        path: raw.to_vec(),
                        external: true,
                    });
                }
            }
        }
        path.extend(raw[offset..].iter().cloned());
        Ok(Symbol {
            path,
            external: false,
        })
    }

    /// Whether a source path resolves through an explicit facade binding.
    pub fn resolves_entrypoint(
        &self,
        module: &[String],
        path: &SynPath,
        entrypoint: &str,
    ) -> Result<bool, SourceDiagnostic> {
        let symbol = self.normalize(
            module,
            &path_segments(path),
            path.leading_colon.is_some(),
            Some(entrypoint),
        )?;
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
        if symbol.external {
            let facade = self.facades.contains(&symbol.path[0]);
            return Ok(match namespace {
                Namespace::Module => {
                    symbol.path.len() == 1
                        || (facade
                            && symbol.path.len() == 2
                            && is_facade_type_namespace(&symbol.path[1]))
                }
                Namespace::Macro(name) => {
                    facade
                        && symbol.path.len() == 2
                        && name.map_or_else(
                            || is_facade_macro(&symbol.path[1]),
                            |name| symbol.path[1] == name,
                        )
                }
                Namespace::Value => facade && symbol.path.len() == 2 && symbol.path[1] == "decode",
            });
        }
        let relevance = if matches!(namespace, Namespace::Macro(Some(_))) {
            self.history_relevance(&symbol, &mut BTreeSet::new())?
        } else {
            HistoryRelevance::Unrelated
        };
        if let HistoryRelevance::Conditional { construct, path } = relevance {
            return Err(SourceDiagnostic::ConditionalSchema { construct, path });
        }
        let mut resolved = None;
        for length in (1..=symbol.path.len()).rev() {
            let prefix = &symbol.path[..length];
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
        if let Namespace::Macro(Some(entrypoint)) = namespace {
            if !resolved && relevance == HistoryRelevance::Glob {
                return Err(SourceDiagnostic::GlobEntrypoint {
                    name: entrypoint.to_owned(),
                    rewrite: format!("use <type-history-dependency>::{entrypoint};"),
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
        Ok(match namespace {
            Namespace::Module => {
                self.local_definitions.contains(path)
                    || self.conditional_bindings.contains_key(path)
            }
            Namespace::Value => self.local_values.contains(path),
            Namespace::Macro(_) => false,
        })
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
