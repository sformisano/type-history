use history_api::__private::{export_json_schema, schemars::JsonSchema};
use history_api::{versioned, ResolvedSchema, Schema};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Schema)]
#[serde(deny_unknown_fields)]
pub struct Nested { pub value: u32 }

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Schema)]
pub enum Choice { Unit, Newtype(u32), Tuple(u32, String), Named { value: String } }

#[derive(Serialize, Deserialize, Schema)]
#[serde(deny_unknown_fields)]
pub struct Record {
    pub nested: Nested,
    pub optional: Option<Nested>,
    pub vector: Vec<String>,
    pub array: [u32; 2],
    pub choice: Choice,
}

#[derive(Serialize, Deserialize, Schema)]
pub struct Empty {}

#[derive(Serialize, Deserialize, Schema)]
pub enum UnitOnly { Ready, Done }

#[versioned(stable_name = "diagnostics.supported")]
pub struct History { pub value: u32 }

fn describe<T: JsonSchema + ResolvedSchema>() -> Value {
    json!({ "schema": export_json_schema::<T>(), "name": T::schema_name(), "id": T::schema_id(), "wire": T::resolved_wire_schema() })
}

pub fn print_supported_output() {
    let record = Record {
        nested: Nested { value: 7 }, optional: None, vector: vec!["x".into()],
        array: [1, 2], choice: Choice::Tuple(3, "y".into()),
    };
    let bytes = serde_json::to_string(&record).unwrap();
    let decoded: Record = serde_json::from_str(&bytes).unwrap();
    assert_eq!(decoded.nested, record.nested);
    assert_eq!(decoded.choice, record.choice);
    let history_bytes = serde_json::to_string(&History { value: 9 }.into_versioned()).unwrap();
    println!("{}", json!({
        "record": describe::<Record>(), "nested": describe::<Nested>(), "choice": describe::<Choice>(),
        "empty": describe::<Empty>(), "unit": describe::<UnitOnly>(), "history": describe::<History>(),
        "bytes": bytes, "empty_bytes": serde_json::to_string(&Empty {}).unwrap(),
        "unit_bytes": serde_json::to_string(&UnitOnly::Ready).unwrap(), "history_bytes": history_bytes,
    }));
}
