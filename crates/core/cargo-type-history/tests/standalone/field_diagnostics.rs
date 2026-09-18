use super::support::{success, text, Fixture};
use serde_json::Value;

const SUPPORT: &str = r#"
use history_api::{versioned, Schema};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Undeclared { pub value: u32 }
"#;

#[test]
fn unsupported_fields_report_missing_support_at_the_authored_type() {
    let fixture = Fixture::empty();
    success(&fixture.cli(&["init", "--package", "standalone-history-consumer"]));
    let mut failures = Vec::new();
    for (kind, ty, displayed) in [
        ("plain", "Undeclared", "Undeclared"),
        ("map", "HashMap<u32, u32>", "HashMap<u32, u32>"),
        ("float", "f64", "f64"),
    ] {
        for (path, declaration) in [
            ("record", format!("#[derive(Schema)]\npub struct Record {{\n    pub value: {ty},\n}}")),
            ("enum", format!("#[derive(Schema)]\npub enum Choice {{\n    Value {{ value: {ty} }},\n}}")),
            ("versioned", format!("#[versioned(stable_name = \"diagnostics.record\")]\npub struct Record {{\n    pub value: {ty},\n}}")),
        ] {
            check_case(&fixture, &format!("{path}-{kind}"), &declaration, ty, displayed, &mut failures);
        }
    }
    for (case, declaration) in [
        ("lint-record", "#[derive(Schema)]\npub struct Record {\n    #[allow(deprecated)]\n    pub value: f64,\n}"),
        ("lint-enum-named", "#[derive(Schema)]\npub enum Choice {\n    Value { #[warn(deprecated)] value: f64 },\n}"),
        ("lint-enum-newtype", "#[derive(Schema)]\npub enum Choice {\n    Value(#[deny(deprecated)] f64),\n}"),
        ("lint-enum-tuple-inherited", "#[derive(Schema)]\npub enum Choice {\n    #[allow(deprecated)]\n    Value(String, f64),\n}"),
        ("lint-versioned", "#[versioned(stable_name = \"diagnostics.record\")]\npub struct Record {\n    #[deny(deprecated)]\n    pub value: f64,\n}"),
    ] {
        check_case(&fixture, case, declaration, "f64", "f64", &mut failures);
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

fn check_case(
    fixture: &Fixture,
    case: &str,
    declaration: &str,
    ty: &str,
    displayed: &str,
    failures: &mut Vec<String>,
) {
    let source = format!("{SUPPORT}\n{declaration}\n");
    let start = source.rfind(ty).expect("authored field type");
    let line = source[..start]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
        + 1;
    let column = start - source[..start].rfind('\n').expect("field line");
    fixture.write("src/lib.rs", &source);
    let output = fixture.cargo(&["check", "--locked", "--offline", "--message-format=json"]);
    println!(
        "FIELD-DIAGNOSTICS-BEGIN {case}\n{}\nFIELD-DIAGNOSTICS-END {case}",
        text(&output)
    );
    if output.status.success() {
        failures.push(format!("{case}: unsupported field compiled"));
    }
    let diagnostics = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|value| value["reason"] == "compiler-message")
        .map(|value| value["message"].clone())
        .collect::<Vec<_>>();
    let rendered = diagnostics
        .iter()
        .filter_map(|value| value["rendered"].as_str())
        .collect::<String>();
    if rendered.contains("consider manually implementing") {
        failures.push(format!("{case}: misleading implementation advice"));
    }
    let expected = format!("`{displayed}` is not supported as a persisted field");
    for label in [
        "this type has no supported structural wire schema",
        "this type has no supported JSON Schema field adapter",
    ] {
        let matches = diagnostics
            .iter()
            .filter(|value| value["level"] == "error" && value["message"] == expected)
            .collect::<Vec<_>>();
        let located = matches.iter().any(|value| {
            value["spans"]
                .as_array()
                .expect("diagnostic spans")
                .iter()
                .any(|span| {
                    span["is_primary"] == true
                        && span["file_name"] == "src/lib.rs"
                        && span["line_start"] == line
                        && span["column_start"] == column
                        && span["label"] == label
                })
        });
        if !located {
            failures.push(format!(
                "{case}: missing {expected:?} / {label:?} at src/lib.rs:{line}:{column}"
            ));
        }
    }
    for guidance in [
        "derive `type_history::Schema` or use your framework's schema declaration",
        "choose a supported serialized representation",
        "wrapping an unsupported field alone is insufficient",
    ] {
        if !rendered.contains(guidance) {
            failures.push(format!("{case}: missing guidance {guidance:?}"));
        }
    }
}

#[test]
fn supported_fields_keep_schema_identity_and_serialized_values() {
    let fixture = Fixture::empty();
    success(&fixture.cli(&["init", "--package", "standalone-history-consumer"]));
    fixture.write(
        "src/lib.rs",
        include_str!("fixtures/field-diagnostics-supported.rs"),
    );
    fixture.write(
        "src/main.rs",
        "fn main() { standalone_history_consumer::print_supported_output(); }\n",
    );
    let output = fixture.cargo(&["run", "--locked", "--offline", "--quiet"]);
    success(&output);
    let value: Value = serde_json::from_slice(&output.stdout).expect("supported output");
    assert_eq!(
        value["bytes"],
        r#"{"nested":{"value":7},"optional":null,"vector":["x"],"array":[1,2],"choice":{"Tuple":[3,"y"]}}"#
    );
    assert_eq!(value["empty_bytes"], "{}");
    assert_eq!(value["unit_bytes"], "\"Ready\"");
    assert_eq!(
        value["history_bytes"],
        r#"{"stable_name":"diagnostics.supported","version":1,"payload":{"value":9}}"#
    );
    println!("FIELD-COMPATIBILITY {value}");
}
