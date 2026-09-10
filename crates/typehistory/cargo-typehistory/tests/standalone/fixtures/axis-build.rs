use std::{env, fs, path::PathBuf};
use typehistory_codegen::{generate_history, GenerationPaths, HistoryPlan};
use typehistory_codegen::input;
use typehistory_codegen::ledger::{HistoryLedger, RecordMetadata, SchemaIdentity};
use syn::parse_quote;

fn main() {
    let (input, identity) = input::parse(
        parse_quote!(id = "fixture.axis.record", redacted),
        parse_quote! {
            pub struct AxisRecord {
                #[sensitive]
                #[history(updated_at = v2, previous_type = u32, cast_from_previous_value = convert)]
                pub value: String,
                #[not_sensitive]
                #[history(added_at = v2, cast_from_previous_payload = inspect)]
                pub inspected: bool,
            }
        },
    ).unwrap();
    let ledger = HistoryLedger::<RecordMetadata>::parse(br#"{
      "fixture.axis.record": { "1": { "metadata": {}, "schema": {
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "urn:typehistory:schema:fixture.axis.record",
        "type": "object", "additionalProperties": false,
        "properties": { "value": { "type": "integer", "format": "uint32", "minimum": 0 } },
        "required": ["value"]
      } } }
    }"#, SchemaIdentity::new("urn:typehistory:schema:")).unwrap();
    let plan = HistoryPlan::infer(&input.name, input.retained_floor, &input.fields).unwrap();
    let history = plan.expand_authorized(&input.name, &input.fields, &identity, &ledger, &RecordMetadata {}).unwrap();
    let generated = generate_history(&input, &identity, &history, &GenerationPaths {
        support: parse_quote!(history_api::__private),
        error: parse_quote!(FrontendError),
        helper_prefix: "frontend".to_owned(),
    }).unwrap();
    let path = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("history.rs");
    fs::write(path, generated.items.to_string()).unwrap();
}
