//! Ordinary builds protect retained source records, not callback semantics.

use std::fs;

use serde_json::Value;
use type_history_core::resolved::SchemaShape;

use super::super::support::{
    failure as assert_failure, success as assert_success, text as output_text, Fixture,
};
use super::{fixture, ledger, source};

const RETIRED: &str = " #[history(removed_in = v2)] retired: String,";
const UPDATE: &str = "#[history(updated_in = v2, previous_type = u16, backfill_fn = widen)]";
const HEAD: &str = " #[history(added_in = v3, backfill_value = 0)] marker: u8,";

fn retained_source() -> String {
    source(
        &format!("{RETIRED}  {UPDATE} count: u32, {HEAD}"),
        "fn widen(previous_payload: &RecordV1) -> Result<u32, Infallible> { let value = previous_payload.count.clone(); Ok(u32::from(value)) }",
        "",
    )
}

fn snapshots() -> Value {
    ledger(vec![
        vec![
            ("retired", SchemaShape::String),
            ("count", SchemaShape::U16),
        ],
        vec![("count", SchemaShape::U32)],
        vec![("count", SchemaShape::U32), ("marker", SchemaShape::U8)],
    ])
}

fn ordinary_command(fixture: &Fixture, command: &str, expected: Option<&str>, cell: &str) {
    let paths = [
        "Cargo.toml",
        "Cargo.lock",
        "type-history/schemas.json",
        "build.rs",
        "src/lib.rs",
    ];
    let before = paths.map(|path| {
        let bytes = fs::read(fixture.root().join(path)).expect("fixture input");
        (path, bytes)
    });
    let output = fixture.cargo(&[command, "--locked", "--offline"]);
    if let Some(expected) = expected {
        assert_failure(&output, expected);
        let text = output_text(&output);
        for unrelated in ["error[E0308]", "error[E0425]", "error[E0277]"] {
            assert!(
                !text.contains(unrelated),
                "unrelated failure in {cell}: {text}"
            );
        }
    } else {
        assert_success(&output);
    }
    for (path, bytes) in before {
        let after = fs::read(fixture.root().join(path)).expect("input after command");
        assert_eq!(bytes, after, "{cell}: {command} changed {path}");
    }
}

#[test]
fn frozen_history_deletions_fail_fresh_and_warm_check_and_build() {
    let original = retained_source();
    let cases = [
        (
            "retired field deletion",
            original.replace(RETIRED, ""),
            "billing.invoice.issued V1 field `retired` disappeared from frozen history",
        ),
        (
            "historical from record deletion",
            original.replace(UPDATE, ""),
            "billing.invoice.issued V1 field `count` changed its frozen wire shape",
        ),
        (
            "sole frozen head boundary deletion",
            original.replace(HEAD, ""),
            "inferred current version V2 is below committed head V3; a retained version lost its last field boundary",
        ),
    ];
    for command in ["check", "build"] {
        // Fresh means this unique consumer has never compiled. Dependency artifacts
        // remain shared under the owned worktree target, as required by the harness.
        for (name, invalid, diagnostic) in &cases {
            assert_ne!(invalid, &original, "stale mutation: {name}");
            let fixture = fixture(invalid);
            fixture.set_ledger(&snapshots());
            ordinary_command(
                &fixture,
                command,
                Some(diagnostic),
                &format!("fresh {name}"),
            );
        }
        let fixture = fixture(&original);
        fixture.set_ledger(&snapshots());
        ordinary_command(&fixture, command, None, "warm baseline");
        for (name, invalid, diagnostic) in &cases {
            fixture.write("src/lib.rs", invalid);
            ordinary_command(&fixture, command, Some(diagnostic), &format!("warm {name}"));
            fixture.write("src/lib.rs", &original);
            ordinary_command(&fixture, command, None, &format!("restored {name}"));
        }
    }
}

#[test]
fn frozen_same_wire_callback_semantics_can_change_without_advancing_head() {
    let payload = r#"

        #[history(updated_in = v2, previous_type = String, backfill_fn = normalize)]
        label: String,
         #[history(added_in = v3, backfill_value = 0)] marker: u8,
    "#;
    let helpers = "fn normalize(previous_payload: &RecordV1) -> Result<String, Infallible> { let value = previous_payload.label.clone(); Ok(value.trim().to_owned()) }";
    let checks = r##"
        #[test] fn semantics_are_author_owned() {
            assert_eq!(Record::VERSION.get(), 3);
            let current: RecordV3 = Record::history()
                .decode(version(1), br#"{"label":" Ada "}"#).unwrap();
            assert_eq!(current.label, "Ada");
            assert_eq!(current.marker, 0);
        }
    "##;
    let original = source(payload, helpers, checks);
    let fixture = fixture(&original);
    fixture.set_ledger(&ledger(vec![
        vec![("label", SchemaShape::String)],
        vec![("label", SchemaShape::String)],
        vec![("label", SchemaShape::String), ("marker", SchemaShape::U8)],
    ]));
    let frozen = fs::read(fixture.root().join("type-history/schemas.json")).unwrap();
    assert_success(&fixture.cargo(&["test", "--lib", "--locked", "--offline"]));
    let changed = source(
        payload,
        &helpers.replace("trim().to_owned()", "trim().to_uppercase()"),
        &checks.replace("current.label, \"Ada\"", "current.label, \"ADA\""),
    );
    assert_ne!(original, changed);
    fixture.write("src/lib.rs", &changed);
    for command in ["check", "build"] {
        ordinary_command(&fixture, command, None, "same-wire callback edit");
    }
    assert_success(&fixture.cargo(&["test", "--lib", "--locked", "--offline"]));
    assert_eq!(
        frozen,
        fs::read(fixture.root().join("type-history/schemas.json")).unwrap()
    );
}

#[test]
fn wire_equal_nominal_substitution_and_same_type_record_removal_preserve_head() {
    let update =
        "#[history(updated_in = v2, previous_type = OriginalLabel, backfill_fn = normalize)]";
    let payload = format!(
        "{update} label: OriginalLabel,  #[history(added_in = v3, backfill_value = 0)] marker: u8,"
    );
    let helpers = r#"
        use history_api::Schema;
        use serde::{Deserialize, Serialize};
        #[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Schema)] pub struct OriginalLabel(String);
        impl TryFrom<String> for OriginalLabel {
            type Error = Infallible;
            fn try_from(value: String) -> Result<Self, Self::Error> { Ok(Self(value)) }
        }
        impl From<OriginalLabel> for String { fn from(value: OriginalLabel) -> Self { value.0 } }
        #[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Schema)] pub struct ReplacementLabel(String);
        impl TryFrom<String> for ReplacementLabel {
            type Error = Infallible;
            fn try_from(value: String) -> Result<Self, Self::Error> { Ok(Self(value)) }
        }
        fn normalize(previous_payload: &RecordV1) -> Result<OriginalLabel, Infallible> { let value = previous_payload.label.clone();
            OriginalLabel::try_from(String::from(value).trim().to_owned())
        }
    "#;
    let checks = r##"
        #[test] fn wire_contract_and_head_remain_stable() {
            let current: RecordV3 = Record::history()
                .decode(version(1), br#"{"label":" Ada "}"#).unwrap();
            assert_eq!(current.label.0, "Ada");
            assert_eq!(current.marker, 0);
            assert_eq!(Record::VERSION.get(), 3);
        }
    "##;
    let original = source(&payload, helpers, checks);
    let fixture = fixture(&original);
    fixture.set_ledger(&ledger(vec![
        vec![("label", SchemaShape::String)],
        vec![("label", SchemaShape::String)],
        vec![("label", SchemaShape::String), ("marker", SchemaShape::U8)],
    ]));
    let frozen = fs::read(fixture.root().join("type-history/schemas.json")).unwrap();
    assert_success(&fixture.cargo(&["test", "--lib", "--locked", "--offline"]));
    let nominal = source(
        &payload.replace("label: OriginalLabel", "label: ReplacementLabel"),
        &helpers
            .replace(
                "Result<OriginalLabel, Infallible>",
                "Result<ReplacementLabel, Infallible>",
            )
            .replace(
                "OriginalLabel::try_from(String::from(value)",
                "ReplacementLabel::try_from(String::from(value)",
            ),
        checks,
    );
    let removed = source(
        &payload.replace(update, ""),
        helpers,
        &checks.replace("current.label.0, \"Ada\"", "current.label.0, \" Ada \""),
    );
    for (name, changed) in [
        ("wire-equal distinct nominal type", nominal),
        ("removed same-type update", removed),
    ] {
        assert_ne!(original, changed);
        fixture.write("src/lib.rs", &changed);
        for command in ["check", "build"] {
            ordinary_command(&fixture, command, None, name);
        }
        assert_success(&fixture.cargo(&["test", "--lib", "--locked", "--offline"]));
        assert_eq!(
            frozen,
            fs::read(fixture.root().join("type-history/schemas.json")).unwrap()
        );
    }
}
