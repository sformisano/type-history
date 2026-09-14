use super::compare_ledgers;
use crate::check_report::{self, CheckCode, DiagnosticCode, Location};
use crate::contract::STANDALONE;
use serde::{ser::Error, Deserialize, Serialize, Serializer};
use serde_json::json;
use type_history_codegen::ledger::{HistoryLedger, SchemaIdentity};

#[derive(Clone, PartialEq, Eq, Deserialize)]
struct FallibleMetadata(bool);
impl Serialize for FallibleMetadata {
    fn serialize<S: Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
        Err(S::Error::custom("caller metadata could not be serialized"))
    }
}

fn ledger<M: Clone + Eq + Serialize + for<'de> Deserialize<'de>>(
    metadata: bool,
    reset: bool,
) -> HistoryLedger<M> {
    HistoryLedger::parse(
        &serde_json::to_vec(&json!({"billing.invoice": {"1": {
            "metadata": metadata,
            "schema": {"$id":"urn:typehistory:schema:billing.invoice", "$schema":"https://json-schema.org/draft/2020-12/schema", "type":"object", "properties":{}, "required":[], "additionalProperties":false},
            "reset_draft": reset
        }}})).unwrap(),
        SchemaIdentity::new(STANDALONE.schema_id_prefix),
    ).unwrap()
}

#[test]
fn fallible_caller_metadata_returns_an_error_for_changes_and_missing_histories() {
    let saved = ledger::<FallibleMetadata>(false, false);
    let changed = ledger::<FallibleMetadata>(true, false);
    let empty = HistoryLedger::empty(SchemaIdentity::new(STANDALONE.schema_id_prefix));
    for actual in [&changed, &empty] {
        let error = compare_ledgers(&saved, actual, true, None, None).unwrap_err();
        assert!(error
            .to_string()
            .contains("caller metadata could not be serialized"));
    }
}

#[test]
fn simultaneous_release_and_current_resets_keep_distinct_authority_locations() {
    let saved = ledger::<bool>(false, true);
    let location = |file: &str| {
        Some(Location {
            file: file.into(),
            line: None,
            column: None,
        })
    };
    let mut findings = compare_ledgers(
        &saved,
        &saved,
        true,
        location("current.json"),
        location("release.json"),
    )
    .unwrap();
    check_report::order(&mut findings);
    assert_eq!(findings.len(), 2);
    assert!(findings
        .iter()
        .all(|d| d.code == DiagnosticCode::Check(CheckCode::ReleasedReset)));
    let files: Vec<_> = findings
        .iter()
        .map(|d| d.location.as_ref().unwrap().file.as_str())
        .collect();
    assert!(files.contains(&"current.json"));
    assert!(files.contains(&"release.json"));
}
