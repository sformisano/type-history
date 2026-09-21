//! Public compiler contracts shared by standalone histories and framework adapters.

#[path = "compiler/attributes.rs"]
mod attributes;
#[path = "compiler/callbacks.rs"]
mod callbacks;
#[path = "compiler/frozen_history.rs"]
mod frozen_history;
#[path = "compiler/grammar.rs"]
mod grammar;
#[path = "compiler/initializers.rs"]
mod initializers;
#[path = "compiler/schemas.rs"]
mod schemas;
#[path = "compiler/timeline.rs"]
mod timeline;

use super::support::{success, Fixture, STABLE_NAME};
use serde_json::{json, Map, Value};
use type_history_codegen::json_schema::{record, JsonSchemaDocument};
use type_history_core::resolved::{FieldPresence, SchemaField, SchemaShape};

fn source(fields: &str, helpers: &str, tests: &str) -> String {
    format!(
        r#"
use history_api::{{versioned, HasHistory}};
use std::convert::Infallible;
#[versioned(stable_name = "billing.invoice.issued")]
pub struct Record {{ {fields} }}
{helpers}
#[cfg(test)] mod checks {{
    use super::*;
    use history_api::PayloadVersion;
    fn version(n: u32) -> PayloadVersion {{ PayloadVersion::try_from_raw(n).unwrap() }}
    {tests}
}}
"#
    )
}

fn ledger(versions: Vec<Vec<(&str, SchemaShape)>>) -> Value {
    let versions: Map<String, Value> = versions.into_iter().enumerate().map(|(index, fields)| {
        let shape = record(fields.into_iter().map(|(name, schema)| SchemaField {
            name: name.to_owned(),
            presence: if matches!(schema, SchemaShape::Option { .. }) {
                FieldPresence::Optional
            } else {
                FieldPresence::Required
            },
            schema,
        }).collect());
        ((index + 1).to_string(), json!({
            "metadata": {},
            "schema": JsonSchemaDocument::from_shape(&shape, &format!("urn:typehistory:schema:{STABLE_NAME}"))
        }))
    }).collect();
    json!({ STABLE_NAME: versions })
}

fn fixture(source: &str) -> Fixture {
    let fixture = Fixture::empty();
    fixture.write("src/lib.rs", source);
    fixture
}

fn run(source: &str, ledger: &Value) {
    let fixture = fixture(source);
    fixture.set_ledger(ledger);
    success(&fixture.cargo(&["test", "--lib", "--locked", "--offline"]));
}
