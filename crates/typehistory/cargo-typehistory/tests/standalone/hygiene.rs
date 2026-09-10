use super::support::{failure, success, Fixture};

#[test]
fn mapper_parameters_preserve_authored_types_and_policies() {
    let fixture = Fixture::empty();
    success(&fixture.cli(&["init", "--package", "standalone-history-consumer"]));
    fixture.write(
        "src/lib.rs",
        r##"
#![deny(non_camel_case_types)]

pub mod field_alias {
    type M = String;

    #[history_api::history(id = "example.field.alias")]
    pub struct Record {
        #[sensitive(redactable::Secret)]
        pub value: M,
        #[not_sensitive]
        pub public: u32,
    }
}

pub mod policy_alias {
    use redactable::Secret as M;

    #[history_api::history(id = "example.policy.alias")]
    pub struct Record {
        #[sensitive(M)]
        pub value: String,
    }
}

#[test]
fn aliased_fields_and_policies_still_redact() {
    use history_api::{decode, HasHistory, PayloadVersion};
    use redactable::{Redactable, ToRedacted};

    let source = decode::<field_alias::Record>(
        &field_alias::Record::TYPE,
        PayloadVersion::INITIAL,
        br#"{"value":"FIELD_SECRET","public":7}"#,
    ).unwrap();
    assert_eq!(source.value, "FIELD_SECRET");
    let safe = source.clone().redact();
    assert_ne!(safe.value, source.value);
    assert_eq!(safe.public, 7);
    assert!(!format!("{source:?}").contains("FIELD_SECRET"));
    assert!(!source.to_redacted().text().contains("FIELD_SECRET"));

    let policy = policy_alias::Record { value: "POLICY_SECRET".to_owned() }.redact();
    assert_ne!(policy.value, "POLICY_SECRET");
}
"##,
    );
    success(&fixture.cargo(&["test", "--lib", "--locked", "--offline"]));
    success(&fixture.cli(&["freeze", "--package", "standalone-history-consumer"]));
    success(&fixture.cargo(&["build", "--locked", "--offline"]));
}

#[test]
fn generated_records_and_schema_helpers_keep_authored_lint_scope() {
    let fixture = Fixture::empty();
    success(&fixture.cli(&["init", "--package", "standalone-history-consumer"]));
    fixture.write("src/lib.rs", LINT_SOURCE);
    success(&fixture.cargo(&["test", "--lib", "--locked", "--offline"]));
    success(&fixture.cli(&["freeze", "--package", "standalone-history-consumer"]));

    // Rust checks field naming at the record's scope: a field-local allowance
    // alone does not override a denying crate lint, even without a macro.
    for record in ["FieldAllowance", "SchemaFieldAllowance"] {
        let source = LINT_SOURCE.replace(
            &format!("#[allow(non_snake_case)]\npub struct {record}"),
            &format!("pub struct {record}"),
        );
        fixture.write("src/lib.rs", &source);
        failure(
            &fixture.cargo(&["build", "--locked", "--offline"]),
            "camelCase",
        );
    }

    for attributes in [
        "#[allow(non_snake_case)]\n#[deny(non_snake_case)]",
        "#[warn(non_snake_case)]",
    ] {
        let source = LINT_SOURCE.replacen("#[allow(non_snake_case)]", attributes, 1);
        fixture.write("src/lib.rs", &source);
        failure(
            &fixture.cargo(&["build", "--locked", "--offline"]),
            "camelCase",
        );
    }

    let transition_source = LINT_SOURCE.replacen(
        "pub camelCase: u32,",
        "pub camelCase: u32,\n    #[not_sensitive]\n    #[history(added_at = v2, backfill_value = INITIAL_REVISION)]\n    pub revision: u32,",
        1,
    );
    fixture.write("src/lib.rs", &transition_source);
    success(&fixture.cargo(&["test", "--lib", "--locked", "--offline"]));
    success(&fixture.cli(&["freeze", "--package", "standalone-history-consumer"]));
    fixture.write(
        "src/lib.rs",
        &transition_source.replace("#[allow(deprecated)]\n", ""),
    );
    failure(
        &fixture.cargo(&["build", "--locked", "--offline"]),
        "INITIAL_REVISION",
    );
}

#[test]
fn deprecated_field_types_keep_their_field_allowances() {
    let fixture = Fixture::empty();
    success(&fixture.cli(&["init", "--package", "standalone-history-consumer"]));
    fixture.write("src/lib.rs", DEPRECATED_SOURCE);
    success(&fixture.cargo(&["test", "--lib", "--locked", "--offline"]));
    success(&fixture.cli(&["freeze", "--package", "standalone-history-consumer"]));
    success(&fixture.cargo(&["build", "--locked", "--offline"]));

    // One field's allowance must not relax a neighboring field's explicit deny,
    // either in a generated history or in a supporting schema record.
    for field in [
        "#[allow(deprecated)]\n    #[not_sensitive]\n    pub second: Old",
        "#[allow(deprecated)]\n    pub second: Old",
    ] {
        let denied = field.replace("allow(deprecated)", "deny(deprecated)");
        fixture.write("src/lib.rs", &DEPRECATED_SOURCE.replace(field, &denied));
        failure(
            &fixture.cargo(&["build", "--locked", "--offline"]),
            "deprecated type alias `Old`",
        );
    }

    let backfill = "    #[allow(deprecated)]\n    #[not_sensitive]\n    #[history(added_at = v2, backfill_value = INITIAL_REVISION)]\n    pub revision: u32,";
    let callback = "    #[allow(deprecated)]\n    #[not_sensitive]\n    #[history(added_at = v2, cast_from_previous_payload = birth)]\n    pub copied: u32,";
    let updated = DEPRECATED_SOURCE.replacen(
        "pub value: Old,",
        "#[history(updated_at = v2, previous_type = Old, cast_from_previous_value = update)]\n    pub value: Old,",
        1,
    ).replacen(
        "pub second: Old,\n}",
        &format!("pub second: Old,\n{backfill}\n{callback}\n}}"),
        1,
    );
    fixture.write("src/lib.rs", &updated);
    success(&fixture.cargo(&["test", "--lib", "--locked", "--offline"]));

    for (field, diagnostic) in [
        (backfill, "deprecated constant `INITIAL_REVISION`"),
        (callback, "deprecated function `birth`"),
    ] {
        let denied = field.replace("allow(deprecated)", "deny(deprecated)");
        fixture.write("src/lib.rs", &updated.replace(field, &denied));
        failure(
            &fixture.cargo(&["build", "--locked", "--offline"]),
            diagnostic,
        );
    }

    fixture.write("src/lib.rs", &updated);
    success(&fixture.cli(&["freeze", "--package", "standalone-history-consumer"]));
    success(&fixture.cargo(&["test", "--lib", "--locked", "--offline"]));
}

const DEPRECATED_SOURCE: &str = r##"
#![deny(deprecated)]

use redactable::{BypassTextRedaction, RedactedValue, ToRedacted};

#[deprecated]
type Old = String;

#[history_api::history(id = "example.deprecated.field")]
pub struct Record {
    #[allow(deprecated)]
    #[not_sensitive]
    pub value: Old,
    #[allow(deprecated)]
    #[not_sensitive]
    pub second: Old,
}

#[derive(history_api::Schema)]
pub struct Supporting {
    #[allow(deprecated)]
    pub value: Old,
    #[allow(deprecated)]
    pub second: Old,
}

#[derive(Debug)]
pub struct ConvertError;

impl ToRedacted for ConvertError {
    fn to_redacted(&self) -> RedactedValue {
        BypassTextRedaction("conversion failed".to_owned()).to_redacted()
    }
}

#[deprecated]
pub fn update(value: String) -> Result<String, ConvertError> {
    Ok(format!("{value}!"))
}

#[deprecated]
pub const INITIAL_REVISION: u32 = 1;

#[deprecated]
pub fn birth(_: &RecordV1) -> Result<u32, ConvertError> {
    Ok(2)
}

#[test]
fn aliases_keep_public_types_and_frozen_shapes() {
    use history_api::{decode, HasHistory, PayloadVersion, ResolvedSchema};

    let value = decode::<Record>(
        &Record::TYPE,
        PayloadVersion::INITIAL,
        br#"{"value":"first","second":"second"}"#,
    ).unwrap();
    let expected = match Record::VERSION.get() {
        1 => serde_json::json!({"value":"first", "second":"second"}),
        2 => serde_json::json!({"value":"first!", "second":"second", "revision":1, "copied":2}),
        version => panic!("unexpected version {version}"),
    };
    assert_eq!(serde_json::to_value(&value).unwrap(), expected);
    let _: String = value.value;
    assert_eq!(RecordV1::resolved_wire_schema(), Supporting::resolved_wire_schema());
}
"##;

const LINT_SOURCE: &str = r##"
#![deny(warnings)]
#![deny(non_snake_case)]
#![deny(deprecated)]

#[deprecated]
pub const INITIAL_REVISION: u32 = 1;

#[history_api::history(id = "example.container.lints")]
#[allow(non_snake_case)]
#[allow(deprecated)]
pub struct ContainerAllowance {
    #[not_sensitive]
    pub camelCase: u32,
}

#[history_api::history(id = "example.field.lints")]
#[allow(non_snake_case)]
pub struct FieldAllowance {
    #[allow(non_snake_case)]
    #[not_sensitive]
    pub camelCase: u32,
}

#[derive(history_api::Schema)]
#[allow(non_snake_case)]
pub struct SchemaContainerAllowance {
    pub camelCase: u32,
}

#[derive(history_api::Schema)]
#[allow(non_snake_case)]
pub struct SchemaFieldAllowance {
    #[allow(non_snake_case)]
    pub camelCase: u32,
}

#[test]
fn allowed_fields_keep_their_wire_names() {
    use history_api::{decode, HasHistory, PayloadVersion, ResolvedSchema};

    let first = decode::<ContainerAllowance>(
        &ContainerAllowance::TYPE,
        PayloadVersion::INITIAL,
        br#"{"camelCase":7}"#,
    ).unwrap();
    let second = decode::<FieldAllowance>(
        &FieldAllowance::TYPE,
        PayloadVersion::INITIAL,
        br#"{"camelCase":8}"#,
    ).unwrap();
    assert_eq!(first.camelCase, 7);
    assert_eq!(second.camelCase, 8);
    let expected = match ContainerAllowance::VERSION.get() {
        1 => serde_json::json!({"camelCase": 7}),
        2 => serde_json::json!({"camelCase": 7, "revision": 1}),
        version => panic!("unexpected version {version}"),
    };
    assert_eq!(serde_json::to_value(&first).unwrap(), expected);
    assert_eq!(
        SchemaContainerAllowance::resolved_wire_schema(),
        SchemaFieldAllowance::resolved_wire_schema(),
    );
}
"##;
