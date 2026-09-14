use super::support::{success, Fixture};

#[test]
fn packed_names_keep_exact_bytes_and_freeze_at_default_recursion_limit() {
    let fixture = Fixture::empty();
    success(&fixture.cli(&["init", "--package", "standalone-history-consumer"]));
    let names = [7, 8, 9, 127, 128, 129].map(|length| "a".repeat(length));
    let mut fields = names
        .iter()
        .map(|name| (name.clone(), name.clone()))
        .collect::<Vec<_>>();
    fields.extend([
        ("r#type".into(), "type".into()),
        ("abcdefé語".into(), "abcdefé語".into()),
    ]);
    let declaration = fields
        .iter()
        .map(|(name, _)| format!("pub {name}: u32,"))
        .collect::<String>();
    let values = fields
        .iter()
        .map(|(name, _)| format!("{name}: 3,"))
        .collect::<String>();
    let assertions = fields.iter().map(|(_, key)| format!("assert_eq!(json[{key:?}], 3); assert!(wire[\"properties\"].get({key:?}).is_some());")).collect::<String>();
    fixture.write("src/lib.rs", &format!(r#"
#[history_api::versioned(stable_name="names.record")]
pub struct Names {{ {declaration} }}
#[test] fn exact_names() {{
    let json = serde_json::to_value(Names {{ {values} }}).unwrap();
    let wire = history_api::__private::export_json_schema::<Names>();
    {assertions}
    use history_api::__private::{{NameByte, NameChunk, NameEnd, WireName}};
    type Old = NameByte<b'a', NameByte<b'b', NameByte<b'c', NameByte<b'd', NameByte<b'e', NameByte<b'f', NameByte<b'g', NameByte<b'h', NameEnd>>>>>>>>;
    type Packed = NameChunk<{{u64::from_be_bytes(*b"abcdefgh")}}, NameEnd>;
    assert_eq!(Old::value(), Packed::value());
    assert_eq!(Packed::value(), "abcdefgh");
}}
"#));
    success(&fixture.cargo(&["test", "--locked", "--offline"]));
    success(&fixture.cli(&["freeze", "--package", "standalone-history-consumer"]));
    success(&fixture.cli(&["check", "--package", "standalone-history-consumer"]));
    let ledger = fixture.read("type-history/schemas.json");
    for (_, key) in fields {
        assert!(ledger.contains(&key));
    }
}
