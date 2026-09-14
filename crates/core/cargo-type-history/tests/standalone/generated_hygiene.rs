use super::support::{success, Fixture};

#[test]
fn caller_constants_and_primitive_aliases_preserve_authored_types() {
    let fixture = Fixture::empty();
    success(&fixture.cli(&["init", "--package", "standalone-history-consumer"]));
    fixture.write("src/lib.rs", r#"
#![deny(warnings)]
#![forbid(non_snake_case, unused_imports)]
#![allow(non_upper_case_globals, non_camel_case_types, dead_code)]
use history_api::versioned;
const version: u32 = 1;
const record: u32 = 2;
const serializer: u32 = 3;
const formatter: u32 = 4;
const generator: u32 = 5;
const schema: u32 = 6;
const value: u32 = 7;
type u8 = String;
type str = String;
type usize = String;
#[versioned(stable_name="hygiene.record")]
/// Receipt schema description.
pub struct Receipt {
    /// Byte field description.
    pub byte: ::core::primitive::u8,
    pub alias: u8,
    pub text: str,
    pub count: usize,
}
#[derive(history_api::Schema)]
pub struct Details { pub byte: ::core::primitive::u8, pub alias: u8, pub optional: Option<String> }
#[derive(history_api::Schema)]
pub enum Choice { Unit, One(::core::primitive::u8), Pair(u8, str), Named { count: usize } }
#[test]
fn roundtrip() {
    let input = Receipt { byte: 8, alias: "a".to_owned(), text: "b".to_owned(), count: "c".to_owned() };
    let _: ::core::primitive::u8 = input.byte;
    let _: &String = &input.alias;
    let encoded = serde_json::to_value(input.clone().into_versioned()).unwrap();
    let restored = Receipt::from_versioned(serde_json::from_value(encoded).unwrap()).unwrap();
    assert_eq!(input, restored);
    let wire = history_api::__private::export_json_schema::<Receipt>();
    assert_eq!(wire["description"], "Receipt schema description.");
    assert_eq!(wire["properties"]["byte"]["description"], "Byte field description.");
    assert_eq!(wire["properties"]["alias"]["type"], "string");
    assert_eq!(wire["properties"]["byte"]["type"], "integer");
    let details = history_api::__private::export_json_schema::<Details>();
    assert_eq!(details["properties"]["optional"]["type"], serde_json::json!(["string", "null"]));
    assert!(details["required"].as_array().unwrap().iter().any(|name| name == "optional"));
    let _ = history_api::__private::export_json_schema::<Choice>();
}
"#);
    success(&fixture.cargo(&["test", "--locked", "--offline"]));
    success(&fixture.cli(&["freeze", "--package", "standalone-history-consumer"]));
    let evolved = fixture.read("src/lib.rs")
        .replace("pub count: usize,", "pub count: usize, #[history(added_in=v2, backfill_value=value + record)] pub added: u32,")
        .replace("count: \"c\".to_owned()", "count: \"c\".to_owned(), added: 9");
    fixture.write("src/lib.rs", &(evolved + r###"
#[test] fn historical_constants() {
    use history_api::Versioned;
    let stored = serde_json::from_str::<Versioned<Receipt>>(r##"{"stable_name":"hygiene.record","version":1,"payload":{"byte":8,"alias":"a","text":"b","count":"c"}}"##).unwrap();
    assert_eq!(Receipt::from_versioned(stored).unwrap().added, 9);
}
"###));
    success(&fixture.cargo(&["test", "--locked", "--offline"]));
    success(&fixture.cli(&["freeze", "--package", "standalone-history-consumer"]));
    success(&fixture.cargo_env(
        &["test", "--locked", "--offline"],
        &[("TYPE_HISTORY_REQUIRE_FROZEN", Some("1"))],
    ));
}
