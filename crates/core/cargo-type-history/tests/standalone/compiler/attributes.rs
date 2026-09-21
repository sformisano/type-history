//! Field lifetimes and associated paths through the public compiler.

use super::super::support::{failure as assert_failure, success as assert_success};
use super::{fixture, ledger, source};
use type_history_core::resolved::SchemaShape;

#[test]
fn associated_type_and_const_paths_survive_append_and_discard() {
    let payload = r#"

        #[history(updated_in = v2, previous_type = <Types as FieldTypes>::Old, backfill_fn = Types::convert)]
        label: <Types as FieldTypes>::Current,
         salt: [u8; <Types as FieldTypes>::WIDTH],
    "#;
    let helpers = r#"
        pub trait FieldTypes { type Old; type Current; const WIDTH: usize; }
        pub struct Types;
        impl FieldTypes for Types { type Old = String; type Current = String; const WIDTH: usize = 2; }
        impl Types {
            fn convert(previous_payload: &RecordV1) -> Result<<Self as FieldTypes>::Current, Infallible> { let value = previous_payload.label.clone(); Ok(value.to_uppercase()) }
            fn backfill() -> <Self as FieldTypes>::Current { "added".to_owned() }
        }
    "#;
    let checks = r##"
        #[test] fn resolve_paths_in_author_scope() {
            let current: RecordV2 = Record::history()
                .decode(version(1), br#"{"label":"ada","salt":[2,7]}"#).unwrap();
            assert_eq!(current.label, "ADA");
            assert_eq!(current.salt, [2,7]);
            assert_eq!(Record::VERSION.get(), 2);
        }
    "##;
    let array = SchemaShape::Array {
        value: Box::new(SchemaShape::U8),
        length: 2,
    };
    let original = source(payload, helpers, checks);
    let fixture = fixture(&original);
    fixture.set_ledger(&ledger(vec![
        vec![("label", SchemaShape::String), ("salt", array.clone())],
        vec![("label", SchemaShape::String), ("salt", array)],
    ]));
    for command in ["check", "build"] {
        assert_success(&fixture.cargo(&[command, "--locked", "--offline"]));
    }
    assert_success(&fixture.cargo(&["test", "--lib", "--locked", "--offline"]));
    let draft = source(
        &format!(
            "{payload}  #[history(added_in = v3, backfill_value = Types::backfill())] note: <Types as FieldTypes>::Current,"
        ),
        helpers,
        &checks
            .replace("RecordV2", "RecordV3")
            .replace("VERSION.get(), 2", "VERSION.get(), 3")
            .replace(
                "assert_eq!(current.label",
                "assert_eq!(current.note, \"added\"); assert_eq!(current.label",
            ),
    );
    fixture.write("src/lib.rs", &draft);
    for command in ["check", "build"] {
        assert_success(&fixture.cargo(&[command, "--locked", "--offline"]));
    }
    assert_success(&fixture.cargo(&["test", "--lib", "--locked", "--offline"]));
    fixture.write("src/lib.rs", &original);
    for command in ["check", "build"] {
        assert_success(&fixture.cargo(&[command, "--locked", "--offline"]));
    }
    assert_success(&fixture.cargo(&["test", "--lib", "--locked", "--offline"]));
}

#[test]
fn retired_field_can_have_a_distinct_successor_but_cannot_reuse_its_identifier() {
    let payload = r#"
         #[history(removed_in = v2)] label: String,
         #[history(added_in = v3, backfill_value = String::from("new"))] replacement_label: String,
    "#;
    let input = source(
        payload,
        "",
        r##"
        #[test] fn distinct_successor_preserves_both_lifetimes() {
            let old: RecordV1 = serde_json::from_slice(br#"{"label":"historical"}"#).unwrap();
            assert_eq!(old.label, "historical");
            let intermediate: RecordV2 = serde_json::from_slice(b"{}").unwrap();
            assert_eq!(serde_json::to_value(intermediate).unwrap(), serde_json::json!({}));
            let current = Record::history()
                .decode(version(1), br#"{"label":"historical"}"#).unwrap();
            assert_eq!(current.replacement_label, "new");
            assert_eq!(serde_json::to_value(current).unwrap(), serde_json::json!({"replacement_label":"new"}));
        }
    "##,
    );
    let fixture = fixture(&input);
    fixture.set_ledger(&ledger(vec![
        vec![("label", SchemaShape::String)],
        vec![],
        vec![("replacement_label", SchemaShape::String)],
    ]));
    assert_success(&fixture.cargo(&["test", "--lib", "--locked", "--offline"]));
    for identifier in ["label", "r#label"] {
        fixture.write(
            "src/lib.rs",
            &source(
                &payload.replace("replacement_label:", &format!("{identifier}:")),
                "",
                "",
            ),
        );
        assert_failure(
            &fixture.cargo(&["check", "--locked", "--offline"]),
            "duplicate retained field `label`; a removed field name cannot be reused",
        );
    }
}

#[test]
fn inactive_update_intervals_and_removal_callbacks_are_rejected() {
    let original = source(" count: u32,", "", "");
    let fixture = fixture(&original);
    fixture.set_ledger(&ledger(vec![vec![("count", SchemaShape::U32)]]));
    for (attributes, diagnostic) in [
        (
            "#[history(added_in = v3, backfill_value = 0)] #[history(updated_in = v2, previous_type = u32, backfill_fn = cast)]",
            "Record.count declares a birth after a later record; birth precedes every update and removal",
        ),
        (
            "#[history(removed_in = v2)] #[history(updated_in = v2, previous_type = u32, backfill_fn = cast)]",
            "Record.count history records are declared newest first with distinct versions; V2 follows V2",
        ),
        (
            "#[history(removed_in = v2, backfill_fn = cast)]",
            "a removal carries no backfill; the previous field stays available to backfill functions",
        ),
        (
            "#[history(removed_in = v2, backfill_fn = contextual)]",
            "a removal carries no backfill; the previous field stays available to backfill functions",
        ),
    ] {
        fixture.write("src/lib.rs", &source(
            &format!(" {attributes} count: u32,"),
            "fn cast(previous_payload: &RecordV1) -> Result<u32, Infallible> { let value = previous_payload.count.clone(); Ok(value) } fn contextual(previous: &RecordV1) -> Result<u32, Infallible> { Ok(previous.count) }",
            "",
        ));
        for command in ["check", "build"] {
            assert_failure(
                &fixture.cargo(&[command, "--locked", "--offline"]),
                diagnostic,
            );
        }
    }
    fixture.write("src/lib.rs", &original);
    assert_success(&fixture.cargo(&["build", "--locked", "--offline"]));
}
