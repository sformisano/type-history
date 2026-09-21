use serde_json::Value;
use type_history_core::resolved::SchemaShape;

use super::super::support::{failure as assert_failure, success as assert_success};
use super::{fixture, ledger, source};

const PAYLOAD: &str = r#"

    #[history(added_in = v2, backfill_value = None)]
    note: Option<String>,

    #[history(added_in = v2, backfill_value = String::default())]
    label: String,

    #[history(updated_in = v3, previous_type = u16, backfill_fn = widen)]
    #[history(added_in = v2, backfill_value = 7_u16)]
    count: u32,
"#;
const HELPERS: &str = "fn widen(previous_payload: &RecordV2) -> Result<u32, Infallible> { let value = previous_payload.count.clone(); Ok(u32::from(value)) }";

fn snapshots() -> Value {
    ledger(vec![
        vec![],
        vec![
            (
                "note",
                SchemaShape::Option {
                    value: Box::new(SchemaShape::String),
                },
            ),
            ("label", SchemaShape::String),
            ("count", SchemaShape::U16),
        ],
        vec![
            (
                "note",
                SchemaShape::Option {
                    value: Box::new(SchemaShape::String),
                },
            ),
            ("label", SchemaShape::String),
            ("count", SchemaShape::U32),
        ],
    ])
}

#[test]
fn history_initializers() {
    let checks = r#"#[test] fn initialize_at_birth_type() {
        let decoded = Record::history().decode(version(1), b"{}").unwrap();
        assert_eq!(decoded.note, None); assert_eq!(decoded.label, ""); assert_eq!(decoded.count, 7);
    }"#;
    let fixture = fixture(&source(PAYLOAD, HELPERS, checks));
    fixture.set_ledger(&snapshots());
    assert_success(&fixture.cargo(&["test", "--lib", "--locked", "--offline"]));
    for (from, to, diagnostic) in [
        (", backfill_value = None", "", "requires exactly one"),
        (
            ", backfill_value = String::default()",
            "",
            "requires exactly one",
        ),
        (
            "backfill_value = 7_u16",
            "backfill_value = 7_u32",
            "mismatched types",
        ),
        (
            "backfill_value = String::default()",
            "backfill_value = { return Err(()); }",
            "mismatched types",
        ),
        (
            "backfill_value = String::default()",
            "backfill_value = { Ok::<String, Infallible>(String::new())? }",
            "?",
        ),
        (
            "backfill_fn = widen",
            "backfill_fn = misspelled",
            "misspelled",
        ),
        ("previous_type = u16, ", "", "requires `previous_type"),
        (", backfill_fn = widen", "", "requires exactly one"),
    ] {
        assert!(PAYLOAD.contains(from));
        fixture.write(
            "src/lib.rs",
            &source(&PAYLOAD.replace(from, to), HELPERS, ""),
        );
        for command in ["check", "build"] {
            assert_failure(
                &fixture.cargo(&[command, "--locked", "--offline"]),
                diagnostic,
            );
        }
    }
    fixture.write(
        "src/lib.rs",
        &source(
            &PAYLOAD.replace("String::default()", "{ return String::new(); }"),
            HELPERS,
            checks,
        ),
    );
    assert_success(&fixture.cargo(&["test", "--lib", "--locked", "--offline"]));
}

#[test]
fn optional_only_addition_uses_its_historical_value() {
    let payload = r#" #[history(added_in = v2, backfill_value = Some("historical".to_owned()))] note: Option<String>,"#;
    let fixture = fixture(&source(
        payload,
        "",
        r#"
        #[test] fn distinguish_exact_history_from_current_optional_decoding() {
            let history = Record::history();
            let old = history.decode(version(1), b"{}").unwrap();
            let current = history.decode(version(2), b"{}").unwrap();
            assert_eq!(old.note.as_deref(), Some("historical"));
            assert_eq!(current.note, None);
        }
    "#,
    ));
    fixture.set_ledger(&ledger(vec![
        vec![],
        vec![(
            "note",
            SchemaShape::Option {
                value: Box::new(SchemaShape::String),
            },
        )],
    ]));
    assert_success(&fixture.cargo(&["test", "--lib", "--locked", "--offline"]));
    fixture.write(
        "src/lib.rs",
        &source(payload, "fn current_producer() -> Record { Record {} }", ""),
    );
    for command in ["check", "build"] {
        assert_failure(
            &fixture.cargo(&[command, "--locked", "--offline"]),
            "missing field `note`",
        );
    }
}
