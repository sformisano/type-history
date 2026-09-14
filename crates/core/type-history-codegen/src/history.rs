//! Field history parsing and authorized retained-version reconstruction.
//!
//! Inference allocates only by source size. Committed authority must approve
//! its range before dense expansion. The ledger validates the inferred head;
//! it never supplies a missing source boundary.

use serde::{de::DeserializeOwned, Serialize};
use std::collections::{BTreeMap, BTreeSet};

use crate::ledger::HistoryLedger;
use type_history_core::resolved::SchemaShape;

use syn::{Attribute, Error, Ident, Result, Type};

use crate::model::NamedField;

mod attributes;
pub use attributes::{parse_field_history, Backfill, FieldHistory, FieldHistoryRecord};

/// One field as it exists at one retained version.
#[derive(Clone)]
pub struct RetainedField {
    /// Durable field name; identical across every version it exists in.
    pub name: Ident,
    /// Resolved type at this version.
    pub ty: Type,
    /// Documentation and lint attributes replayed on the generated field.
    pub attributes: Vec<Attribute>,
}

/// How one destination field of an adjacent transition is produced.
#[derive(Clone)]
pub enum FieldTransition {
    /// Move the predecessor field unchanged.
    Carry {
        /// Field name in both payloads.
        name: Ident,
    },
    /// Initialize a field born at the destination version.
    Birth {
        /// Field name in the destination payload.
        name: Ident,
        /// Destination field type at its birth version.
        ty: Type,
        /// Authored initialization rule.
        assignment: Backfill,
    },
    /// Convert a field that changes at the destination version.
    Convert {
        /// Field name in both payloads.
        name: Ident,
        /// Destination field type.
        ty: Type,
        /// Authored conversion rule.
        assignment: Backfill,
    },
}

impl FieldTransition {
    /// Field name this transition assigns.
    pub fn name(&self) -> &Ident {
        match self {
            Self::Carry { name } | Self::Birth { name, .. } | Self::Convert { name, .. } => name,
        }
    }

    /// Whether this transition borrows the whole predecessor payload.
    pub fn borrows_previous_payload(&self) -> bool {
        matches!(
            self,
            Self::Birth {
                assignment: Backfill::Function(_),
                ..
            } | Self::Convert {
                assignment: Backfill::Function(_),
                ..
            }
        )
    }
}

/// One adjacent conversion from a retained version to its successor.
#[derive(Clone)]
pub struct AdjacentTransition {
    /// Source version.
    pub from: u32,
    /// Destination version, always `from + 1`.
    pub to: u32,
    /// Destination fields in payload declaration order.
    pub fields: Vec<FieldTransition>,
}

/// One retained payload version and its exact fields.
#[derive(Clone)]
pub struct RetainedVersion {
    /// Positive version number.
    pub version: u32,
    /// Fields in payload declaration order.
    pub fields: Vec<RetainedField>,
}

/// Complete reconstructed history for one record.
#[derive(Clone)]
pub struct ReconstructedHistory {
    /// Inferred current version: V1 or the highest field boundary.
    pub head: u32,
    /// Every retained version from V1 through `head`, in ascending order.
    pub versions: Vec<RetainedVersion>,
    /// Adjacent conversions, one per retained step.
    pub transitions: Vec<AdjacentTransition>,
}

impl ReconstructedHistory {
    /// Whether this history retains a predecessor of its current version.
    pub fn has_retained_predecessors(&self) -> bool {
        self.head > 1
    }

    /// Every retained version number in ascending order.
    pub fn retained_versions(&self) -> Vec<u32> {
        self.versions
            .iter()
            .map(|version| version.version)
            .collect()
    }

    /// Reconstruct every retained version from the authored payload inventory.
    ///
    /// `event_marker` carries the span used for record-level diagnostics.
    ///
    /// Only bounded unit fixtures use this unchecked expansion. Production
    /// callers must use `HistoryPlan::expand_authorized`.
    #[cfg(test)]
    pub(crate) fn reconstruct(event_marker: &Ident, fields: &[NamedField]) -> Result<Self> {
        let plan = HistoryPlan::infer(event_marker, fields)?;
        plan.expand_unchecked(event_marker, fields)
    }
}

/// History reconstructed only after its committed authority approves the range.
#[derive(Clone)]
pub struct AuthorizedHistory {
    history: ReconstructedHistory,
    frozen_shapes: BTreeMap<u32, SchemaShape>,
}

impl AuthorizedHistory {
    /// Inferred current version.
    pub fn head(&self) -> u32 {
        self.history.head
    }
    /// Every retained record, in ascending order.
    pub fn versions(&self) -> &[RetainedVersion] {
        &self.history.versions
    }
    /// Adjacent typed transitions, in ascending order.
    pub fn transitions(&self) -> &[AdjacentTransition] {
        &self.history.transitions
    }
    /// Frozen shapes requiring compiler assertions; reset reservations are excluded.
    pub fn frozen_shapes(&self) -> &BTreeMap<u32, SchemaShape> {
        &self.frozen_shapes
    }
    /// Whether the record retains a predecessor.
    pub fn has_retained_predecessors(&self) -> bool {
        self.history.has_retained_predecessors()
    }
    /// Retained positive version numbers in ascending order.
    pub fn retained_versions(&self) -> Vec<u32> {
        self.history.retained_versions()
    }
}

/// A parsed and validated field history whose head is known but whose dense
/// retained range has not been allocated.
///
/// Separating inference from expansion is what makes the approved order
/// possible: parse boundaries, infer the head, let committed authority approve
/// the head, and only then allocate one retained version per step.
#[derive(Clone, Copy, Debug)]
pub struct HistoryPlan {
    /// Inferred current version: V1 or the highest field boundary.
    pub head: u32,
}

impl HistoryPlan {
    /// Parse every field's history, validate each lifetime, and infer the head.
    ///
    /// Allocation here is bounded by the number of declared fields and their
    /// history records, never by a version number.
    pub fn infer(event_marker: &Ident, fields: &[NamedField]) -> Result<Self> {
        let mut head = 1;
        let mut names = BTreeSet::new();
        for field in fields {
            let authored = field.name.to_string();
            let name = authored.strip_prefix("r#").unwrap_or(&authored);
            if !names.insert(name.to_owned()) {
                return Err(Error::new_spanned(
                    &field.name,
                    format!(
                        "duplicate retained field `{name}`; a removed field name cannot be reused"
                    ),
                ));
            }
            let lifetime = FieldLifetime::resolve(event_marker, field)?;
            if let Some(boundary) = lifetime.history.highest_boundary() {
                head = head.max(boundary);
            }
        }
        Ok(Self { head })
    }

    /// Authorize the inferred range before allocating retained versions.
    pub fn expand_authorized<M: Clone + Eq + Serialize + DeserializeOwned>(
        &self,
        event_marker: &Ident,
        fields: &[NamedField],
        stable_name: &str,
        baseline: &HistoryLedger<M>,
        metadata: &M,
    ) -> Result<AuthorizedHistory> {
        let inferred = Self::infer(event_marker, fields)?;
        if inferred.head != self.head {
            return Err(Error::new_spanned(
                event_marker,
                "history inputs changed after inference",
            ));
        }
        baseline
            .authorize_history(stable_name, metadata, *self)
            .map_err(|reason| Error::new_spanned(event_marker, reason))?;
        let history = self.expand_unchecked(event_marker, fields)?;
        for version in &history.versions {
            let Some(snapshot) = baseline.snapshot(stable_name, version.version) else {
                continue;
            };
            if snapshot.reset_draft {
                continue;
            }
            let SchemaShape::Record { fields: frozen } = snapshot.schema.shape() else {
                return Err(Error::new_spanned(
                    event_marker,
                    format!(
                        "{stable_name} V{}: record authority must describe a record",
                        version.version
                    ),
                ));
            };
            let expected: BTreeSet<_> = frozen.iter().map(|field| field.name.as_str()).collect();
            let actual: BTreeSet<_> = version
                .fields
                .iter()
                .map(|field| {
                    let name = field.name.to_string();
                    name.strip_prefix("r#").unwrap_or(&name).to_owned()
                })
                .collect();
            for field in &version.fields {
                let name = field.name.to_string();
                let name = name.strip_prefix("r#").unwrap_or(&name);
                if !expected.contains(name) {
                    return Err(Error::new_spanned(&field.name, format!("{stable_name} V{} field `{name}` did not exist in frozen history; use added_in at its actual successor boundary instead of inventing an earlier field", version.version)));
                }
            }
            if let Some(name) = expected.iter().find(|name| !actual.contains(**name)) {
                return Err(Error::new_spanned(event_marker, format!("{stable_name} V{} field `{name}` disappeared from frozen history; restore its lifetime and use removed_in at a successor boundary", version.version)));
            }
        }
        let frozen_shapes = history
            .versions
            .iter()
            .filter_map(|version| {
                let snapshot = baseline.snapshot(stable_name, version.version)?;
                (!snapshot.reset_draft).then(|| (version.version, snapshot.schema.shape()))
            })
            .collect();
        Ok(AuthorizedHistory {
            history,
            frozen_shapes,
        })
    }

    /// Expand the dense retained range.
    ///
    /// Private because the authority check belongs to the caller that knows
    /// the history stable name; see this module's documentation.
    fn expand_unchecked(
        &self,
        event_marker: &Ident,
        fields: &[NamedField],
    ) -> Result<ReconstructedHistory> {
        let head = self.head;
        let mut lifetimes = Vec::with_capacity(fields.len());
        for field in fields {
            lifetimes.push(FieldLifetime::resolve(event_marker, field)?);
        }

        let mut versions = Vec::new();
        let mut version = 1;
        loop {
            let mut present = Vec::new();
            for lifetime in &lifetimes {
                if let Some(ty) = lifetime.type_at(version) {
                    present.push(RetainedField {
                        name: lifetime.field.name.clone(),
                        ty,
                        attributes: lifetime.retained_attributes.clone(),
                    });
                }
            }
            versions.push(RetainedVersion {
                version,
                fields: present,
            });
            if version == head {
                break;
            }
            version = version.checked_add(1).ok_or_else(|| {
                Error::new_spanned(
                    event_marker,
                    "retained version range exceeds the supported positive range",
                )
            })?;
        }

        let mut transitions = Vec::new();
        for window in versions.windows(2) {
            let (source, destination) = (&window[0], &window[1]);
            let mut assigned = Vec::new();
            for lifetime in &lifetimes {
                let Some(destination_type) = lifetime.type_at(destination.version) else {
                    continue;
                };
                let name = lifetime.field.name.clone();
                if lifetime.type_at(source.version).is_none() {
                    let assignment =
                        lifetime.birth_assignment(event_marker, destination.version)?;
                    assigned.push(FieldTransition::Birth {
                        name,
                        ty: destination_type,
                        assignment,
                    });
                } else if let Some(assignment) = lifetime.update_assignment(destination.version) {
                    assigned.push(FieldTransition::Convert {
                        name,
                        ty: destination_type,
                        assignment,
                    });
                } else {
                    assigned.push(FieldTransition::Carry { name });
                }
            }
            transitions.push(AdjacentTransition {
                from: source.version,
                to: destination.version,
                fields: assigned,
            });
        }

        Ok(ReconstructedHistory {
            head,
            versions,
            transitions,
        })
    }
}

/// One field's validated lifetime across the retained range.
struct FieldLifetime<'a> {
    field: &'a NamedField,
    history: FieldHistory,
    retained_attributes: Vec<Attribute>,
    birth: u32,
    removal: Option<u32>,
    /// Ascending update boundaries paired with their predecessor types.
    updates: Vec<(u32, Type)>,
}

impl<'a> FieldLifetime<'a> {
    fn resolve(event_marker: &Ident, field: &'a NamedField) -> Result<Self> {
        let history = parse_field_history(&field.attributes)?;
        let retained_attributes = field
            .attributes
            .iter()
            .filter(|attribute| !attribute.path().is_ident("history"))
            .cloned()
            .collect();

        // Records are authored newest first. Reject a reversed or repeated
        // boundary before interpreting any of them.
        let mut previous: Option<u32> = None;
        for record in &history.records {
            let version = record.version();
            if version <= 1 {
                return Err(Error::new_spanned(
                    record.marker(),
                    format!(
                        "{}.{} boundary V{version} is not above V1",
                        event_marker, field.name,
                    ),
                ));
            }
            if let Some(previous) = previous {
                if version >= previous {
                    return Err(Error::new_spanned(
                        record.marker(),
                        format!(
                            "{}.{} history records are declared newest first with distinct versions; V{version} follows V{previous}",
                            event_marker, field.name,
                        ),
                    ));
                }
            }
            previous = Some(version);
        }

        let mut birth = 1;
        let mut explicit_birth = None;
        let mut removal = None;
        let mut updates = Vec::new();
        // Walk oldest first so chronology reads forwards.
        for record in history.records.iter().rev() {
            match record {
                FieldHistoryRecord::Added {
                    version, marker, ..
                } => {
                    if explicit_birth.is_some() {
                        return Err(Error::new_spanned(
                            marker,
                            format!(
                                "{}.{} declares more than one birth",
                                event_marker, field.name
                            ),
                        ));
                    }
                    if !updates.is_empty() || removal.is_some() {
                        return Err(Error::new_spanned(
                            marker,
                            format!(
                                "{}.{} declares a birth after a later record; birth precedes every update and removal",
                                event_marker, field.name,
                            ),
                        ));
                    }
                    birth = *version;
                    explicit_birth = Some(*version);
                }
                FieldHistoryRecord::Updated {
                    version,
                    previous_type,
                    marker,
                    ..
                } => {
                    if *version <= birth {
                        return Err(Error::new_spanned(
                            marker,
                            format!(
                                "{}.{} updates at V{version}, which is not after its birth V{birth}",
                                event_marker, field.name,
                            ),
                        ));
                    }
                    if let Some(removed) = removal {
                        return Err(Error::new_spanned(
                            marker,
                            format!(
                                "{}.{} updates at V{version} after its removal at V{removed}",
                                event_marker, field.name,
                            ),
                        ));
                    }
                    updates.push((*version, (**previous_type).clone()));
                }
                FieldHistoryRecord::Removed { version, marker } => {
                    if removal.is_some() {
                        return Err(Error::new_spanned(
                            marker,
                            format!(
                                "{}.{} declares more than one removal",
                                event_marker, field.name
                            ),
                        ));
                    }
                    if *version <= birth {
                        return Err(Error::new_spanned(
                            marker,
                            format!(
                                "{}.{} is removed at V{version}, which is not after its birth V{birth}",
                                event_marker, field.name,
                            ),
                        ));
                    }
                    removal = Some(*version);
                }
            }
        }

        Ok(Self {
            field,
            history,
            retained_attributes,
            birth,
            removal,
            updates,
        })
    }

    /// Resolved field type at `version`, or `None` when the field is absent.
    fn type_at(&self, version: u32) -> Option<Type> {
        if version < self.birth {
            return None;
        }
        if self.removal.is_some_and(|removed| version >= removed) {
            return None;
        }
        // The declared type applies from the newest update onward. Below that
        // boundary the type is the `previous_type` of the next update above `version`.
        for (boundary, previous) in &self.updates {
            if version < *boundary {
                return Some(previous.clone());
            }
        }
        Some(self.field.ty.clone())
    }

    fn birth_assignment(&self, event_marker: &Ident, version: u32) -> Result<Backfill> {
        for record in &self.history.records {
            if let FieldHistoryRecord::Added {
                version: boundary,
                assignment,
                ..
            } = record
            {
                if *boundary == version {
                    return Ok(assignment.clone());
                }
            }
        }
        Err(Error::new_spanned(
            &self.field.name,
            format!(
                "{}.{} appears at V{version} without an explicit historical initializer",
                event_marker, self.field.name,
            ),
        ))
    }

    fn update_assignment(&self, version: u32) -> Option<Backfill> {
        self.history.records.iter().find_map(|record| match record {
            FieldHistoryRecord::Updated {
                version: boundary,
                assignment,
                ..
            } if *boundary == version => Some(assignment.clone()),
            _ => None,
        })
    }
}

#[cfg(test)]
mod tests;
