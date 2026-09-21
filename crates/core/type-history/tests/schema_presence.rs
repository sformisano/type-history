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

const NULLABLE: usize = 2;

#[derive(Debug, PartialEq, Serialize, Deserialize, Schema)]
pub struct PublicConstArray([u8; NULLABLE]);

#[derive(Debug, PartialEq, Serialize, Deserialize, Schema)]
struct PrivateRecord {
    value: u32,
}

#[derive(Debug, PartialEq, Serialize, Deserialize, Schema)]
pub struct PublicNewtype(PrivateRecord);

#[derive(Debug, PartialEq, Serialize, Deserialize, Schema)]
pub struct PublicTuple(PrivateRecord, Option<u32>);

#[derive(Debug, PartialEq, Serialize, Deserialize, Schema)]
pub struct PublicNullable(Option<PrivateRecord>);

#[derive(Debug, PartialEq, Serialize, Deserialize, Schema)]
pub struct PublicNested(PublicNullable);

#[derive(Debug, PartialEq, Serialize, Deserialize, Schema)]
struct EncapsulatedFields {
    required: PublicNullable,
    nested: PublicNested,
    optional: Option<PublicNewtype>,
    tuple: PublicTuple,
}

#[test]
fn public_tuple_structs_keep_private_fields_and_their_wire_contracts() {
    let array = json!([1, 2]);
    let value: PublicConstArray = serde_json::from_value(array.clone()).unwrap();
    assert_eq!(serde_json::to_value(value).unwrap(), array);
    assert_eq!(
        PublicConstArray::resolved_wire_schema(),
        <[u8; NULLABLE]>::resolved_wire_schema()
    );

    let record = json!({"value": 7});
    let newtype: PublicNewtype = serde_json::from_value(record.clone()).unwrap();
    assert_eq!(serde_json::to_value(newtype).unwrap(), record);
    let tuple = json!([record, null]);
    let pair: PublicTuple = serde_json::from_value(tuple.clone()).unwrap();
    assert_eq!(serde_json::to_value(pair).unwrap(), tuple);

    let value = json!({"required": null, "nested": null, "tuple": tuple});
    let fields: EncapsulatedFields = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(fields.required, PublicNullable(None));
    assert_eq!(fields.nested, PublicNested(PublicNullable(None)));
    assert_eq!(fields.optional, None);
    for required in ["required", "nested", "tuple"] {
        let mut missing = value.clone();
        missing.as_object_mut().unwrap().remove(required);
        assert!(serde_json::from_value::<EncapsulatedFields>(missing).is_err());
    }

    assert_eq!(
        PublicNewtype::resolved_wire_schema(),
        PrivateRecord::resolved_wire_schema()
    );
    assert_eq!(
        PublicTuple::resolved_wire_schema(),
        <(PrivateRecord, Option<u32>)>::resolved_wire_schema()
    );
    assert_eq!(
        PublicNested::resolved_wire_schema(),
        <Option<PrivateRecord>>::resolved_wire_schema()
    );
    let document = JsonSchemaDocument::from_export(
        export_json_schema::<EncapsulatedFields>(),
        "private-fields",
    )
    .unwrap();
    assert_eq!(document.shape(), EncapsulatedFields::resolved_wire_schema());
}
