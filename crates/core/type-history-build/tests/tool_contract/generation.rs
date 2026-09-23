//! A generated consumer keeps custom observation configuration and public APIs.

use std::{collections::BTreeMap, fs, path::Path, process::Command};
use syn::parse_quote;
use type_history_codegen::{
    generate::{
        generate_history, generate_history_with_options, GenerationOptions, GenerationPaths,
    },
    history::HistoryPlan,
    json_schema::JsonSchemaDocument,
    ledger::{HistoryLedger, RecordMetadata, SchemaIdentity, Snapshot},
    NamedField, RecordDerives, RecordInput,
};
use type_history_core::resolved::{FieldPresence, SchemaField, SchemaShape};

const ID: &str = "custom.receipt";

fn generated(ledger: &HistoryLedger<RecordMetadata>, configured: bool, drift: bool) -> String {
    let input = RecordInput {
        name: parse_quote!(Receipt),
        visibility: parse_quote!(pub),
        attributes: vec![],
        derives: RecordDerives::default(),
        fields: vec![NamedField {
            name: parse_quote!(count),
            ty: if drift {
                parse_quote!(u64)
            } else {
                parse_quote!(u32)
            },
            attributes: vec![],
        }],
        field_visibility: BTreeMap::from([("count".into(), parse_quote!(pub))]),
    };
    let history = HistoryPlan { head: 1 }
        .expand_authorized(&input.name, &input.fields, ID, ledger, &RecordMetadata {})
        .unwrap();
    let paths = GenerationPaths {
        support: parse_quote!(type_history::__private),
        error: parse_quote!(type_history::DecodeError),
        helper_prefix: "custom_receipt".into(),
    };
    let result = if configured {
        generate_history_with_options(
            &input,
            ID,
            &history,
            &paths,
            &GenerationOptions {
                observation_cfg: Some(parse_quote!(custom_history_schema_export)),
                ..GenerationOptions::default()
            },
        )
    } else {
        generate_history(&input, ID, &history, &paths)
    }
    .unwrap();
    format!("{}\n{API_TEST}", result.items)
}

const BUILD: &str = r#"
use std::{env, path::PathBuf};
use history_build::{compile::compile_package, contract::{ToolContract, STANDALONE}, inventory::{Declaration, PackageInventory}};
use history_codegen::ledger::{HistoryReadiness, RecordMetadata};
fn main() {
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).canonicalize().unwrap();
    let contract = ToolContract { authority_kind: "custom-generator", strict_env: "CUSTOM_HISTORY_REQUIRE_FROZEN", export_env: "CUSTOM_HISTORY_SCHEMA_EXPORT", export_cfg: "custom_history_schema_export", ..STANDALONE };
    let inventory = PackageInventory {
        package: "custom-generator-consumer".into(), root: root.clone(),
        declarations: vec![Declaration { name: "Receipt".into(), stable_name: "custom.receipt".into(), metadata: RecordMetadata {}, version: 1, retained_versions: vec![1], readiness: HistoryReadiness::Frozen, source: None }],
        tracked_paths: vec![root.join("src/lib.rs"), root.join(contract.ledger_path)], declaration_count: 1,
    };
    compile_package(&root, &inventory, &contract).unwrap();
}
"#;

const API_TEST: &str = r##"
#[cfg(test)]
mod api_tests {
    use type_history::{DecodeContext, History, PayloadVersion, StableName};
    use type_history::__private::{canonical_json, ConstantShape, SchemaShape, WireNode, serde_json::json};
    struct DefaultNode;
    impl WireNode for DefaultNode { const SHAPE: ConstantShape = ConstantShape::U32; }
    struct OverrideNode;
    impl WireNode for OverrideNode {
        const SHAPE: ConstantShape = ConstantShape::U32;
        fn schema() -> SchemaShape { SchemaShape::U64 }
    }
    #[test]
    fn public_contracts_remain_available() {
        assert_eq!(DefaultNode::schema(), SchemaShape::U32);
        assert_eq!(OverrideNode::schema(), SchemaShape::U64);
        const NAME: StableName = StableName::new("custom.receipt");
        assert_eq!(NAME.as_static_str(), Some("custom.receipt"));
        let version = PayloadVersion::try_from_raw(1).unwrap();
        let history: History<u32, &'static str> = History::new(&[], |_, _| Err("custom"));
        assert_eq!(history.decode(version, b"{}"), Err("custom"));
        assert_eq!(DecodeContext::unsupported(NAME, version).source_version(), version);
        assert_eq!(canonical_json::value_to_vec(&json!({"z": 2, "a": 1})).unwrap(), br#"{"a":1,"z":2}"#);
    }
}
"##;

#[test]
fn custom_cfg_observes_drift_while_default_generation_stays_guarded() {
    let owner = tempfile::tempdir().unwrap();
    let root = owner.path();
    let build = Path::new(env!("CARGO_MANIFEST_DIR"));
    let runtime = build.parent().unwrap().join("type-history");
    let codegen = build.parent().unwrap().join("type-history-codegen");
    let target = build.ancestors().nth(3).unwrap().join("target");
    fs::create_dir(root.join("src")).unwrap();
    fs::create_dir(root.join("type-history")).unwrap();
    fs::write(root.join("Cargo.toml"), format!("[package]\nname='custom-generator-consumer'\nversion='0.0.0'\nedition='2024'\n[workspace]\n[dependencies]\ntype-history={{path={runtime:?}}}\n[build-dependencies]\nhistory_build={{package='type-history-build',path={build:?}}}\nhistory_codegen={{package='type-history-codegen',path={codegen:?}}}\n")).unwrap();
    fs::write(root.join("build.rs"), BUILD).unwrap();
    let identity = SchemaIdentity::new("urn:typehistory:schema:");
    let shape = SchemaShape::Record {
        fields: vec![SchemaField {
            name: "count".into(),
            presence: FieldPresence::Required,
            schema: SchemaShape::U32,
        }],
    };
    let ledger = HistoryLedger::from_entries(
        BTreeMap::from([(
            ID.into(),
            BTreeMap::from([(
                1,
                Snapshot {
                    metadata: RecordMetadata {},
                    schema: JsonSchemaDocument::from_shape(&shape, &identity.schema_id(ID)),
                    reset_draft: false,
                },
            )]),
        )]),
        identity,
    )
    .unwrap();
    let bytes = serde_json::to_string_pretty(ledger.entries()).unwrap();
    fs::write(root.join("type-history/schemas.json"), &bytes).unwrap();
    let run = |arguments: &[&str], export: bool| {
        let mut command = Command::new(env!("CARGO"));
        command
            .current_dir(root)
            .args(arguments)
            .env("CARGO_TARGET_DIR", &target)
            .env_remove("CUSTOM_HISTORY_REQUIRE_FROZEN")
            .env_remove("TYPE_HISTORY_REQUIRE_FROZEN")
            .env_remove("TYPE_HISTORY_SCHEMA_EXPORT")
            .env_remove("CUSTOM_HISTORY_SCHEMA_EXPORT");
        if export {
            command.env("CUSTOM_HISTORY_SCHEMA_EXPORT", "1");
        }
        command.output().unwrap()
    };
    fs::write(root.join("src/lib.rs"), generated(&ledger, true, true)).unwrap();
    let lock = run(&["generate-lockfile", "--offline"], false);
    assert!(
        lock.status.success(),
        "{}",
        String::from_utf8_lossy(&lock.stderr)
    );
    for (configured, drift, export, passed) in [
        (true, true, true, true),
        (true, true, false, false),
        (false, true, true, false),
        (false, false, false, true),
    ] {
        fs::write(
            root.join("src/lib.rs"),
            generated(&ledger, configured, drift),
        )
        .unwrap();
        let output = run(&["test", "--lib", "--locked", "--offline"], export);
        assert_eq!(
            output.status.success(),
            passed,
            "configured={configured} drift={drift} export={export}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        if !passed {
            assert!(String::from_utf8_lossy(&output.stderr).contains("frozen wire shape"));
        }
        assert_eq!(
            fs::read(root.join("type-history/schemas.json")).unwrap(),
            bytes.as_bytes()
        );
    }
}
