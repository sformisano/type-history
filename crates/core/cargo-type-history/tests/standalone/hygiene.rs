use super::support::{failure, success, Fixture};

#[test]
fn generated_traits_preserve_authored_field_types_and_values() {
    let fixture = Fixture::empty();
    success(&fixture.cli(&["init", "--package", "standalone-history-consumer"]));
    fixture.write(
        "src/lib.rs",
        r##"
#![deny(non_camel_case_types)]

type M = String;

#[history_api::versioned(stable_name = "example.field.alias")]
pub struct Record {
    pub value: M,
    pub count: u32,
}

#[test]
fn generated_records_use_ordinary_traits_and_preserve_decoder_errors() {
    use history_api::{decode, DecodeFailureKind, HasHistory, PayloadVersion, ReadError};
    use serde_json::Error as JsonError;
    use std::error::Error;

    let source = decode::<Record>(
        &Record::STABLE_NAME,
        PayloadVersion::INITIAL,
        br#"{"value":"INV-7","count":7}"#,
    ).unwrap();
    let copied = source.clone();
    assert_eq!(source, copied);
    assert_eq!(copied.value, "INV-7");
    assert_eq!(copied.count, 7);
    let text = format!("{source:?}");
    assert!(text.contains("value: \"INV-7\""));
    assert!(text.contains("count: 7"));

    let ReadError::Decode(error) = decode::<Record>(
        &Record::STABLE_NAME,
        PayloadVersion::INITIAL,
        br#"{"value":"INV-7","count":"wrong"}"#,
    ).unwrap_err() else { panic!("expected a decoder error") };
    assert_eq!(error.kind(), DecodeFailureKind::Decode);
    assert_eq!(error.source_version(), PayloadVersion::INITIAL);
    assert!(error.source().unwrap().is::<JsonError>());
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
        "pub camelCase: u32,\n    #[history(added_in = v2, backfill_value = INITIAL_REVISION)]\n    pub revision: u32,",
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
    for record in ["Record", "Supporting"] {
        let field = "#[allow(deprecated)]\n    pub second: Old";
        let denied = field.replace("allow(deprecated)", "deny(deprecated)");
        let marker = format!("pub struct {record} {{");
        let (prefix, body) = DEPRECATED_SOURCE.split_once(&marker).unwrap();
        let source = format!("{prefix}{marker}{}", body.replacen(field, &denied, 1));
        fixture.write("src/lib.rs", &source);
        failure(
            &fixture.cargo(&["build", "--locked", "--offline"]),
            "deprecated type alias `Old`",
        );
    }

    let backfill = "    #[allow(deprecated)]\n    #[history(added_in = v2, backfill_value = INITIAL_REVISION)]\n    pub revision: u32,";
    let callback = "    #[allow(deprecated)]\n    #[history(added_in = v2, backfill_fn = birth)]\n    pub copied: u32,";
    let updated = DEPRECATED_SOURCE.replacen(
        "pub value: Old,",
        "#[history(updated_in = v2, previous_type = Old, backfill_fn = update)]\n    pub value: Old,",
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

use std::error::Error;
use std::fmt::{Display, Formatter, Result as FmtResult};

#[deprecated]
type Old = String;

#[history_api::versioned(stable_name = "example.deprecated.field")]
pub struct Record {
    #[allow(deprecated)]
    pub value: Old,
    #[allow(deprecated)]
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

impl Display for ConvertError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        formatter.write_str("conversion failed")
    }
}
impl Error for ConvertError {}

#[deprecated]
pub fn update(previous: &RecordV1) -> Result<String, ConvertError> {
    Ok(format!("{}!", previous.value))
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
        &Record::STABLE_NAME,
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

#[history_api::versioned(stable_name = "example.container.lints")]
#[allow(non_snake_case)]
#[allow(deprecated)]
pub struct ContainerAllowance {
    pub camelCase: u32,
}

#[history_api::versioned(stable_name = "example.field.lints")]
#[allow(non_snake_case)]
pub struct FieldAllowance {
    #[allow(non_snake_case)]
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
        &ContainerAllowance::STABLE_NAME,
        PayloadVersion::INITIAL,
        br#"{"camelCase":7}"#,
    ).unwrap();
    let second = decode::<FieldAllowance>(
        &FieldAllowance::STABLE_NAME,
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
