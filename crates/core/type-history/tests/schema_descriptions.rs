use serde_json::Value;
use type_history::{ResolvedSchema, Schema};
use type_history_codegen::json_schema::JsonSchemaDocument;
use type_history_core::resolved::export_json_schema;

// No Serde codecs are needed to export a descriptive schema.
#[allow(dead_code)]
#[derive(Schema)]
struct Settlement {
    charge: Charge,
    status: Status,
    state: State,
    pair: Pair,
    reference: Reference,
}

/// Charge details.
#[allow(dead_code)]
#[derive(Schema)]
struct Charge {
    /// Amount paid in cents.
    cents: u32,
}

/// Posting status.
#[allow(dead_code)]
#[derive(Schema)]
enum Status {
    /// Posting is pending.
    Pending,
    /// Posting is complete.
    Complete,
}

/// Settlement state.
#[allow(dead_code)]
#[derive(Schema)]
enum State {
    /// The payment was posted.
    Posted,
    /// The payment is held.
    Held {
        /// Reason for the hold.
        reason: String,
    },
}

/// Range endpoints.
#[allow(dead_code)]
#[derive(Schema)]
struct Pair(
    /// Lower endpoint.
    i32,
    /// Upper endpoint.
    i32,
);

/// External reference.
#[allow(dead_code)]
#[derive(Schema)]
struct Reference(String);

#[test]
fn nested_descriptions_survive_exports_without_changing_frozen_contracts() {
    let schema = export_json_schema::<Settlement>();
    fn description<'a>(value: &'a Value, pointer: &str) -> &'a str {
        value
            .pointer(pointer)
            .and_then(Value::as_str)
            .unwrap_or_else(|| panic!("missing {pointer} in {value}"))
    }
    let props = &schema["properties"];
    assert_eq!(description(props, "/charge/description"), "Charge details.");
    assert_eq!(
        description(props, "/charge/properties/cents/description"),
        "Amount paid in cents."
    );
    assert_eq!(
        description(props, "/state/description"),
        "Settlement state."
    );
    let statuses = props["status"]["oneOf"]
        .as_array()
        .expect("documented unit variants");
    let pending = statuses.iter().find(|v| v["const"] == "Pending").unwrap();
    let complete = statuses.iter().find(|v| v["const"] == "Complete").unwrap();
    assert_eq!(description(pending, "/description"), "Posting is pending.");
    assert_eq!(
        description(complete, "/description"),
        "Posting is complete."
    );
    let variants = props["state"]["oneOf"]
        .as_array()
        .expect("externally tagged variants");
    let posted = variants
        .iter()
        .find(|v| v["const"] == "Posted")
        .expect("unit variant");
    let held = variants
        .iter()
        .find(|v| v["properties"].get("Held").is_some())
        .expect("named variant");
    assert_eq!(
        description(posted, "/description"),
        "The payment was posted."
    );
    assert_eq!(description(held, "/description"), "The payment is held.");
    assert_eq!(
        description(held, "/properties/Held/properties/reason/description"),
        "Reason for the hold."
    );
    assert_eq!(description(props, "/pair/description"), "Range endpoints.");
    assert_eq!(
        description(props, "/pair/prefixItems/0/description"),
        "Lower endpoint."
    );
    assert_eq!(
        description(props, "/pair/prefixItems/1/description"),
        "Upper endpoint."
    );
    assert_eq!(
        description(props, "/reference/description"),
        "External reference."
    );
    let normalized = JsonSchemaDocument::from_export(schema, "settlement").unwrap();
    assert_eq!(
        normalized,
        JsonSchemaDocument::from_shape(&Settlement::resolved_wire_schema(), "settlement")
    );
}
