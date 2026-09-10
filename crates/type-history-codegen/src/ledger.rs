//! Strict committed history authority shared by compilation and lifecycle tools.
use crate::{history::HistoryPlan, json_schema::JsonSchemaDocument};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fmt::{Display, Formatter, Result as FmtResult},
    fs,
    path::{Path, PathBuf},
};
use type_history_core::TypeIdentity;

mod unique_json;

/// Exact schema-ID policy selected by one frontend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaIdentity {
    prefix: String,
}
impl SchemaIdentity {
    /// Select the frontend's nonempty schema-ID prefix.
    pub fn new(prefix: impl Into<String>) -> Self {
        let prefix = prefix.into();
        assert!(!prefix.is_empty(), "a schema identity prefix is required");
        Self { prefix }
    }
    /// Prefix used by all schemas in this authority.
    pub fn prefix(&self) -> &str {
        &self.prefix
    }
    /// Complete expected schema ID for the logical type.
    pub fn schema_id(&self, identity: &str) -> String {
        format!("{}{identity}", self.prefix)
    }
}

/// Standalone histories carry no aggregate metadata.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordMetadata {}

/// One frozen record shape and frontend-owned metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot<M> {
    /// Equality-protected metadata validated by its owning frontend.
    pub metadata: M,
    /// Normalized compiler-resolved record shape.
    pub schema: JsonSchemaDocument,
    /// Reservation retaining the original latest frozen shape.
    #[serde(default, skip_serializing_if = "is_false")]
    pub reset_draft: bool,
}
fn is_false(value: &bool) -> bool {
    !value
}

/// Readiness of an authored head under committed authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HistoryReadiness {
    /// All declared versions have frozen authority.
    Frozen,
    /// One permitted latest draft has no frozen shape.
    Draft,
    /// The latest shape is reserved for explicit reset.
    ResetDraft,
}

/// Failure to read a valid committed authority.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LedgerError {
    /// The file cannot be read.
    #[error("cannot read committed history authority at {path}: {reason}")]
    Unreadable {
        /// Exact attempted path.
        path: PathBuf,
        /// Operating-system failure.
        reason: String,
    },
    /// The bytes violate the strict authority contract.
    #[error("invalid committed history authority at {path}: {reason}")]
    Invalid {
        /// Exact attempted path.
        path: PathBuf,
        /// Structural diagnostic.
        reason: String,
    },
}

/// Validated complete history inventory for one package and schema-ID policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryLedger<M> {
    entries: BTreeMap<String, BTreeMap<u32, Snapshot<M>>>,
    schema_identity: SchemaIdentity,
}

impl<M: Clone + Eq + Serialize + DeserializeOwned> HistoryLedger<M> {
    /// Empty authority for explicit initialization or import admission.
    pub fn empty(schema_identity: SchemaIdentity) -> Self {
        Self {
            entries: BTreeMap::new(),
            schema_identity,
        }
    }
    /// Read an existing ledger at the exact frontend-selected path.
    pub fn read_file(path: &Path, schema_identity: SchemaIdentity) -> Result<Self, LedgerError> {
        let bytes = fs::read(path).map_err(|error| LedgerError::Unreadable {
            path: path.to_path_buf(),
            reason: error.to_string(),
        })?;
        Self::parse(&bytes, schema_identity).map_err(|reason| LedgerError::Invalid {
            path: path.to_path_buf(),
            reason,
        })
    }
    /// Parse strict bytes before exposing any inventory or permitting expansion.
    pub fn parse(bytes: &[u8], schema_identity: SchemaIdentity) -> Result<Self, String> {
        unique_json::check(bytes).map_err(|error| error.to_string())?;
        let raw: BTreeMap<String, BTreeMap<String, Snapshot<M>>> =
            serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
        let mut entries = BTreeMap::new();
        for (identity, raw_versions) in raw {
            TypeIdentity::try_from(identity.clone())
                .map_err(|error| format!("invalid type identity `{identity}`: {error}"))?;
            if raw_versions.is_empty() {
                return Err(format!("{identity} records no versions"));
            }
            let mut versions = BTreeMap::new();
            for (key, snapshot) in raw_versions {
                let version = key.parse::<u32>().map_err(|error| error.to_string())?;
                if version == 0 || version.to_string() != key {
                    return Err(format!(
                        "version key `{key}` must be a canonical positive integer"
                    ));
                }
                if snapshot.schema.identity() != schema_identity.schema_id(&identity) {
                    return Err(format!(
                        "{identity} V{version}: schema $id disagrees with its type identity"
                    ));
                }
                versions.insert(version, snapshot);
            }
            let (head, latest) = versions
                .last_key_value()
                .expect("non-empty parsed version map");
            let metadata = &latest.metadata;
            let floor = *versions
                .first_key_value()
                .expect("non-empty parsed version map")
                .0;
            for (offset, (version, snapshot)) in versions.iter().enumerate() {
                if &snapshot.metadata != metadata {
                    return Err(format!(
                        "{identity} disagrees about its metadata across retained versions"
                    ));
                }
                if snapshot.reset_draft && version != head {
                    return Err(format!("{identity} V{version}: reset_draft is permitted only on the latest retained version V{head}"));
                }
                let expected = floor
                    .checked_add(u32::try_from(offset).map_err(|_| {
                        format!("{identity} records more versions than the supported range")
                    })?)
                    .ok_or_else(|| {
                        format!("{identity} records a version above the supported range")
                    })?;
                if *version != expected {
                    return Err(format!("{identity} retains a non-contiguous range; expected V{expected}, found V{version}"));
                }
            }
            entries.insert(identity, versions);
        }
        Ok(Self {
            entries,
            schema_identity,
        })
    }
    /// Every retained logical identity in deterministic order.
    pub fn identities(&self) -> impl Iterator<Item = &str> {
        self.entries.keys().map(String::as_str)
    }
    /// Immutable complete authority inventory.
    pub fn entries(&self) -> &BTreeMap<String, BTreeMap<u32, Snapshot<M>>> {
        &self.entries
    }
    /// Exact schema-ID policy used for candidate validation.
    pub fn schema_identity(&self) -> &SchemaIdentity {
        &self.schema_identity
    }
    /// Validate a lifecycle candidate through the same strict reader.
    pub fn from_entries(
        entries: BTreeMap<String, BTreeMap<u32, Snapshot<M>>>,
        schema_identity: SchemaIdentity,
    ) -> Result<Self, String> {
        Self::parse(
            &serde_json::to_vec(&entries).map_err(|error| error.to_string())?,
            schema_identity,
        )
    }
    /// One frozen shape or latest reset reservation.
    pub fn snapshot(&self, identity: &str, version: u32) -> Option<&Snapshot<M>> {
        self.entries.get(identity)?.get(&version)
    }
    /// Authorize the source range and metadata before dense reconstruction.
    pub fn authorize_history(
        &self,
        identity: &str,
        metadata: &M,
        plan: HistoryPlan,
    ) -> Result<HistoryReadiness, String> {
        TypeIdentity::try_from(identity.to_owned()).map_err(|error| error.to_string())?;
        self.authority(identity)
            .permits(plan.floor, plan.head)
            .map_err(|reason| format!("{identity}: {reason}"))?;
        let Some(versions) = self.entries.get(identity) else {
            return Ok(HistoryReadiness::Draft);
        };
        if versions
            .values()
            .any(|snapshot| &snapshot.metadata != metadata)
        {
            return Err(format!("{identity}: source metadata differs from frozen history; restore its original metadata"));
        }
        let (head, latest) = versions
            .last_key_value()
            .expect("validated non-empty inventory");
        if latest.reset_draft {
            if plan.head != *head {
                return Err(format!("{identity} V{head}: a reset draft cannot have a successor; freeze or undo this exact reset first"));
            }
            Ok(HistoryReadiness::ResetDraft)
        } else if plan.head == *head {
            Ok(HistoryReadiness::Frozen)
        } else {
            Ok(HistoryReadiness::Draft)
        }
    }
    /// Committed range authority for one logical identity.
    pub fn authority(&self, identity: &str) -> LedgerAuthority {
        match self.entries.get(identity) {
            None => LedgerAuthority::NewIdentity,
            Some(versions) => {
                let mut keys = versions.keys().copied();
                let floor = keys.next().expect("validated non-empty version map");
                let head = keys.next_back().unwrap_or(floor);
                LedgerAuthority::Frozen { floor, head }
            }
        }
    }
}

/// What committed authority permits for one event identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LedgerAuthority {
    /// Nothing is committed for this identity. Only a V1 draft is valid.
    NewIdentity,
    /// A contiguous committed range. Only that head, or the one permitted
    /// successor draft, is valid.
    Frozen {
        /// Lowest committed version.
        floor: u32,
        /// Highest committed version.
        head: u32,
    },
}

impl LedgerAuthority {
    /// Whether an inferred floor and head are authorized.
    ///
    /// This runs before the dense retained range is expanded, so a mistaken
    /// enormous boundary is rejected instead of allocated. The committed head
    /// is never combined with the inferred head: authority validates source
    /// history, it never restores history that source deleted.
    pub fn permits(&self, floor: u32, head: u32) -> Result<(), String> {
        if head < floor {
            return Err(format!(
                "inferred current version V{head} is below the retained floor V{floor}"
            ));
        }
        match *self {
            Self::NewIdentity => {
                if floor != crate::history::DEFAULT_RETAINED_FLOOR {
                    return Err(format!(
                        "retained floor V{floor} is not authorized; no committed history exists for this type, so a new identity starts at V{}. Import a real baseline with the explicit import command before declaring a higher floor",
                        crate::history::DEFAULT_RETAINED_FLOOR
                    ));
                }
                if head != crate::history::DEFAULT_RETAINED_FLOOR {
                    return Err(format!(
                        "inferred current version V{head} is not authorized; a new type identity begins with only V{}. Freeze V{} before declaring the next version",
                        crate::history::DEFAULT_RETAINED_FLOOR,
                        crate::history::DEFAULT_RETAINED_FLOOR
                    ));
                }
                Ok(())
            }
            Self::Frozen {
                floor: committed_floor,
                head: committed_head,
            } => {
                if floor != committed_floor {
                    return Err(format!(
                        "retained floor V{floor} does not match committed authority V{committed_floor}; restore the declared floor, or change it with the explicit import command"
                    ));
                }
                if head == committed_head {
                    return Ok(());
                }
                if head < committed_head {
                    return Err(format!(
                        "inferred current version V{head} is below committed head V{committed_head}; a retained version lost its last field boundary. Restore that history rather than deleting it"
                    ));
                }
                // Only reached for a head above the committed one, so the
                // committed head is strictly below `u32::MAX` and its
                // successor is representable.
                let successor = committed_head.checked_add(1).ok_or_else(|| {
                    format!(
                        "committed head V{committed_head} has no representable successor version"
                    )
                })?;
                if head != successor {
                    return Err(format!(
                        "inferred current version V{head} exceeds the one permitted successor V{successor} above committed head V{committed_head}; freeze or discard the current draft first"
                    ));
                }
                Ok(())
            }
        }
    }
}

impl Display for LedgerAuthority {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        match self {
            Self::NewIdentity => formatter.write_str("no committed history"),
            Self::Frozen { floor, head } => write!(formatter, "committed V{floor}..=V{head}"),
        }
    }
}

#[cfg(test)]
mod tests;
