use super::support::{failure, success, Fixture, LEDGER, STABLE_NAME, V1, V2, V3};
use std::time::{Duration, Instant};
use toml::{Table, Value};

#[test]
fn unknown_arguments_and_field_attributes_are_rejected() {
    let fixture = Fixture::empty();
    success(&fixture.cli(&["init", "--package", "standalone-history-consumer"]));
    let before = fixture.read(LEDGER);
    for (source, diagnostic) in [
        (
            V1.replace(
                "billing.invoice.issued\")",
                "billing.invoice.issued\", unknown = true)",
            ),
            "expected `stable_name`",
        ),
        (
            V1.replace("pub count", "#[unknown] pub count"),
            "record fields support only documentation, lints, and `#[history(...)]`",
        ),
    ] {
        assert_ne!(source, V1);
        fixture.write("src/lib.rs", &source);
        failure(
            &fixture.cargo(&["build", "--locked", "--offline"]),
            diagnostic,
        );
        assert_eq!(fixture.read(LEDGER), before);
    }
}

#[test]
fn target_dependencies_enforce_drafts_and_frozen_inventory() {
    for inherited in [false, true] {
        let fixture = Fixture::empty();
        let mut manifest: Value = toml::from_str(&fixture.read("Cargo.toml")).unwrap();
        let dependency = manifest["dependencies"]
            .as_table_mut()
            .unwrap()
            .remove("history_api")
            .unwrap();
        let selected = if inherited {
            manifest["workspace"].as_table_mut().unwrap().insert(
                "dependencies".into(),
                Table::from_iter([("history_api".into(), dependency)]).into(),
            );
            Table::from_iter([("workspace".into(), true.into())]).into()
        } else {
            dependency
        };
        let targets = Table::from_iter([(
            "cfg(all())".into(),
            Table::from_iter([(
                "dependencies".into(),
                Table::from_iter([("history_api".into(), selected)]).into(),
            )])
            .into(),
        )]);
        manifest
            .as_table_mut()
            .unwrap()
            .insert("target".into(), targets.into());
        fixture.write("Cargo.toml", &toml::to_string(&manifest).unwrap());
        success(&fixture.cargo(&["generate-lockfile", "--offline"]));
        success(&fixture.cli(&["init", "--package", "standalone-history-consumer"]));
        fixture.write("src/lib.rs", V1);
        let empty = fixture.read(LEDGER);
        {
            let strict = "TYPE_HISTORY_REQUIRE_FROZEN";
            failure(
                &fixture.cargo_env(&["build", "--locked", "--offline"], &[(strict, Some("1"))]),
                "draft",
            );
        }
        failure(
            &fixture.cargo(&["build", "--release", "--locked", "--offline"]),
            "draft",
        );
        assert_eq!(fixture.read(LEDGER), empty);
        success(&fixture.cli(&["freeze", "--package", "standalone-history-consumer"]));
        success(&fixture.cli(&["check"]));
        success(&fixture.cargo(&["build", "--release", "--locked", "--offline"]));
        let frozen = fixture.read(LEDGER);
        fixture.write("src/lib.rs", "");
        failure(
            &fixture.cargo(&["build", "--locked", "--offline"]),
            "missing",
        );
        assert_eq!(fixture.read(LEDGER), frozen);
    }
}

#[test]
fn standalone_adjacent_history_runtime_and_error_sources() {
    let fixture = Fixture::frozen();
    fixture.write("src/lib.rs", V2);
    success(&fixture.cargo(&["build", "--locked", "--offline"]));
    success(&fixture.cli(&[
        "freeze",
        "--package",
        "standalone-history-consumer",
        "--type",
        STABLE_NAME,
        "--version",
        "2",
    ]));
    fixture.write("src/lib.rs", V3);
    success(&fixture.cargo(&["test", "--lib", "--locked", "--offline"]));
    success(&fixture.cli(&["freeze", "--package", "standalone-history-consumer"]));
    success(&fixture.cargo(&["build", "--release", "--locked", "--offline"]));
    assert_eq!(fixture.ledger()[STABLE_NAME].as_object().unwrap().len(), 3);

    // Body semantics are outside wire authority; the authored conversion still runs.
    let revised = V3.replace("Ok(value.to_string())", "Ok(format!(\"{value}\"))");
    assert_ne!(revised, V3);
    fixture.write("src/lib.rs", &revised);
    success(&fixture.cargo(&["test", "--lib", "--locked", "--offline"]));
    success(&fixture.cli(&["check"]));
}

#[test]
fn standalone_compiler_checks_history_contracts_and_macro_hygiene() {
    let fixture = Fixture::frozen();
    let before = fixture.read(LEDGER);
    let cases = [
        (
            V2.replace("impl Error for ConvertError {}", ""),
            "std::error::Error",
        ),
        (
            V2.replace(", backfill_fn = widen", ""),
            "backfill_fn",
        ),
        (
            V2.replace("fn widen(previous: &InvoiceV1)", "fn widen(previous: InvoiceV1)"),
            "mismatched types",
        ),
        (
            V2.replace(
                "Result<u64, ConvertError> { Ok(u64::from(previous.count)) }",
                "Result<String, ConvertError> { Ok(previous.count.to_string()) }",
            ),
            "mismatched types",
        ),
        (
            V2.replace("previous: &InvoiceV1", "previous: &InvoiceV2"),
            "mismatched types",
        ),
        (
            V2.replace("previous_type = u32", "from = u32"),
            "unknown history key",
        ),
        (V2.replace("updated_in = v2", "updated_in = v0"), "positive"),
        (
            V2.replace("updated_in = v2", "updated_in = vbanana"),
            "a history version is written as",
        ),
        (
            V2.replace(
                "#[history(added_in = v2, backfill_value = 7_u32)]",
                "#[history(added_in = v2)]",
            ),
            "backfill_value",
        ),
        (
            V2.replace(
                "backfill_value = 7_u32",
                "backfill_value = 7_u32, backfill_fn = label",
            ),
            "exactly one",
        ),
        (
            V2.replace(
                "removed_in = v2",
                "removed_in = v2, backfill_fn = label",
            ),
            "a removal carries no backfill",
        ),
        (
            V1.replace("billing.invoice.issued", "ReceiptCreated"),
            "stable name segment 1 must start with a lowercase ASCII letter",
        ),
        (
            V1.replace(
                "#[versioned(stable_name",
                "#[derive(Clone)]\n#[versioned(stable_name",
            ),
            "derive",
        ),
        (
            V1.replace("pub struct Invoice", "#[derive(Clone)]\npub struct Invoice"),
            "derive",
        ),
        (
            V1.replace("pub struct Invoice", "pub struct Invoice<T>"),
            "generic",
        ),
        (
            V1.replace(
                "pub count: u32,",
                "pub r#count: u32,\npub count: u32,",
            ),
            "duplicate",
        ),
        (format!("{V1}\nstruct InvoiceV1;"), "InvoiceV1"),
        (
            format!(
                "{V1}\n#[cfg_attr(any(), history_api::versioned(stable_name = \"example.optional.record\"))] struct Conditional {{}}"
            ),
            "conditional",
        ),
        (
            format!(
                "{V1}\n#[cfg(any())] mod conditional {{ use history_api::versioned; #[versioned(stable_name = \"example.optional.record\")] struct Conditional {{}} }}"
            ),
            "conditional",
        ),
    ];
    for (source, diagnostic) in cases {
        fixture.write("src/lib.rs", &source);
        failure(
            &fixture.cargo(&["build", "--locked", "--offline"]),
            diagnostic,
        );
        assert_eq!(fixture.read(LEDGER), before);
    }
    fixture.write(
        "src/lib.rs",
        &V2.replace("updated_in = v2", "updated_in = v4294967295"),
    );
    let started = Instant::now();
    failure(
        &fixture.cargo(&["build", "--locked", "--offline"]),
        "exceeds the one permitted successor",
    );
    assert!(
        started.elapsed() < Duration::from_secs(30),
        "unauthorized range was not rejected promptly"
    );
    fixture.write(
        "src/lib.rs",
        &format!(
            r#"{V1}
#[history_api::versioned(stable_name = "example.upper.record")]
pub struct AB {{}}
#[history_api::versioned(stable_name = "example.mixed.record")]
pub struct Ab {{}}
use history_api::versioned as evolve;
mod nested {{
    use super::evolve;
    #[evolve(stable_name = "example.raw.record")]
    pub struct Raw {{ pub r#type: u32 }}
}}
#[test]
fn public_aliases_and_raw_names_are_usable() {{
    use nested::Raw;
    let _: AB = ABV1 {{}};
    let _: Ab = AbV1 {{}};
    use history_api::{{decode, HasHistory, PayloadVersion}};
    let _: AB = decode::<AB>(&AB::STABLE_NAME, PayloadVersion::INITIAL, b"{{}}").unwrap();
    let _: Ab = decode::<Ab>(&Ab::STABLE_NAME, PayloadVersion::INITIAL, b"{{}}").unwrap();
    let value = Raw {{ r#type: 7 }};
    assert_eq!(value.r#type, 7);
}}
"#
        ),
    );
    success(&fixture.cargo(&["test", "--lib", "--locked", "--offline"]));
    success(&fixture.cli(&["freeze", "--package", "standalone-history-consumer"]));
    assert_eq!(fixture.ledger().as_object().unwrap().len(), 4);
}

#[test]
fn standalone_removed_field_visibility_and_module_relative_callbacks() {
    let fixture = Fixture::frozen();
    let source = V2
        .replace("backfill_fn = widen", "backfill_fn = callbacks::widen")
        .replace("backfill_fn = label", "backfill_fn = callbacks::label")
        .replace(
            "fn widen(previous: &InvoiceV1)",
            "mod callbacks { use super::{ConvertError, InvoiceV1}; pub fn widen(previous: &InvoiceV1)",
        )
        .replace("fn label(previous:", "pub fn label(previous:");
    fixture.write("src/lib.rs", &format!("{source}\n}}\n"));
    success(&fixture.cargo(&["build", "--locked", "--offline"]));
    failure(
        &{
            fixture.write(
                "src/lib.rs",
                &format!(
                    "{source}\n}}\nfn forbidden(value: Invoice) {{ let _ = value.legacy; }}\n"
                ),
            );
            fixture.cargo(&["build", "--locked", "--offline"])
        },
        "no field",
    );
    fixture.write(
        "src/lib.rs",
        &format!(
            r#"{V1}
mod private {{
    #[history_api::versioned(stable_name = "example.private.record")]
    pub struct Hidden {{ value: String }}
}}
fn forbidden(value: private::Hidden) {{ let _ = value.value; }}
"#
        ),
    );
    failure(
        &fixture.cargo(&["build", "--locked", "--offline"]),
        "private",
    );
}
