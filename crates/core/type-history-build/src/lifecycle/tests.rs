//! Candidate invariants tested without replacing public CLI proofs.
use super::candidate::{compare_frozen, reset_candidate};
use crate::contract::STANDALONE;
use crate::options::{self as args, LifecycleOptions as Options};
use crate::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map};
use std::collections::BTreeMap;
use std::ffi::OsString;
use type_history_codegen::json_schema::JsonSchemaDocument;
use type_history_codegen::ledger::{HistoryLedger, SchemaIdentity, Snapshot};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TestMetadata {
    label: TestLabel,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
enum TestLabel {
    Original,
    Changed,
}
type TestBaseline = HistoryLedger<TestMetadata>;
type TestSnapshot = Snapshot<TestMetadata>;
fn freeze_candidate(
    saved: &TestBaseline,
    observed: &TestBaseline,
    options: &Options,
) -> Result<TestBaseline> {
    super::candidate::freeze_candidate(saved, observed, options, &STANDALONE)
}

const ID: &str = "test.account.opened";
fn options(arguments: &[&str]) -> Options {
    args::parse(arguments.iter().map(OsString::from), &STANDALONE)
        .unwrap()
        .unwrap()
}
fn snapshot(fields: &[&str]) -> TestSnapshot {
    let properties = fields
        .iter()
        .map(|name| ((*name).to_owned(), json!({"type":"string"})))
        .collect::<Map<_, _>>();
    TestSnapshot { metadata: TestMetadata { label: TestLabel::Original }, schema: JsonSchemaDocument::from_export(json!({"type":"object","properties":properties,"required":fields,"additionalProperties":false}), &format!("urn:typehistory:schema:{ID}")).unwrap(), reset_draft: false }
}
fn baseline(versions: &[(u32, &[&str])]) -> TestBaseline {
    TestBaseline::from_entries(
        BTreeMap::from([(
            ID.into(),
            versions
                .iter()
                .map(|(version, fields)| (*version, snapshot(fields)))
                .collect(),
        )]),
        SchemaIdentity::new(STANDALONE.schema_id_prefix),
    )
    .unwrap()
}
#[test]
fn exact_mutation_scope_and_selectors() {
    for invalid in [
        vec!["freeze"],
        vec!["reset", "-p", "domain"],
        vec!["freeze", "-p", "domain", "--type", ID],
        vec!["freeze", "-p", "*"],
        vec!["reset", "-p", "domain", "--type", "*", "--version", "1"],
        vec!["check", "--undo"],
        vec!["freeze", "-p", "domain", "--from", "old.json"],
        vec!["reset", "-p", "domain", "--type", ID, "--version", "01"],
    ] {
        assert!(args::parse(invalid.into_iter().map(OsString::from), &STANDALONE).is_err());
    }
    assert!(args::parse([OsString::from("update")], &STANDALONE)
        .unwrap_err()
        .to_string()
        .contains("freeze"));
}
#[test]
fn frozen_wire_drift_cannot_be_frozen_again() {
    let saved = baseline(&[(1, &["owner"])]);
    let current = baseline(&[(1, &["owner", "currency"])]);
    for selection in [
        vec!["freeze", "-p", "domain"],
        vec!["freeze", "-p", "domain", "--type", ID, "--version", "1"],
    ] {
        assert!(freeze_candidate(&saved, &current, &options(&selection)).is_err());
    }
}
#[test]
fn freeze_preserves_unselected_drafts_and_exact_frozen_noop() {
    let saved = baseline(&[(1, &["owner"])]);
    let observed = baseline(&[(1, &["owner"]), (2, &["owner", "currency"])]);
    let unchanged = freeze_candidate(
        &saved,
        &observed,
        &options(&["freeze", "-p", "domain", "--type", ID, "--version", "1"]),
    )
    .unwrap();
    assert_eq!(unchanged, saved);
    let frozen =
        freeze_candidate(&saved, &observed, &options(&["freeze", "-p", "domain"])).unwrap();
    assert_eq!(frozen, observed);
}
#[test]
fn reset_preserves_original_shape_and_requires_exact_close() {
    let saved = baseline(&[(1, &["owner"]), (2, &["owner", "currency"])]);
    let reset = reset_candidate(
        &saved,
        &options(&["reset", "-p", "domain", "--type", ID, "--version", "2"]),
    )
    .unwrap();
    assert_eq!(
        reset.snapshot(ID, 2).unwrap().schema,
        saved.snapshot(ID, 2).unwrap().schema
    );
    assert!(reset.snapshot(ID, 2).unwrap().reset_draft);
    let observed = baseline(&[(1, &["owner"]), (2, &["owner", "locale"])]);
    assert!(compare_frozen(&reset, &observed).is_ok());
    assert!(freeze_candidate(&reset, &observed, &options(&["freeze", "-p", "domain"])).is_err());
    let closed = freeze_candidate(
        &reset,
        &observed,
        &options(&["freeze", "-p", "domain", "--type", ID, "--version", "2"]),
    )
    .unwrap();
    assert_eq!(closed, observed);
    let undo = reset_candidate(
        &reset,
        &options(&[
            "reset",
            "-p",
            "domain",
            "--type",
            ID,
            "--version",
            "2",
            "--undo",
        ]),
    )
    .unwrap();
    assert_eq!(undo, saved);
    assert!(compare_frozen(&undo, &observed).is_err());
}
#[test]
fn historical_reset_and_inventory_deletion_are_rejected() {
    let saved = baseline(&[(1, &["owner"]), (2, &["owner", "currency"])]);
    assert!(reset_candidate(
        &saved,
        &options(&["reset", "-p", "domain", "--type", ID, "--version", "1"])
    )
    .is_err());
    assert!(compare_frozen(
        &saved,
        &TestBaseline::empty(SchemaIdentity::new(STANDALONE.schema_id_prefix))
    )
    .is_err());
}
#[test]
fn metadata_and_unrelated_shape_remain_frozen_during_reset() {
    let saved = baseline(&[(1, &["owner"]), (2, &["owner", "currency"])]);
    let reset = reset_candidate(
        &saved,
        &options(&["reset", "-p", "domain", "--type", ID, "--version", "2"]),
    )
    .unwrap();
    let changed = baseline(&[(1, &["changed"]), (2, &["owner", "locale"])]);
    assert!(compare_frozen(&reset, &changed).is_err());
    let mut entries = reset.entries().clone();
    for value in entries.get_mut(ID).unwrap().values_mut() {
        value.metadata.label = TestLabel::Changed;
        value.reset_draft = false;
    }
    let changed =
        TestBaseline::from_entries(entries, SchemaIdentity::new(STANDALONE.schema_id_prefix))
            .unwrap();
    assert!(compare_frozen(&reset, &changed).is_err());
}
