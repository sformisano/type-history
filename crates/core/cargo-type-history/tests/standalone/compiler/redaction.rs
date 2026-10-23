//! Direct derives through the package build hook and the generated payloads.

use super::super::{
    support::{failure, success, Fixture},
    tutorial::documented_block,
};
use super::ledger;
use serde_json::json;
use type_history_core::resolved::SchemaShape;

fn source(derives: &str, helpers: &str, fields: &str, options: &str) -> String {
    format!(
        r#"
use privacy::{{Secret, Sensitive as HiddenDerive}};
#[type_history::versioned(stable_name = "billing.invoice.issued", {options})]
#[derive({derives})]
{helpers}
pub struct Record {{ {fields} }}
"#
    )
}

#[test]
fn direct_redaction_accepts_renames_and_rejects_incompatible_derives_and_history() {
    let fixture = Fixture::empty();
    let mut manifest: toml::Value = toml::from_str(&fixture.read("Cargo.toml")).unwrap();
    let dependencies = manifest["dependencies"].as_table_mut().unwrap();
    let history = dependencies.remove("history_api").unwrap();
    dependencies.insert("type-history".into(), history);
    dependencies.insert(
        "privacy".into(),
        toml::Value::try_from(json!({
            "package": "redactable", "version": "=0.15.0"
        }))
        .unwrap(),
    );
    fixture.write("Cargo.toml", &toml::to_string(&manifest).unwrap());
    success(&fixture.cargo(&["generate-lockfile", "--offline"]));
    fixture.set_ledger(&json!({}));
    let fields = "#[sensitive(Secret)] pub owner: String,";
    let original = source("HiddenDerive, Eq", "", fields, "derive_debug = false");
    let snippet = documented_block("example:redactable.rs", "rust");
    fixture.write("src/lib.rs", &format!(
        "{original}\npub mod book_example {{ use privacy as redactable;\n{snippet}\n}}\n\
         #[cfg(test)] mod checks {{\n\
         use super::*; use privacy::ToRedacted;\n\
         #[test] fn renamed_derives_and_book_example() {{\n\
         let value = Record {{ owner: \"private-owner-canary\".into() }};\n\
         assert_eq!(value.to_redacted().json(), serde_json::json!({{\"owner\":\"[REDACTED]\"}}));\n\
         assert_eq!(value, value.clone());\n\
         let event = book_example::AccountCreated {{ owner_name: \"private-owner-canary\".into() }};\n\
         assert_eq!(event.to_redacted().json(), serde_json::json!({{\"owner_name\":\"[REDACTED]\"}}));\n\
         assert_eq!(serde_json::to_value(event).unwrap(), serde_json::json!({{\"owner_name\":\"private-owner-canary\"}}));\n\
         }} }}"
    ));
    success(&fixture.cargo(&["test", "--lib", "--locked", "--offline"]));

    for (input, diagnostics) in [
        (
            source("HiddenDerive", "", fields, ""),
            vec!["conflicting implementations", "Debug"],
        ),
        (
            source(
                "HiddenDerive, privacy::SensitiveDisplay",
                "#[error(\"{owner}\")]",
                fields,
                "derive_debug = false",
            ),
            vec!["conflicting implementations"],
        ),
        (
            source(
                "HiddenDerive",
                "#[not_sensitive]",
                fields,
                "derive_debug = false",
            ),
            vec!["only supported on fields"],
        ),
        (
            source(
                "HiddenDerive",
                "",
                "#[error(\"owner\")] pub owner: String,",
                "derive_debug = false",
            ),
            vec!["record fields support"],
        ),
        (
            source(
                "HiddenDerive",
                "",
                "pub owner: String,",
                "derive_debug = false",
            ),
            vec!["Redactable"],
        ),
    ] {
        fixture.write("src/lib.rs", &input);
        let output = fixture.cargo(&["check", "--locked", "--offline"]);
        for diagnostic in diagnostics {
            failure(&output, diagnostic);
        }
    }

    for owned in [
        "Clone",
        "PartialEq",
        "Debug",
        "serde::Serialize",
        "serde::Deserialize",
        "type_history::Schema",
    ] {
        fixture.write("src/lib.rs", &source(owned, "", "pub owner: String,", ""));
        failure(
            &fixture.cargo(&["check", "--locked", "--offline"]),
            "conflicting implementations",
        );
    }

    fixture.set_ledger(&ledger(vec![vec![("owner", SchemaShape::String)]]));
    fixture.write("src/lib.rs", &source(
        "privacy::SensitiveDisplay", "#[error(\"{label}\")]",
        &format!("{fields} #[history(added_in = v2, backfill_value = String::new())] #[not_sensitive] pub label: String,"),
        "derive_debug = false",
    ));
    failure(
        &fixture.cargo(&["check", "--locked", "--offline"]),
        "unknown field `label` in format string",
    );

    fixture.set_ledger(&ledger(vec![vec![("owner", SchemaShape::U32)]]));
    fixture.write("src/lib.rs", &(source(
        "HiddenDerive", "",
        "#[history(updated_in = v2, previous_type = u32, backfill_fn = convert)] #[sensitive(privacy::Email)] pub owner: String,",
        "derive_debug = false",
    ) + "fn convert(old: &RecordV1) -> Result<String, std::convert::Infallible> { Ok(old.owner.to_string()) }"));
    let output = fixture.cargo(&["check", "--locked", "--offline"]);
    failure(&output, "Email");
    failure(&output, "u32");
    failure(
        &output,
        "u32: redactable::__private::PolicyField<redactable::Email>",
    );
}
