use serde::{Deserialize, Serialize};
use serde_json::json;
use type_history::{JsonSchemaField, ResolvedSchema, Schema};
use type_history_codegen::json_schema::JsonSchemaDocument;
use type_history_core::resolved::{export_json_schema, Newtype, WireNode};
use type_history_core::schemars::{Schema as JsonSchema, SchemaGenerator};

#[derive(Serialize, Deserialize)]
struct Nullable(Option<u32>);

// An explicit declaration exercises the shared contract before tuple derives.
impl ResolvedSchema for Nullable {
    type Wire = Newtype<<Option<u32> as ResolvedSchema>::Wire>;
}

impl JsonSchemaField for Nullable {
    fn json_schema(generator: &mut SchemaGenerator) -> JsonSchema {
        <Option<u32> as JsonSchemaField>::json_schema(generator)
    }
}

#[derive(Serialize, Deserialize, Schema)]
struct OptionalRecord {
    value: Option<u32>,
}

#[derive(Serialize, Deserialize, Schema)]
struct RequiredRecord {
    value: Nullable,
}

#[test]
fn field_support_presence_matches_serde_and_survives_export() {
    let omitted: OptionalRecord = serde_json::from_str("{}").unwrap();
    assert!(omitted.value.is_none());
    assert!(serde_json::from_str::<RequiredRecord>("{}").is_err());
    let explicit: RequiredRecord = serde_json::from_str(r#"{"value":null}"#).unwrap();
    assert!(explicit.value.0.is_none());

    let optional_raw = export_json_schema::<OptionalRecord>();
    let required_raw = export_json_schema::<RequiredRecord>();
    assert_eq!(optional_raw["required"], json!([]));
    assert_eq!(required_raw["required"], json!(["value"]));
    assert_eq!(optional_raw["properties"], required_raw["properties"]);

    let optional = JsonSchemaDocument::from_export(optional_raw, "presence").unwrap();
    let required = JsonSchemaDocument::from_export(required_raw, "presence").unwrap();
    assert_eq!(optional.shape(), OptionalRecord::resolved_wire_schema());
    assert_eq!(required.shape(), RequiredRecord::resolved_wire_schema());
    assert_ne!(
        optional.canonical_bytes().unwrap(),
        required.canonical_bytes().unwrap()
    );
    assert!(!<OptionalRecord as ResolvedSchema>::Wire::SHAPE
        .same(&<RequiredRecord as ResolvedSchema>::Wire::SHAPE));

    for document in [optional, required] {
        let written = JsonSchemaDocument::from_shape(&document.shape(), "presence");
        assert_eq!(written, document);
        let bytes = serde_json::to_vec(&document).unwrap();
        let restored: JsonSchemaDocument = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(restored, document);
    }
}
