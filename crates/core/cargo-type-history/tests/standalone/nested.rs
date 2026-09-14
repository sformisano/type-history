use super::support::{success, Fixture, LEDGER};
use toml::Value;

const NESTED: &str = include_str!("fixtures/renamed.rs");

#[test]
fn nested_histories_survive_renamed_and_raw_identifier_facades() {
    let fixture = Fixture::empty();
    success(&fixture.cli(&["init", "--package", "standalone-history-consumer"]));
    fixture.write("src/lib.rs", NESTED);
    success(&fixture.cli(&["freeze", "--package", "standalone-history-consumer"]));
    success(&fixture.cargo(&["test", "--lib", "--locked", "--offline"]));
    let frozen = fixture.read(LEDGER);

    let mut manifest: Value = toml::from_str(&fixture.read("Cargo.toml")).unwrap();
    let dependencies = manifest["dependencies"].as_table_mut().unwrap();
    let runtime = dependencies.remove("history_api").unwrap();
    dependencies.insert("type".into(), runtime);
    fixture.write("Cargo.toml", &toml::to_string(&manifest).unwrap());
    let renamed = NESTED.replace("history_api::", "r#type::");
    assert_ne!(renamed, NESTED);
    fixture.write("src/lib.rs", &renamed);
    success(&fixture.cargo(&["generate-lockfile", "--offline"]));
    success(&fixture.cargo(&["test", "--lib", "--locked", "--offline"]));
    success(&fixture.cli(&["check", "--package", "standalone-history-consumer"]));
    assert_eq!(fixture.read(LEDGER), frozen);
}
