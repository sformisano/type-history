//! Generated traits and schema helpers preserve authored field types and Self projections.

use super::support::{failure, success, Fixture, LEDGER};

#[test]
fn generated_traits_preserve_authored_field_types_and_values() {
    let fixture = Fixture::empty();
    success(&fixture.cli(&["init", "--package", "standalone-history-consumer"]));
    fixture.write(
        "src/lib.rs",
        r##"
#![deny(non_camel_case_types)]

type M = String;

#[history_api::versioned(stable_name = "example.field.alias")]
pub struct Record {
    pub value: M,
    pub count: u32,
}

#[test]
fn generated_records_use_ordinary_traits_and_preserve_decoder_errors() {
    use history_api::{decode, DecodeFailureKind, HasHistory, PayloadVersion, ReadError};
    use serde_json::Error as JsonError;
    use std::error::Error;

    let source = decode::<Record>(
        &Record::STABLE_NAME,
        PayloadVersion::INITIAL,
        br#"{"value":"INV-7","count":7}"#,
    ).unwrap();
    let copied = source.clone();
    assert_eq!(source, copied);
    assert_eq!(copied.value, "INV-7");
    assert_eq!(copied.count, 7);
    let text = format!("{source:?}");
    assert!(text.contains("value: \"INV-7\""));
    assert!(text.contains("count: 7"));

    let ReadError::Decode(error) = decode::<Record>(
        &Record::STABLE_NAME,
        PayloadVersion::INITIAL,
        br#"{"value":"INV-7","count":"wrong"}"#,
    ).unwrap_err() else { panic!("expected a decoder error") };
    assert_eq!(error.kind(), DecodeFailureKind::Decode);
    assert_eq!(error.source_version(), PayloadVersion::INITIAL);
    assert!(error.source().unwrap().is::<JsonError>());
}
"##,
    );
    success(&fixture.cargo(&["test", "--lib", "--locked", "--offline"]));
    success(&fixture.cli(&["freeze", "--package", "standalone-history-consumer"]));
    success(&fixture.cargo(&["build", "--locked", "--offline"]));
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
