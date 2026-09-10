use super::support::{failure, success, Fixture, LEDGER, V1, V2, V3};
use std::path::Path;
use toml::Value;

const PACKAGE: &str = "standalone-history-consumer";
const COMMON: &str = include_str!("fixtures/redacted-common.rs");
const RUNTIME: &str = include_str!("fixtures/redacted-tests.rs");

fn source(version: u32) -> String {
    let count = match version {
        1 => "#[sensitive] pub count: u32,",
        2 => "#[sensitive] #[history(updated_at = v2, previous_type = u32, cast_from_previous_value = widen)] pub count: u64,",
        _ => "#[sensitive] #[history(updated_at = v3, previous_type = u64, cast_from_previous_value = text)] #[history(updated_at = v2, previous_type = u32, cast_from_previous_value = widen)] pub count: String,",
    };
    let legacy = if version == 1 {
        "#[sensitive] pub legacy: String,"
    } else {
        "#[sensitive] #[history(removed_at = v2)] pub legacy: String,"
    };
    let label = if version == 1 {
        ""
    } else {
        "#[sensitive] #[history(added_in = v2, cast_from_previous_payload = label)] pub label: String,"
    };
    format!("{COMMON}\n#[history_api::versioned(stable_name = \"example.secure.record\", redacted)]\npub struct Secure {{\n{legacy}\n{count}\n{label}\n pub nested: Details,\n pub wrapped: SensitiveValue<String, WrapperSecret>,\n #[not_sensitive] pub public: String,\n}}\n")
}

#[test]
fn redacted_retained_records_errors_wire_and_policies() {
    let fixture = Fixture::configured(true);
    success(&fixture.cli(&["init", "--package", PACKAGE]));
    for version in 1..=3 {
        fixture.write("src/lib.rs", &source(version));
        success(&fixture.cargo(&["build", "--locked", "--offline"]));
        success(&fixture.cli(&["freeze", "--package", PACKAGE]));
    }
    fixture.write("src/lib.rs", &format!("{}{RUNTIME}", source(3)));
    success(&fixture.cargo(&["test", "--lib", "--locked", "--offline"]));
    success(&fixture.cargo(&["test", "--lib", "--release", "--locked", "--offline"]));
    success(&fixture.cli(&["check"]));
    let frozen = fixture.read(LEDGER);
    let original = source(3);
    let revised = original.replace(
        "#[sensitive] #[history(removed_at = v2)] pub legacy",
        "#[not_sensitive] #[history(removed_at = v2)] pub legacy",
    );
    assert_ne!(
        original, revised,
        "the policy edit must change the declaration"
    );
    fixture.write(
        "src/lib.rs",
        &format!("{revised}\n{}", include_str!("fixtures/policy-edit.rs")),
    );
    success(&fixture.cargo(&["test", "--lib", "--locked", "--offline"]));
    success(&fixture.cli(&["check"]));
    assert_eq!(fixture.read(LEDGER), frozen);
    fixture.write(
        "src/lib.rs",
        &source(3).replace(
            "#[sensitive] #[history(updated_at = v3",
            "#[sensitive(history_api::redactable::Email)] #[history(updated_at = v3",
        ),
    );
    failure(
        &fixture.cargo(&["build", "--locked", "--offline"]),
        "Policy",
    );
    assert_eq!(fixture.read(LEDGER), frozen);
}

#[test]
fn feature_enabled_ordinary_histories_keep_original_error_contracts() {
    let fixture = Fixture::configured(true);
    success(&fixture.cli(&["init", "--package", PACKAGE]));
    for source in [V1, V2] {
        fixture.write("src/lib.rs", source);
        success(&fixture.cli(&["freeze", "--package", PACKAGE]));
    }
    fixture.write("src/lib.rs", V3);
    success(&fixture.cargo(&["test", "--lib", "--locked", "--offline"]));
}

#[test]
fn invalid_redaction_declarations_fail_before_becoming_safe_output() {
    let fixture = Fixture::configured(true);
    success(&fixture.cli(&["init", "--package", PACKAGE]));
    for (field, expected) in [
        ("pub value: String,", "no declared redaction behavior"),
        (
            "#[sensitive] #[not_sensitive] pub value: String,",
            "exactly one",
        ),
        (
            "#[not_sensitive(secret)] pub value: String,",
            "use `#[sensitive]`",
        ),
        (
            "#[sensitive(UnknownPolicy)] pub value: String,",
            "UnknownPolicy",
        ),
        (
            "#[sensitive(history_api::redactable::Email)] pub value: u32,",
            "Policy",
        ),
    ] {
        fixture.write("src/lib.rs", &format!("#[history_api::versioned(stable_name = \"test.invalid.record\", redacted)] pub struct Invalid {{ {field} }}"));
        failure(
            &fixture.cargo(&["build", "--locked", "--offline"]),
            expected,
        );
    }
    fixture.write("src/lib.rs", "#[history_api::versioned(stable_name = \"test.invalid.record\")] pub struct Invalid { #[sensitive] pub value: String }");
    failure(
        &fixture.cargo(&["build", "--locked", "--offline"]),
        "field redaction requires",
    );
    fixture.write("src/lib.rs", "#[history_api::versioned(stable_name = \"test.invalid.record\", redacted)] pub struct Invalid { #[sensitive] pub value: String }");
    success(&fixture.cli(&["freeze", "--package", PACKAGE]));
    fixture.write(
        "src/lib.rs",
        r#"
use std::num::ParseIntError;
#[history_api::versioned(stable_name = "test.invalid.record", redacted)]
pub struct Invalid {
    #[sensitive]
    #[history(updated_at = v2, previous_type = String, cast_from_previous_value = convert)]
    pub value: u32,
}
fn convert(value: String) -> Result<u32, ParseIntError> { value.parse() }
"#,
    );
    failure(
        &fixture.cargo(&["build", "--locked", "--offline"]),
        "ToRedacted",
    );
}

#[test]
fn mixed_libraries_compile_documented_example_and_renamed_upstream_derives() {
    let fixture = Fixture::configured(true);
    let mut manifest: Value = toml::from_str(&fixture.read("Cargo.toml")).unwrap();
    manifest["workspace"]
        .as_table_mut()
        .unwrap()
        .insert("members".into(), Value::Array(vec!["plain".into()]));
    let runtime = manifest["dependencies"]["history_api"]["path"]
        .as_str()
        .unwrap();
    let build = manifest["build-dependencies"]["history_build"]["path"]
        .as_str()
        .unwrap();
    fixture.write(
        "plain/Cargo.toml",
        &format!(
            r#"
[package]
name = "plain-history-library"
version = "0.0.0"
edition = "2024"
publish = false
[dependencies]
history_api = {{ package = "type-history", path = {runtime:?} }}
serde_json = "1.0.149"
[build-dependencies]
history_build = {{ package = "type-history-build", path = {build:?} }}
"#
        ),
    );
    fixture.write("plain/build.rs", "fn main() { history_build::compile(); }");
    fixture.write("plain/type-history.toml", "format = 1\n");
    fixture.write("plain/src/lib.rs", "");
    manifest["dependencies"].as_table_mut().unwrap().insert(
        "plain-history-library".into(),
        toml::from_str::<Value>("path = 'plain'").unwrap(),
    );
    fixture.write("Cargo.toml", &toml::to_string(&manifest).unwrap());
    success(&fixture.cargo(&["generate-lockfile", "--offline"]));
    success(&fixture.cli(&["init", "--package", "plain-history-library"]));
    success(&fixture.cli(&["init", "--package", PACKAGE]));
    for source in [V1, V2] {
        fixture.write("plain/src/lib.rs", source);
        success(&fixture.cli(&["freeze", "--package", "plain-history-library"]));
    }
    fixture.write("plain/src/lib.rs", V3);
    let guide = include_str!("../../../../docs/redactable.md");
    let example = guide
        .split("```rust\n")
        .nth(1)
        .unwrap()
        .split("```")
        .next()
        .unwrap();
    fixture.write(
        "src/lib.rs",
        &example.replace("type_history::", "history_api::"),
    );
    success(&fixture.cli(&["freeze", "--package", PACKAGE]));
    success(&fixture.cli(&["freeze", "--package", "plain-history-library"]));
    success(&fixture.cargo(&["test", "--workspace", "--locked", "--offline"]));
    success(&fixture.cargo(&["build", "--workspace", "--release", "--locked", "--offline"]));
    manifest["dependencies"].as_table_mut().unwrap().insert("redaction_api".into(), toml::from_str::<Value>("package = 'redactable'\nversion = '=0.12.0'\ndefault-features = false\nfeatures = ['redaction']").unwrap());
    fixture.write("Cargo.toml", &toml::to_string(&manifest).unwrap());
    success(&fixture.cargo(&["generate-lockfile", "--offline"]));
    fixture.write(
        "src/lib.rs",
        &format!(
            "{}\n{}",
            example.replace("type_history::", "history_api::"),
            include_str!("fixtures/renamed.rs")
        ),
    );
    success(&fixture.cargo(&["test", "--workspace", "--locked", "--offline"]));
    success(&fixture.cli(&["freeze", "--package", PACKAGE]));
    success(&fixture.cli(&["check"]));
    let dependencies = manifest["dependencies"].as_table_mut().unwrap();
    let runtime = dependencies.remove("history_api").unwrap();
    dependencies.insert("type".into(), runtime);
    fixture.write("Cargo.toml", &toml::to_string(&manifest).unwrap());
    fixture.write(
        "src/lib.rs",
        &fixture
            .read("src/lib.rs")
            .replace("history_api::", "r#type::"),
    );
    success(&fixture.cargo(&["test", "--workspace", "--locked", "--offline"]));
    success(&fixture.cli(&["check"]));
}

#[test]
fn frontend_generation_preserves_concrete_axis_error_projection() {
    let fixture = Fixture::configured(true);
    let mut manifest: Value = toml::from_str(&fixture.read("Cargo.toml")).unwrap();
    let build_path = manifest["build-dependencies"]["history_build"]["path"]
        .as_str()
        .unwrap()
        .to_owned();
    let codegen = Path::new(&build_path)
        .parent()
        .unwrap()
        .join("type-history-codegen");
    let dependencies = manifest["build-dependencies"].as_table_mut().unwrap();
    dependencies.insert(
        "type-history-codegen".into(),
        toml::from_str::<Value>(&format!("path = {codegen:?}\nfeatures = ['redactable']")).unwrap(),
    );
    dependencies.insert(
        "syn".into(),
        toml::from_str::<Value>("version = '2'\nfeatures = ['full']").unwrap(),
    );
    fixture.write("Cargo.toml", &toml::to_string(&manifest).unwrap());
    fixture.write("build.rs", include_str!("fixtures/axis-build.rs"));
    fixture.write("src/lib.rs", include_str!("fixtures/axis-runtime.rs"));
    success(&fixture.cargo(&["generate-lockfile", "--offline"]));
    success(&fixture.cargo(&["test", "--lib", "--locked", "--offline"]));
    success(&fixture.cargo(&["test", "--lib", "--release", "--locked", "--offline"]));
}
