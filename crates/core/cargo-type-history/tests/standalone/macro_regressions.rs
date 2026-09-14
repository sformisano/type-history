use super::support::{failure, success, Fixture, LEDGER, V1, V2};
use std::fs;
use std::path::Path;

#[test]
fn caller_macros_cannot_replace_generated_builtins_or_frozen_checks() {
    let fixture = Fixture::frozen();
    let frozen = fixture.read(LEDGER);
    let source = format!(
        r#"#![allow(unused_macros)]
macro_rules! assert {{ ($($tokens:tt)*) => {{ () }}; }}
macro_rules! include_str {{ ($($tokens:tt)*) => {{ ::core::compile_error!("caller include_str") }}; }}
macro_rules! panic {{ ($($tokens:tt)*) => {{ ::core::compile_error!("caller panic") }}; }}
macro_rules! stringify {{ ($($tokens:tt)*) => {{ ::core::compile_error!("caller stringify") }}; }}
{V1}
#[derive(history_api::Schema)]
pub struct Details {{ pub value: u32 }}
"#
    );
    fixture.write("src/lib.rs", &source);
    for profile in [None, Some("--release")] {
        let mut arguments = vec!["build", "--locked", "--offline"];
        arguments.extend(profile);
        success(&fixture.cargo(&arguments));
    }
    fixture.write(
        "src/lib.rs",
        &source.replace("pub count: u32", "pub count: String"),
    );
    for profile in [None, Some("--release")] {
        let mut arguments = vec!["build", "--locked", "--offline"];
        arguments.extend(profile);
        failure(&fixture.cargo(&arguments), "changed its frozen wire shape");
        assert_eq!(fixture.read(LEDGER), frozen);
    }
}

#[test]
fn field_self_keeps_its_owner_in_schema_helpers_and_lint_aliases() {
    let fixture = Fixture::empty();
    success(&fixture.cli(&["init", "--package", "standalone-history-consumer"]));
    let source = r#"
use history_api::{versioned, Schema};
use serde::{Deserialize, Serialize};

pub trait ValueType { type Value; }

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Schema)]
#[serde(deny_unknown_fields)]
pub struct Details {
    pub direct: <Self as ValueType>::Value,
    #[allow(dead_code)]
    pub scoped: Option<Vec<<Self as ValueType>::Value>>,
}
impl ValueType for Details { type Value = u32; }

#[versioned(stable_name = "example.self.record")]
pub struct Record {
    pub details: Details,
    pub direct: <Self as ValueType>::Value,
    #[allow(dead_code)]
    pub converted: <Self as ValueType>::Value,
    #[allow(dead_code)]
    pub scoped: Option<<Self as ValueType>::Value>,
}
impl ValueType for Record { type Value = u32; }

#[test]
fn projections_preserve_their_concrete_wire_type() {
    use history_api::{decode, HasHistory, Versioned};
    let value = Record {
        details: Details { direct: 3, scoped: Some(vec![4, 5]) },
        direct: 6,
        converted: 8,
        scoped: Some(7),
    };
    let payload = serde_json::to_vec(&value).unwrap();
    let decoded = decode::<Record>(&Record::STABLE_NAME, Record::VERSION, &payload).unwrap();
    assert_eq!(decoded, value);
    let wire = serde_json::to_vec(&value.clone().into_versioned()).unwrap();
    let stored: Versioned<Record> = serde_json::from_slice(&wire).unwrap();
    assert_eq!(Record::from_versioned(stored).unwrap(), value);
}
"#;
    fixture.write("src/lib.rs", source);
    success(&fixture.cargo(&["test", "--lib", "--locked", "--offline"]));
    success(&fixture.cli(&["freeze", "--package", "standalone-history-consumer"]));
    let frozen = fixture.read(LEDGER);
    fixture.write(
        "src/lib.rs",
        &source.replace("type Value = u32;", "type Value = String;"),
    );
    failure(
        &fixture.cargo(&["build", "--locked", "--offline"]),
        "changed its frozen wire shape",
    );
    assert_eq!(fixture.read(LEDGER), frozen);

    let evolved = source
        .replace(
            "pub struct Record {",
            "pub struct Record {\n    #[history(added_in = v2, backfill_value = 9_u32)]\n    pub revision: u32,",
        )
        .replace(
            "pub converted: <Self as ValueType>::Value,",
            "#[history(updated_in = v2, previous_type = <Self as ValueType>::Value, backfill_fn = widen)]\n    pub converted: u64,",
        )
        .replace("let value = Record {", "let value = Record {\n        revision: 9,");
    let evolved = format!(
        r##"{evolved}
use std::convert::Infallible;

fn widen(previous: &RecordV1) -> Result<u64, Infallible> {{
    Ok(u64::from(previous.converted))
}}

#[test]
fn current_self_projections_still_decode_frozen_v1() {{
    use history_api::{{decode, HasHistory, PayloadVersion, Versioned}};
    let payload = br#"{{"details":{{"direct":3,"scoped":[4,5]}},"direct":6,"converted":8,"scoped":7}}"#;
    let decoded = decode::<Record>(&Record::STABLE_NAME, PayloadVersion::INITIAL, payload).unwrap();
    assert_eq!(decoded.revision, 9);
    assert_eq!(decoded.direct, 6);
    assert_eq!(decoded.converted, 8);
    assert_eq!(decoded.scoped, Some(7));
    let wire = br#"{{"stable_name":"example.self.record","version":1,"payload":{{"details":{{"direct":3,"scoped":[4,5]}},"direct":6,"converted":8,"scoped":7}}}}"#;
    let stored: Versioned<Record> = serde_json::from_slice(wire).unwrap();
    assert_eq!(stored.source_version().get(), 1);
    assert_eq!(Record::from_versioned(stored).unwrap(), decoded);
}}
"##
    );
    fixture.write("src/lib.rs", &evolved);
    success(&fixture.cli(&["freeze", "--package", "standalone-history-consumer"]));
    success(&fixture.cargo(&["test", "--lib", "--locked", "--offline"]));
    assert_eq!(
        fixture.ledger()["example.self.record"]
            .as_object()
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn current_alias_links_to_documented_public_fields_methods_and_traits() {
    let fixture = Fixture::frozen();
    let source = V2
        .replace(
            "pub struct Invoice",
            "/// Current invoice documentation.\npub struct Invoice",
        )
        .replace(
            "pub count: u64",
            "/// Number of billed units.\n    pub count: u64",
        );
    fixture.write("src/lib.rs", &source);
    success(&fixture.cargo_env(
        &["doc", "--no-deps", "--locked", "--offline"],
        &[("RUSTDOCFLAGS", Some("-Dwarnings"))],
    ));
    let docs = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("repository root")
        .join("target/doc/standalone_history_consumer");
    let alias = fs::read_to_string(docs.join("type.Invoice.html")).expect("current alias page");
    assert!(alias.contains("Current invoice documentation."));
    assert!(alias.contains("struct.InvoiceV2.html"));
    assert!(alias.contains("pub count:"));
    let current = fs::read_to_string(docs.join("struct.InvoiceV2.html"))
        .expect("current numbered record page");
    for visible in [
        "Current invoice documentation.",
        "Number of billed units.",
        "id=\"structfield.count\"",
        "id=\"method.from_versioned\"",
        "id=\"method.into_versioned\"",
        "id=\"trait-implementations\"",
    ] {
        assert!(current.contains(visible), "missing rendered API: {visible}");
    }
    let index = fs::read_to_string(docs.join("index.html")).expect("consumer documentation index");
    assert!(!index.contains("struct.InvoiceV1.html"));
}
