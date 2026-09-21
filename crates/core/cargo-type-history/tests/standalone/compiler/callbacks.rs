use serde_json::Value;
use type_history_core::resolved::SchemaShape;

use super::super::support::{failure as assert_failure, success as assert_success};
use super::{fixture, ledger, source};

const PAYLOAD: &str = r#"
     #[history(removed_in = v2)] full_name: String,
     #[history(removed_in = v2)] suffix: String,
     #[history(updated_in = v2, previous_type = String, backfill_fn = amount)] amount: u32,
     #[history(added_in = v2, backfill_fn = first)] first: String,
     #[history(added_in = v2, backfill_fn = last)] last: String,
     #[history(added_in = v2, backfill_fn = combined)] combined: String,
"#;
const HELPERS: &str = r#"
use std::error::Error;
use std::fmt::Display;
use std::fmt::{Debug, Formatter, Result as FmtResult};
use std::sync::Mutex;
static CALLS: Mutex<Vec<&str>> = Mutex::new(Vec::new());
static FAIL: Mutex<&str> = Mutex::new("");
struct ConversionError;
impl Debug for ConversionError {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult { f.write_str("RAW_SECRET_ERROR") }
}
impl Display for ConversionError {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult { f.write_str("conversion failed") }
}
impl Error for ConversionError {}
fn record(name: &'static str) -> Result<(), ConversionError> {
    CALLS.lock().unwrap().push(name);
    if *FAIL.lock().unwrap() == name { Err(ConversionError) } else { Ok(()) }
}
fn first(previous: &RecordV1) -> Result<String, ConversionError> {
    record("first")?; Ok(previous.full_name.split_once(' ').unwrap().0.to_owned())
}
fn last(previous: &RecordV1) -> Result<String, ConversionError> {
    record("last")?; Ok(previous.full_name.split_once(' ').unwrap().1.to_owned())
}
fn combined(previous: &RecordV1) -> Result<String, ConversionError> {
    record("combined")?; Ok(format!("{} {} {}", previous.full_name, previous.suffix, previous.amount))
}
fn amount(previous: &RecordV1) -> Result<u32, ConversionError> {
    let value = &previous.amount;
    record("amount")?; Ok(value.parse().unwrap())
}
"#;

fn snapshots() -> Value {
    ledger(vec![
        vec![
            ("full_name", SchemaShape::String),
            ("suffix", SchemaShape::String),
            ("amount", SchemaShape::String),
        ],
        vec![
            ("amount", SchemaShape::U32),
            ("first", SchemaShape::String),
            ("last", SchemaShape::String),
            ("combined", SchemaShape::String),
        ],
    ])
}

#[test]
fn frozen_field_reordering_changes_callback_order_in_source_order() {
    let checks = r##"
        #[test] fn source_order_is_the_execution_order() {
            let current = Record::history()
                .decode(version(1), br#"{"full_name":"Ada Lovelace","suffix":"Jr","amount":"7"}"#).unwrap();
            assert_eq!(current.first, "Ada"); assert_eq!(current.last, "Lovelace");
            assert_eq!(current.combined, "Ada Lovelace Jr 7"); assert_eq!(current.amount, 7);
            assert_eq!(*CALLS.lock().unwrap(), vec!["amount", "first", "last", "combined"]);
        }
    "##;
    let fixture = fixture(&source(PAYLOAD, HELPERS, checks));
    fixture.set_ledger(&snapshots());
    let frozen = std::fs::read(fixture.root().join("type-history/schemas.json")).unwrap();
    assert_success(&fixture.cargo(&["test", "--lib", "--locked", "--offline"]));
    // Every backfill follows field declaration order.
    let reordered = r#"
         #[history(updated_in = v2, previous_type = String, backfill_fn = amount)] amount: u32,
         #[history(added_in = v2, backfill_fn = combined)] combined: String,
         #[history(added_in = v2, backfill_fn = last)] last: String,
         #[history(added_in = v2, backfill_fn = first)] first: String,
         #[history(removed_in = v2)] suffix: String,
         #[history(removed_in = v2)] full_name: String,
    "#;
    fixture.write(
        "src/lib.rs",
        &source(
            reordered,
            HELPERS,
            &checks.replace(
                "vec![\"amount\", \"first\", \"last\", \"combined\"]",
                "vec![\"amount\", \"combined\", \"last\", \"first\"]",
            ),
        ),
    );
    for command in ["check", "build"] {
        assert_success(&fixture.cargo(&[command, "--locked", "--offline"]));
    }
    assert_success(&fixture.cargo(&["test", "--lib", "--locked", "--offline"]));
    assert_eq!(
        frozen,
        std::fs::read(fixture.root().join("type-history/schemas.json")).unwrap()
    );
}

#[test]
fn history_callback_contracts() {
    let fixture = fixture(&source(PAYLOAD, HELPERS, ""));
    fixture.set_ledger(&snapshots());
    assert_success(&fixture.cargo(&["build", "--locked", "--offline"]));
    for (from, to, diagnostic) in [
        (
            "fn amount(previous: &RecordV1)",
            "fn amount(previous: &RecordV2)",
            "mismatched types",
        ),
        (
            "fn first(previous: &RecordV1)",
            "fn first(previous: &RecordV2)",
            "mismatched types",
        ),
        (
            "fn first(previous: &RecordV1)",
            "fn first(previous: RecordV1)",
            "mismatched types",
        ),
        (
            "fn first(previous: &RecordV1)",
            "async fn first(previous: &RecordV1)",
            "mismatched types",
        ),
        (
            "fn first(previous: &RecordV1)",
            "fn first(previous: &'static RecordV1)",
            "mismatched types",
        ),
    ] {
        let changed = HELPERS.replace(from, to);
        fixture.write("src/lib.rs", &source(PAYLOAD, &changed, ""));
        for command in ["check", "build"] {
            assert_failure(
                &fixture.cargo(&[command, "--locked", "--offline"]),
                diagnostic,
            );
        }
    }
    // A capturing closure is not a named synchronous callback path.
    fixture.write(
        "src/lib.rs",
        &source(
            &PAYLOAD.replace(
                "backfill_fn = first",
                "backfill_fn = |previous| first(previous)",
            ),
            HELPERS,
            "",
        ),
    );
    for command in ["check", "build"] {
        assert_failure(
            &fixture.cargo(&[command, "--locked", "--offline"]),
            "expected",
        );
    }
    for callback in [
        "fn wrong(previous: &RecordV1) -> Result<u64, ConversionError> { Ok(0) }",
        "fn wrong(previous: &RecordV1) -> Result<&str, ConversionError> { Ok(previous.full_name.as_str()) }",
    ] {
        fixture.write(
            "src/lib.rs",
            &source(
                &PAYLOAD.replace("backfill_fn = first", "backfill_fn = wrong"),
                &format!("{HELPERS}\n{callback}"),
                "",
            ),
        );
        for command in ["check", "build"] {
            assert_failure(
                &fixture.cargo(&[command, "--locked", "--offline"]),
                "mismatched types",
            );
        }
    }
    for (field_type, constructor, diagnostic) in [
        ("std::rc::Rc<()>", "std::rc::Rc::new(())", "cannot be sent"),
        (
            "std::cell::Cell<u32>",
            "std::cell::Cell::new(0)",
            "cannot be shared",
        ),
    ] {
        let helpers = HELPERS
            .replace(
                "struct ConversionError;",
                &format!("struct ConversionError({field_type});"),
            )
            .replace(
                "Err(ConversionError)",
                &format!("Err(ConversionError({constructor}))"),
            );
        fixture.write("src/lib.rs", &source(PAYLOAD, &helpers, ""));
        for command in ["check", "build"] {
            assert_failure(
                &fixture.cargo(&[command, "--locked", "--offline"]),
                diagnostic,
            );
        }
    }
}
