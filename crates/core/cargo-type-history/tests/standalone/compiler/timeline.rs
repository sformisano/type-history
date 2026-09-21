use serde_json::Value;
use type_history_core::resolved::SchemaShape;

use super::{ledger, run, source};

const PAYLOAD: &str = r#"

    #[history(updated_in = v3, previous_type = u16, backfill_fn = widen)]
    #[history(updated_in = v2, previous_type = String, backfill_fn = parse_count)]
    count: u32,

    #[history(removed_in = v2)]
    obsolete: String,

    #[history(added_in = v3, backfill_value = None)]
    note: Option<String>,
"#;
const HELPERS: &str = r#"
fn parse_count(previous_payload: &RecordV1) -> Result<u16, Infallible> { let value = previous_payload.count.clone(); Ok(value.parse().unwrap()) }
fn widen(previous_payload: &RecordV2) -> Result<u32, Infallible> { let value = previous_payload.count.clone(); Ok(u32::from(value)) }
"#;
fn snapshots() -> Value {
    ledger(vec![
        vec![
            ("count", SchemaShape::String),
            ("obsolete", SchemaShape::String),
        ],
        vec![("count", SchemaShape::U16)],
        vec![
            ("count", SchemaShape::U32),
            (
                "note",
                SchemaShape::Option {
                    value: Box::new(SchemaShape::String),
                },
            ),
        ],
    ])
}

#[test]
fn history_type_timeline() {
    run(
        &source(
            PAYLOAD,
            HELPERS,
            r##"
        #[test] fn wrong_version_shapes_are_rejected() {
            let history = Record::history();
            assert_eq!(history.retained_versions().iter().map(|v| v.get()).collect::<Vec<_>>(), vec![1,2,3]);
            for (v, bytes) in [(1, br#"{"count":"7","obsolete":"kept historically"}"#.as_slice()), (2, br#"{"count":7}"#), (3, br#"{"count":7,"note":null}"#)] {
                let current: RecordV3 = history.decode(version(v), bytes).unwrap();
                assert_eq!(current.count, 7);
                assert_eq!(current.note, None);
            }
            assert_eq!(serde_json::to_value(Record { count: 7, note: None }).unwrap(), serde_json::json!({"count":7,"note":null}));
            for bytes in [br#"{"count":7,"obsolete":"old"}"#.as_slice(), br#"{"count":"7"}"#, br#"{"count":"7","obsolete":"old","extra":1}"#, br#"{"count":"7","count":"8","obsolete":"old"}"#, br#"["7","old"]"#, b"null", b"true"] {
                assert!(history.decode(version(1), bytes).is_err(), "{:?}", bytes);
            }
            assert!(history.decode(version(2), br#"{"count":"7"}"#).is_err());
            assert!(history.decode(version(4), b"{}").is_err());
            // Omitted Option remains valid for its exact version.
            assert_eq!(history.decode(version(3), br#"{"count":7}"#).unwrap().note, None);
            assert!(history.decode(version(3), br#"[7,null]"#).is_err());
        }
    "##,
        ),
        &snapshots(),
    );
}

#[test]
fn history_boundaries() {
    let payload = r#" #[history(removed_in = v4)] #[history(updated_in = v2, previous_type = String, backfill_fn = normalize)] text: String,"#;
    run(
        &source(
            payload,
            "use std::sync::atomic::{AtomicUsize, Ordering}; static NORMALIZATIONS: AtomicUsize = AtomicUsize::new(0); fn normalize(previous_payload: &RecordV1) -> Result<String, Infallible> { let value = previous_payload.text.clone(); assert_eq!(value, \" abc \" ); NORMALIZATIONS.fetch_add(1, Ordering::SeqCst); Ok(value.trim().to_owned()) }",
            r##"
        #[test] fn removal_and_intermediate_identity() {
            use std::sync::atomic::Ordering;
            let history = Record::history();
            for v in [1,2,3] { let _: RecordV4 = history.decode(version(v), br#"{"text":" abc "}"#).unwrap(); }
            assert_eq!(NORMALIZATIONS.load(Ordering::SeqCst), 1);
            assert_eq!(serde_json::to_value(Record {}).unwrap(), serde_json::json!({}));
        }
    "##,
        ),
        &ledger(vec![
            vec![("text", SchemaShape::String)],
            vec![("text", SchemaShape::String)],
            vec![("text", SchemaShape::String)],
            vec![],
        ]),
    );
    run(
        &source(
            "",
            "",
            "#[test] fn empty() { assert_eq!(Record::VERSION.get(), 1); assert!(Record::history().decode(version(1), b\"{}\").is_ok()); }",
        ),
        &ledger(vec![vec![]]),
    );
}

#[test]
fn one_field_can_be_added_updated_repeatedly_and_removed() {
    run(
        &source(
            r#"
        #[history(removed_in = v5)]
        #[history(updated_in = v4, previous_type = u32, backfill_fn = widen)]
        #[history(updated_in = v3, previous_type = String, backfill_fn = parse)]
        #[history(added_in = v2, backfill_value = birth())]
        count: u64,"#,
            r#"
        use std::sync::Mutex;
        static CALLS: Mutex<Vec<&str>> = Mutex::new(Vec::new());
        fn birth() -> String { CALLS.lock().unwrap().push("birth"); "7".to_owned() }
        fn parse(previous_payload: &RecordV2) -> Result<u32, Infallible> { let value = previous_payload.count.clone(); CALLS.lock().unwrap().push("parse"); assert_eq!(value, "7"); Ok(7) }
        fn widen(previous_payload: &RecordV3) -> Result<u64, Infallible> { let value = previous_payload.count.clone(); CALLS.lock().unwrap().push("widen"); assert_eq!(value, 7); Ok(107) }
        "#,
            r##"#[test] fn complete_lifetime_keeps_exact_adjacent_steps() {
            let history = Record::history();
            for (v, bytes, expected) in [
                (1, b"{}".as_slice(), vec!["birth", "parse", "widen"]),
                (2, br#"{"count":"7"}"#, vec!["parse", "widen"]),
                (3, br#"{"count":7}"#, vec!["widen"]),
                (4, br#"{"count":107}"#, vec![]),
                (5, b"{}", vec![]),
            ] {
                CALLS.lock().unwrap().clear();
                let latest: RecordV5 = history.decode(version(v), bytes).unwrap();
                assert_eq!(*CALLS.lock().unwrap(), expected);
                assert_eq!(serde_json::to_value(latest).unwrap(), serde_json::json!({}));
            }
        }"##,
        ),
        &ledger(vec![
            vec![],
            vec![("count", SchemaShape::String)],
            vec![("count", SchemaShape::U32)],
            vec![("count", SchemaShape::U64)],
            vec![],
        ]),
    );
}
