//! Public proof that a long retained history remains contiguous and decodable.

use super::support::{success, text, Fixture, LEDGER, STABLE_NAME};
use serde_json::{json, Map, Value};
use std::fmt::Write;
use type_history_codegen::json_schema::{record, JsonSchemaDocument};
use type_history_core::resolved::{FieldPresence, SchemaField, SchemaShape};

const PACKAGE: &str = "standalone-history-consumer";

#[test]
fn standalone_sixty_four_version_import_decodes_every_backfill() {
    let fixture = Fixture::empty();
    success(&fixture.cli(&["init", "--package", PACKAGE]));
    let source = sixty_four_version_source();
    fixture.write("src/lib.rs", &source);
    fixture.remove(LEDGER);
    let ledger = complete_history();
    let complete = format!("{}\n", serde_json::to_string_pretty(&ledger).unwrap());
    fixture.write("import.json", &complete);
    success(&fixture.cli(&["import", "--package", PACKAGE, "--from", "import.json"]));
    assert_eq!(fixture.ledger(), ledger);
    assert_eq!(fixture.read("import.json"), complete);
    let committed = fixture.read(LEDGER);

    let versions = ledger[STABLE_NAME].as_object().unwrap();
    assert_eq!(versions.len(), 64);
    for version in 1..=64 {
        assert!(versions.contains_key(&version.to_string()));
    }
    success(&fixture.cargo(&["check", "--locked", "--offline"]));
    assert_eq!(fixture.read(LEDGER), committed);
    let decoded = fixture.cargo(&[
        "test",
        "--lib",
        "--locked",
        "--offline",
        "checks::every_backfill_reaches_version_64",
        "--",
        "--exact",
    ]);
    success(&decoded);
    assert!(text(&decoded).contains("1 passed"));
    assert_eq!(fixture.read(LEDGER), committed);
    success(&fixture.cargo(&["build", "--release", "--locked", "--offline"]));
    assert_eq!(fixture.read(LEDGER), committed);
}

fn complete_history() -> Value {
    let identity = format!("urn:typehistory:schema:{STABLE_NAME}");
    let mut fields = Vec::new();
    let mut versions = Map::new();
    for version in 1..=64 {
        fields.push(SchemaField {
            name: format!("field_{version}"),
            presence: FieldPresence::Required,
            schema: SchemaShape::U32,
        });
        let shape = record(fields.clone());
        versions.insert(
            version.to_string(),
            json!({
                "metadata": {},
                "schema": JsonSchemaDocument::from_shape(&shape, &identity),
            }),
        );
    }
    json!({ STABLE_NAME: versions })
}

fn sixty_four_version_source() -> String {
    let mut fields = String::new();
    for version in 1..=64 {
        if version == 1 {
            writeln!(fields, "    pub field_1: u32,").unwrap();
        } else {
            writeln!(
                fields,
                "    #[history(added_in = v{version}, backfill_value = 0_u32)]\n    pub field_{version}: u32,"
            )
            .unwrap();
        }
    }
    format!(
        r##"use history_api::versioned;
#[versioned(stable_name = "{STABLE_NAME}")]
pub struct Invoice {{
{fields}}}

#[cfg(test)]
mod checks {{
    use super::Invoice;
    use history_api::{{HasHistory, PayloadVersion}};

    #[test]
    fn every_backfill_reaches_version_64() {{
        let history = Invoice::history();
        assert_eq!(history.retained_versions().len(), 64);
        let latest = history
            .decode(PayloadVersion::INITIAL, br#"{{"field_1":19}}"#)
            .unwrap();
        assert_eq!(latest.field_1, 19);
        assert_eq!(latest.field_2, 0);
        assert_eq!(latest.field_32, 0);
        assert_eq!(latest.field_64, 0);
        assert_eq!(Invoice::VERSION.get(), 64);
    }}
}}
"##
    )
}
