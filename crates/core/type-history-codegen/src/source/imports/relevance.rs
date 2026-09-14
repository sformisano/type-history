//! History relevance for import collisions and conditional selection.

use std::collections::BTreeSet;
use std::path::PathBuf;

use super::{is_facade_macro, ImportIndex, Symbol};
use crate::source::SourceDiagnostic;

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum HistoryRelevance {
    Unrelated,
    Explicit,
    Glob,
    Conditional {
        construct: &'static str,
        path: PathBuf,
    },
}

impl HistoryRelevance {
    pub(super) fn is_relevant(&self) -> bool {
        *self != Self::Unrelated
    }
}

impl ImportIndex {
    /// Whether retained import targets reach history authority, including globs.
    ///
    /// This is deliberately independent of Rust's ordinary item namespaces.
    /// Conditional branches contribute every target; none is selected here.
    pub(super) fn history_relevance(
        &self,
        symbol: &Symbol,
        visiting: &mut BTreeSet<Vec<String>>,
    ) -> Result<HistoryRelevance, SourceDiagnostic> {
        let relevance = self.classify_history(symbol, visiting)?;
        if relevance.is_relevant() && !symbol.external {
            for length in (1..=symbol.path.len()).rev() {
                if let Some(binding) = self.conditional_bindings.get(&symbol.path[..length]) {
                    return Ok(HistoryRelevance::Conditional {
                        construct: binding.construct,
                        path: binding.path.clone(),
                    });
                }
            }
        }
        Ok(relevance)
    }

    fn classify_history(
        &self,
        symbol: &Symbol,
        visiting: &mut BTreeSet<Vec<String>>,
    ) -> Result<HistoryRelevance, SourceDiagnostic> {
        if symbol.external {
            return Ok(if self.facade_authority(&symbol.path) {
                HistoryRelevance::Explicit
            } else {
                HistoryRelevance::Unrelated
            });
        }
        // Removing a conditional shadow can expose the extern prelude even
        // when that shadow's own target has no history reexports.
        for length in 1..=symbol.path.len() {
            let prefix = &symbol.path[..length];
            if self.conditional_bindings.contains_key(prefix)
                && self.facade_authority(&symbol.path[length - 1..])
            {
                return Ok(HistoryRelevance::Explicit);
            }
        }
        for length in (1..=symbol.path.len()).rev() {
            let prefix = &symbol.path[..length];
            let targets = self
                .bindings
                .get(prefix)
                .into_iter()
                .flatten()
                .chain(
                    self.conditional_bindings
                        .get(prefix)
                        .into_iter()
                        .flat_map(|binding| &binding.targets),
                )
                .chain(
                    self.conditional_unit_imports
                        .get(prefix)
                        .into_iter()
                        .flatten(),
                );
            let mut has_targets = false;
            let mut relevance = HistoryRelevance::Unrelated;
            // Key cycles can grow path suffixes indefinitely. Track the
            // binding identity, as the entrypoint resolver does.
            if !visiting.insert(prefix.to_vec()) {
                return Err(SourceDiagnostic::ImportCycle {
                    path: prefix.join("::"),
                });
            }
            for target in targets {
                has_targets = true;
                let mut target = target.clone();
                target.path.extend_from_slice(&symbol.path[length..]);
                relevance = relevance.max(self.history_relevance(&target, visiting)?);
            }
            visiting.remove(prefix);
            if has_targets {
                return Ok(relevance);
            }
        }
        // A module's glob can supply a qualified member or a bare imported
        // module. Keep that provenance so selection can reject the glob.
        // Once a prefix names an indexed module, its members cannot be
        // supplied by a glob in that module's parent scope.
        let module_boundary = (1..=symbol.path.len())
            .rev()
            .find(|&length| {
                let prefix = &symbol.path[..length];
                self.local_definitions.contains(prefix)
                    || self.conditional_bindings.contains_key(prefix)
            })
            .unwrap_or(0);
        for length in (module_boundary..=symbol.path.len()).rev() {
            let prefix = &symbol.path[..length];
            let Some(targets) = self.globs.get(prefix) else {
                continue;
            };
            if !visiting.insert(prefix.to_vec()) {
                return Err(SourceDiagnostic::ImportCycle {
                    path: prefix.join("::"),
                });
            }
            let mut relevance = HistoryRelevance::Unrelated;
            for target in targets {
                // Unknown external namespaces have no indexed authority.
                // Do not feed their lexical path back through this same glob.
                if !target.external && !self.has_local_import_target(&target.path) {
                    continue;
                }
                let mut target = target.clone();
                target.path.extend_from_slice(&symbol.path[length..]);
                let target_relevance = match self.history_relevance(&target, visiting)? {
                    HistoryRelevance::Unrelated => HistoryRelevance::Unrelated,
                    conditional @ HistoryRelevance::Conditional { .. } => conditional,
                    _ => HistoryRelevance::Glob,
                };
                relevance = relevance.max(target_relevance);
            }
            visiting.remove(prefix);
            if relevance.is_relevant() {
                return Ok(relevance);
            }
        }
        // A local module can expose history through explicit reexports even
        // though importing the module itself has no direct binding target.
        for key in self
            .bindings
            .keys()
            .chain(self.conditional_bindings.keys())
            .chain(self.conditional_unit_imports.keys())
            .filter(|key| key.len() > symbol.path.len() && key.starts_with(&symbol.path))
        {
            let member = Symbol {
                path: key.clone(),
                external: false,
            };
            let relevance = self.history_relevance(&member, visiting)?;
            if relevance.is_relevant() {
                return Ok(relevance);
            }
        }
        Ok(HistoryRelevance::Unrelated)
    }

    fn facade_authority(&self, path: &[String]) -> bool {
        path.first().is_some_and(|name| self.facades.contains(name))
            && (path.len() == 1 || (path.len() == 2 && is_facade_macro(&path[1])))
    }

    fn has_local_import_target(&self, path: &[String]) -> bool {
        self.local_definitions.contains(path)
            || self.conditional_bindings.contains_key(path)
            || self.globs.contains_key(path)
            || (1..=path.len()).any(|length| {
                self.bindings.contains_key(&path[..length])
                    || self.conditional_unit_imports.contains_key(&path[..length])
            })
    }
}
