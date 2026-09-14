//! Strict committed history authority shared by compilation and lifecycle tools.
use crate::{history::HistoryPlan, json_schema::JsonSchemaDocument};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fmt::{Display, Formatter, Result as FmtResult},
    fs,
    path::{Path, PathBuf},
};
use type_history_core::StableName;

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
    pub fn schema_id(&self, stable_name: &str) -> String {
        format!("{}{stable_name}", self.prefix)
    }
}

/// Type History declarations carry no additional metadata.
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
        for (stable_name, raw_versions) in raw {
            StableName::try_from(stable_name.clone())
                .map_err(|error| format!("invalid stable name `{stable_name}`: {error}"))?;
            if raw_versions.is_empty() {
                return Err(format!("{stable_name} records no versions"));
            }
            let mut versions = BTreeMap::new();
            for (key, snapshot) in raw_versions {
                let version = key.parse::<u32>().map_err(|error| error.to_string())?;
                if version == 0 || version.to_string() != key {
                    return Err(format!(
                        "version key `{key}` must be a canonical positive integer"
                    ));
                }
                if snapshot.schema.identity() != schema_identity.schema_id(&stable_name) {
                    return Err(format!(
                        "{stable_name} V{version}: schema $id disagrees with its stable name"
                    ));
                }
                versions.insert(version, snapshot);
            }
            let (head, latest) = versions
                .last_key_value()
                .expect("non-empty parsed version map");
            let metadata = &latest.metadata;
            for (offset, (version, snapshot)) in versions.iter().enumerate() {
                if &snapshot.metadata != metadata {
                    return Err(format!(
                        "{stable_name} disagrees about its metadata across retained versions"
                    ));
                }
                if snapshot.reset_draft && version != head {
                    return Err(format!("{stable_name} V{version}: reset_draft is permitted only on the latest retained version V{head}"));
                }
                let expected = 1_u32
                    .checked_add(u32::try_from(offset).map_err(|_| {
                        format!("{stable_name} records more versions than the supported range")
                    })?)
                    .ok_or_else(|| {
                        format!("{stable_name} records a version above the supported range")
                    })?;
                if *version != expected {
                    return Err(format!("{stable_name} retains a non-contiguous range; expected V{expected}, found V{version}"));
                }
            }
            entries.insert(stable_name, versions);
        }
        Ok(Self {
            entries,
            schema_identity,
        })
    }
    /// Every retained stable name in deterministic order.
    pub fn stable_names(&self) -> impl Iterator<Item = &str> {
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
    pub fn snapshot(&self, stable_name: &str, version: u32) -> Option<&Snapshot<M>> {
        self.entries.get(stable_name)?.get(&version)
    }
    /// Authorize the source range and metadata before dense reconstruction.
    pub fn authorize_history(
        &self,
        stable_name: &str,
        metadata: &M,
        plan: HistoryPlan,
    ) -> Result<HistoryReadiness, String> {
        StableName::try_from(stable_name.to_owned()).map_err(|error| error.to_string())?;
        self.authority(stable_name)
            .permits(plan.head)
            .map_err(|reason| format!("{stable_name}: {reason}"))?;
        let Some(versions) = self.entries.get(stable_name) else {
            return Ok(HistoryReadiness::Draft);
        };
        if versions
            .values()
            .any(|snapshot| &snapshot.metadata != metadata)
        {
            return Err(format!("{stable_name}: source metadata differs from frozen history; restore its original metadata"));
        }
        let (head, latest) = versions
            .last_key_value()
            .expect("validated non-empty inventory");
        if latest.reset_draft {
            if plan.head != *head {
                return Err(format!("{stable_name} V{head}: a reset draft cannot have a successor; freeze or undo this exact reset first"));
            }
            Ok(HistoryReadiness::ResetDraft)
        } else if plan.head == *head {
            Ok(HistoryReadiness::Frozen)
        } else {
            Ok(HistoryReadiness::Draft)
        }
    }
    /// Committed range authority for one stable name.
    pub fn authority(&self, stable_name: &str) -> LedgerAuthority {
        match self.entries.get(stable_name) {
            None => LedgerAuthority::NewHistory,
            Some(versions) => {
                let head = *versions
                    .last_key_value()
                    .expect("validated non-empty version map")
                    .0;
                LedgerAuthority::Frozen { head }
            }
        }
    }
}

/// What committed authority permits for one history.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LedgerAuthority {
    /// Nothing is committed for this stable name. Only a V1 draft is valid.
    NewHistory,
    /// A contiguous committed range. Only that head, or the one permitted
    /// successor draft, is valid.
    Frozen {
        /// Highest committed version.
        head: u32,
    },
}

impl LedgerAuthority {
    /// Whether an inferred head is authorized.
    ///
    /// This runs before the dense retained range is expanded, so a mistaken
    /// enormous boundary is rejected instead of allocated. The committed head
    /// is never combined with the inferred head: authority validates source
    /// history, it never restores history that source deleted.
    pub fn permits(&self, head: u32) -> Result<(), String> {
        if head == 0 {
            return Err("history versions are positive".into());
        }
        match *self {
            Self::NewHistory => {
                if head != 1 {
                    return Err(format!(
                        "inferred current version V{head} is not authorized; a new stable name begins with only V1. Freeze V1 before declaring the next version"
                    ));
                }
                Ok(())
            }
            Self::Frozen {
                head: committed_head,
            } => {
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
            Self::NewHistory => formatter.write_str("no committed history"),
            Self::Frozen { head } => write!(formatter, "committed V1..=V{head}"),
        }
    }
}

#[cfg(test)]
mod tests;
