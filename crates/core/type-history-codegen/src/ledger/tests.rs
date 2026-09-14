use super::{
    HistoryLedger, HistoryReadiness, LedgerAuthority, LedgerError, SchemaIdentity, Snapshot,
};
use crate::history::HistoryPlan;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Role {
    Creation,
    Update,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Metadata {
    role: Role,
}

const RELATIVE_PATH: &str = "type-history/schemas.json";
fn parse(bytes: &[u8]) -> Result<HistoryLedger<Metadata>, String> {
    HistoryLedger::parse(bytes, SchemaIdentity::new("urn:typehistory:schema:"))
}
fn from_entries(
    entries: BTreeMap<String, BTreeMap<u32, Snapshot<Metadata>>>,
) -> Result<HistoryLedger<Metadata>, String> {
    HistoryLedger::from_entries(entries, SchemaIdentity::new("urn:typehistory:schema:"))
}
fn read(root: &Path) -> Result<HistoryLedger<Metadata>, LedgerError> {
    HistoryLedger::read_file(
        &root.join(RELATIVE_PATH),
        SchemaIdentity::new("urn:typehistory:schema:"),
    )
}

#[test]
fn nested_duplicate_schema_properties_are_rejected() {
    let bytes = ledger("a.events.registered", &[1]).replace(
        "\"type\":\"string\"",
        "\"type\":\"string\",\"type\":\"string\"",
    );
    let error = parse(bytes.as_bytes()).unwrap_err();
    assert!(error.contains("duplicate JSON key `type`"), "{error}");
}

#[test]
fn ledger_stable_name_and_source_role_remain_authoritative() {
    let stable_name = "a.events.registered";
    let invalid = ledger(stable_name, &[1]).replace(
        "urn:typehistory:schema:a.events.registered",
        "urn:typehistory:schema:a.events.different",
    );
    assert!(parse(invalid.as_bytes()).unwrap_err().contains("$id"));
    let baseline = parse(ledger(stable_name, &[1]).as_bytes()).unwrap();
    assert!(baseline
        .authorize_history(
            stable_name,
            &Metadata { role: Role::Update },
            HistoryPlan { head: 1 }
        )
        .unwrap_err()
        .contains("metadata"));
}

#[test]
fn resets_keep_the_latest_reservation_and_block_successors() {
    let stable_name = "a.events.registered";
    let baseline = parse(ledger(stable_name, &[1, 2]).as_bytes()).unwrap();
    let mut entries = baseline.entries().clone();
    entries
        .get_mut(stable_name)
        .unwrap()
        .get_mut(&1)
        .unwrap()
        .reset_draft = true;
    assert!(from_entries(entries).unwrap_err().contains("latest"));
    let mut entries = baseline.entries().clone();
    entries
        .get_mut(stable_name)
        .unwrap()
        .get_mut(&2)
        .unwrap()
        .reset_draft = true;
    let reset = from_entries(entries).unwrap();
    assert_eq!(
        reset
            .authorize_history(
                stable_name,
                &Metadata {
                    role: Role::Creation
                },
                HistoryPlan { head: 2 }
            )
            .unwrap(),
        HistoryReadiness::ResetDraft
    );
    assert!(reset
        .authorize_history(
            stable_name,
            &Metadata {
                role: Role::Creation
            },
            HistoryPlan { head: 3 }
        )
        .unwrap_err()
        .contains("cannot have a successor"));
    assert_eq!(
        reset.snapshot(stable_name, 2).unwrap().schema,
        baseline.snapshot(stable_name, 2).unwrap().schema
    );
}

fn document(stable_name: &str, property: &str) -> String {
    format!(
        r#"{{"$schema":"https://json-schema.org/draft/2020-12/schema","$id":"urn:typehistory:schema:{stable_name}","type":"object","additionalProperties":false,"required":["{property}"],"properties":{{"{property}":{{"type":"string"}}}}}}"#
    )
}

fn ledger(stable_name: &str, versions: &[u32]) -> String {
    let entries = versions
        .iter()
        .map(|version| {
            format!(
                r#""{version}":{{"metadata":{{"role":"creation"}},"schema":{}}}"#,
                document(stable_name, "owner_name")
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(r#"{{"{stable_name}":{{{entries}}}}}"#)
}

#[test]
fn an_absent_stable_name_has_no_committed_history() {
    let baseline = parse(ledger("a.events.registered", &[1]).as_bytes()).expect("valid authority");
    assert_eq!(
        baseline.authority("a.events.other"),
        LedgerAuthority::NewHistory
    );
}

#[test]
fn a_committed_stable_name_reports_its_exact_range() {
    let baseline =
        parse(ledger("a.events.registered", &[1, 2, 3, 4]).as_bytes()).expect("valid authority");
    assert_eq!(
        baseline.authority("a.events.registered"),
        LedgerAuthority::Frozen { head: 4 }
    );
}

#[test]
fn every_history_starts_at_version_one() {
    for versions in [&[2][..], &[3, 4][..]] {
        let error =
            parse(ledger("a.events.registered", versions).as_bytes()).expect_err("V1 is required");
        assert!(error.contains("expected V1"), "{error}");
    }
}

#[test]
fn a_gap_in_the_retained_range_is_rejected() {
    let error =
        parse(ledger("a.events.registered", &[1, 3]).as_bytes()).expect_err("a gap is invalid");
    assert!(error.contains("non-contiguous"), "{error}");
}

#[test]
fn a_duplicate_version_key_is_rejected() {
    let stable_name = "a.events.registered";
    let schema = document(stable_name, "owner_name");
    let bytes = format!(
        r#"{{"{stable_name}":{{"1":{{"metadata":{{"role":"creation"}},"schema":{schema}}},"1":{{"metadata":{{"role":"creation"}},"schema":{schema}}}}}}}"#
    );
    let error = parse(bytes.as_bytes()).expect_err("a duplicate key is invalid");
    assert!(error.contains("duplicate JSON key"), "{error}");
}

#[test]
fn a_duplicate_stable_name_key_is_rejected() {
    let stable_name = "a.events.registered";
    let schema = document(stable_name, "owner_name");
    let entry = format!(
        r#""{stable_name}":{{"1":{{"metadata":{{"role":"creation"}},"schema":{schema}}}}}"#
    );
    let bytes = format!("{{{entry},{entry}}}");
    let error = parse(bytes.as_bytes()).expect_err("a duplicate key is invalid");
    assert!(error.contains("duplicate JSON key"), "{error}");
}

#[test]
fn role_disagreement_within_one_stable_name_is_rejected() {
    let stable_name = "a.events.registered";
    let schema = document(stable_name, "owner_name");
    let bytes = format!(
        r#"{{"{stable_name}":{{"1":{{"metadata":{{"role":"creation"}},"schema":{schema}}},"2":{{"metadata":{{"role":"update"}},"schema":{schema}}}}}}}"#
    );
    let error = parse(bytes.as_bytes()).expect_err("role drift is invalid");
    assert!(error.contains("disagrees about its metadata"), "{error}");
}

#[test]
fn an_absent_file_is_not_empty_authority() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let error = read(directory.path()).expect_err("restore or explicitly initialize authority");
    assert!(matches!(error, LedgerError::Unreadable { .. }));
}

#[test]
fn a_present_but_corrupt_file_is_never_read_as_empty_history() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    fs::create_dir_all(directory.path().join("type-history")).expect("authority directory");
    fs::write(directory.path().join(RELATIVE_PATH), b"{ this is not json")
        .expect("corrupt authority");
    let error = read(directory.path()).expect_err("corruption is an error");
    assert!(matches!(error, LedgerError::Invalid { .. }));
}

#[test]
fn a_new_history_permits_only_version_one() {
    assert!(LedgerAuthority::NewHistory.permits(1).is_ok());
    let error = LedgerAuthority::NewHistory
        .permits(2)
        .expect_err("a new stable_name cannot begin above V1");
    assert!(error.contains("begins with only V1"), "{error}");
}

#[test]
fn committed_authority_permits_its_head_and_one_successor() {
    let authority = LedgerAuthority::Frozen { head: 4 };
    assert!(authority.permits(4).is_ok());
    assert!(authority.permits(5).is_ok());
}

#[test]
fn a_second_successor_draft_is_rejected() {
    let authority = LedgerAuthority::Frozen { head: 4 };
    let error = authority
        .permits(6)
        .expect_err("only one draft is permitted");
    assert!(error.contains("one permitted successor"), "{error}");
}

#[test]
fn an_enormous_boundary_is_rejected_before_expansion() {
    let authority = LedgerAuthority::Frozen { head: 1 };
    let error = authority
        .permits(4_000_000_000)
        .expect_err("an enormous head is invalid");
    assert!(error.contains("one permitted successor"), "{error}");
}

#[test]
fn a_head_below_committed_authority_is_rejected() {
    let authority = LedgerAuthority::Frozen { head: 3 };
    let error = authority
        .permits(2)
        .expect_err("deleted history is invalid");
    assert!(error.contains("below committed head"), "{error}");
}

#[test]
fn a_head_at_the_representable_ceiling_is_permitted_without_arithmetic() {
    let authority = LedgerAuthority::Frozen { head: u32::MAX };
    assert!(authority.permits(u32::MAX).is_ok());
}

#[test]
fn noncanonical_and_invalid_positive_version_keys_are_rejected() {
    for key in ["0", "01", "+1", " 1", "4294967296"] {
        let bytes = ledger("a.events.registered", &[1]).replace("\"1\":", &format!("\"{key}\":"));
        assert!(parse(bytes.as_bytes()).is_err(), "version key {key}");
    }
}

#[test]
fn strict_metadata_and_snapshot_keys_are_rejected() {
    let original = ledger("a.events.registered", &[1]);
    for bytes in [
        original.replace(
            "\"role\":\"creation\"",
            "\"role\":\"creation\",\"unknown\":true",
        ),
        original.replace("\"schema\":", "\"unknown\":true,\"schema\":"),
    ] {
        assert!(parse(bytes.as_bytes()).is_err());
    }
}
