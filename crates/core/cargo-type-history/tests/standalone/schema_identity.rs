use super::support::{success, Fixture};

#[test]
fn module_distinct_records_and_enums_have_separate_definitions() {
    let fixture = Fixture::empty();
    success(&fixture.cli(&["init", "--package", "standalone-history-consumer"]));
    fixture.write("src/lib.rs", r#"
mod first {
    #[derive(history_api::Schema)] pub struct Details { pub first: u32 }
    #[derive(history_api::Schema)] pub enum Choice { First }
}
mod second {
    #[derive(history_api::Schema)] pub struct Details { pub second: String }
    #[derive(history_api::Schema)] pub enum Choice { Second }
}
#[test]
fn separate() {
    use crate::first::{Details as FirstDetails, Choice as FirstChoice};
    use crate::second::{Details as SecondDetails, Choice as SecondChoice};
    use history_api::__private::schemars::{JsonSchema, SchemaGenerator};
    assert_ne!(FirstDetails::schema_id(), SecondDetails::schema_id());
    assert_ne!(FirstChoice::schema_id(), SecondChoice::schema_id());
    assert_eq!(FirstDetails::schema_name(), SecondDetails::schema_name());
    let mut generator = SchemaGenerator::default();
    let a = generator.subschema_for::<FirstDetails>();
    let b = generator.subschema_for::<SecondDetails>();
    assert_ne!(a, b);
    let c = generator.subschema_for::<FirstChoice>();
    let d = generator.subschema_for::<SecondChoice>();
    assert_ne!(c, d);
    for (reference, expected) in [(a, "first"), (b, "second"), (c, "First"), (d, "Second")] {
        let key = reference.as_object().unwrap()["$ref"].as_str().unwrap().rsplit('/').next().unwrap();
        let definition = generator.definitions()[key].to_string();
        assert!(definition.contains(expected), "{definition}");
    }
    for document in [history_api::__private::export_json_schema::<FirstDetails>(), history_api::__private::export_json_schema::<SecondDetails>(), history_api::__private::export_json_schema::<FirstChoice>(), history_api::__private::export_json_schema::<SecondChoice>()] {
        assert!(!document.to_string().contains("$ref"));
    }
}
"#);
    success(&fixture.cargo(&["test", "--locked", "--offline"]));
}
