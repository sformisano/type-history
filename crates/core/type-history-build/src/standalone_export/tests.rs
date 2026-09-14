//! Exact exported inventory is checked before any lifecycle candidate is built.
use super::decode;
use crate::contract::STANDALONE;
use crate::inventory::{Declaration, PackageInventory};
use serde_json::{json, Value};
use type_history_codegen::ledger::{HistoryReadiness, RecordMetadata};

const ID: &str = "billing.invoice.issued";
fn inventory() -> PackageInventory<RecordMetadata> {
    PackageInventory {
        package: "billing".into(),
        root: "/captured/billing".into(),
        declarations: vec![Declaration {
            name: "Invoice".into(),
            stable_name: ID.into(),
            metadata: RecordMetadata {},
            version: 2,
            retained_versions: vec![1, 2],
            readiness: HistoryReadiness::Draft,
        }],
        tracked_paths: Vec::new(),
        declaration_count: 1,
    }
}
fn row(stable_name: &str, versions: &[u32]) -> Value {
    json!({"stable_name": stable_name, "versions": versions.iter().map(|version| json!({
        "version": version,
        "wire": {"type":"object", "properties":{"owner":{"type":"string"}}, "required":["owner"], "additionalProperties":false}
    })).collect::<Vec<_>>()})
}
fn output(rows: &[Value]) -> Vec<u8> {
    rows.iter()
        .map(|row| format!("{}{}\n", STANDALONE.export_marker, row))
        .collect::<String>()
        .into_bytes()
}

#[test]
fn complete_exports_keep_exact_stable_name_versions_schema_and_empty_metadata() {
    let ledger = decode(&inventory(), &output(&[row(ID, &[1, 2])])).unwrap();
    assert_eq!(ledger.stable_names().collect::<Vec<_>>(), vec![ID]);
    assert_eq!(
        ledger.entries()[ID].keys().copied().collect::<Vec<_>>(),
        vec![1, 2]
    );
    for snapshot in ledger.entries()[ID].values() {
        assert_eq!(
            snapshot.schema.identity(),
            format!("urn:typehistory:schema:{ID}")
        );
        assert_eq!(serde_json::to_value(snapshot.metadata).unwrap(), json!({}));
        assert!(!snapshot.reset_draft);
    }
}

#[test]
fn missing_duplicate_unexpected_and_incomplete_exports_are_rejected() {
    let cases = [
        (vec![], "incomplete history export"),
        (
            vec![row(ID, &[1, 2]), row(ID, &[1, 2])],
            "duplicate exported history",
        ),
        (
            vec![row(ID, &[1, 1, 2])],
            "duplicate exported history version",
        ),
        (
            vec![row("billing.invoice.other", &[1, 2])],
            "unexpected exported history",
        ),
        (vec![row(ID, &[1])], "incomplete exported retained history"),
        (vec![row(ID, &[0, 1, 2])], "exported version differs"),
        (vec![row(ID, &[1, 2, 3])], "exported version differs"),
    ];
    for (rows, expected) in cases {
        let error = decode(&inventory(), &output(&rows))
            .unwrap_err()
            .to_string();
        assert!(error.contains(expected), "{expected}: {error}");
    }
}
