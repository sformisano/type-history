use super::super::support::{failure, success, LEDGER};
use super::{check, fixture, freeze, frozen_failure, replace, tests};

const LONG: &str = include_str!("../fixtures/fields/long_tuples.rs");
const NATIVE: &str = include_str!("../fixtures/fields/native_traits.rs");
const RETAINED: &str = r#"
#[tracked(stable_name = "fields.retained_tuple", derive_partial_eq = false, derive_debug = false,)]
pub struct Retained { pub value: T16, pub counter: u32 }
#[cfg(test)]
mod retained_tests {
    use super::{common, Retained};
    use history_api::Versioned;
    use serde_json::json;
    #[test]
    fn saved_old_tuple() {
        let value: Retained = serde_json::from_value(json!({"value":(1..=16).collect::<Vec<_>>(),"counter":17})).unwrap();
        let (json, binary) = common::retain("retained-tuple", &value.into_versioned());
        let json: Versioned<Retained> = serde_json::from_slice(&json).unwrap();
        let binary: Versioned<Retained> = rmp_serde::from_slice(&binary).unwrap();
        for stored in [json, binary] {
            assert_eq!(stored.source_version().get(), 1);
            assert_eq!(Retained::from_versioned(stored).unwrap().counter, 17);
        }
    }
}
"#;

#[test]
fn field_support_long_tuples_recursive_aliases_and_retained_versions() {
    let fixture = fixture(&["rc"], "");
    let initial = format!("{LONG}\n{RETAINED}");
    fixture.write("src/lib.rs", &initial);
    tests(&fixture);
    freeze(&fixture);
    tests(&fixture);
    // Trait options do not conceal actual arity changes from frozen checks.
    frozen_failure(
        &fixture,
        &replace(&initial, "pub t16: T16", "pub t16: T15"),
        "changed its frozen wire shape",
    );
    fixture.write("src/lib.rs", &initial);
    for bound in ["Debug", "PartialEq"] {
        let source = format!("{initial}\nfn require<T: std::{}::{bound}>() {{}}\nfn unavailable() {{ require::<Tuples>(); }}", if bound == "Debug" { "fmt" } else { "cmp" });
        fixture.write("src/lib.rs", &source);
        failure(&fixture.cargo(&["check", "--locked", "--offline"]), bound);
    }
    let updated = replace(
        &initial,
        "pub struct Retained { pub value: T16, pub counter: u32 }",
        "pub struct Retained { #[history(removed_in = v2)] pub value: T16, pub counter: u32 }",
    );
    // The test writes V1 bytes only when absent; the retained files already exist.
    // Current V2 no longer contains value, so construct a numbered V1 explicitly.
    let updated = replace(
        &updated,
        "use super::{common, Retained};",
        "use super::{common, Retained, RetainedV1};",
    );
    let updated = replace(
        &updated,
        "let value: Retained = serde_json::from_value",
        "let value: RetainedV1 = serde_json::from_value",
    );
    let updated = replace(&updated, "let (json, binary) = common::retain(\"retained-tuple\", &value.into_versioned());", "let _ = value; let json = std::fs::read(\"retained-tuple.json\").unwrap(); let binary = std::fs::read(\"retained-tuple.msgpack\").unwrap();");
    fixture.write("src/lib.rs", &updated);
    tests(&fixture);
    freeze(&fixture);
    check(&fixture);
    tests(&fixture);
}

#[test]
fn field_support_native_traits_and_generation_options_preserve_storage() {
    let fixture = fixture(&[], "");
    fixture.write("src/lib.rs", NATIVE);
    tests(&fixture);
    freeze(&fixture);
    let ledger = fixture.read(LEDGER);
    let (prefix, checks) = NATIVE.split_once("// NATIVE_CHECK_BEGIN").unwrap();
    let (_, suffix) = checks.split_once("// NATIVE_CHECK_END").unwrap();
    let no_checks = format!("{prefix}{suffix}");
    for options in [
        "derive_debug = false",
        "derive_partial_eq = false",
        "derive_debug = false, derive_partial_eq = false",
    ] {
        let source = replace(
            &no_checks,
            "stable_name = \"fields.native\"",
            &format!("stable_name = \"fields.native\", {options}"),
        );
        fixture.write("src/lib.rs", &source);
        check(&fixture);
        tests(&fixture);
        assert_eq!(fixture.read(LEDGER), ledger);
    }
    // Each flag removes only its own native bound.
    for (native, options, operation) in [
        (
            "Debug",
            "derive_partial_eq = false",
            "let _ = format!(\"{:?}\", value);",
        ),
        (
            "PartialEq",
            "derive_debug = false",
            "assert!(value == value.clone());",
        ),
    ] {
        let source = format!(
            r#"
use history_api::{{versioned, Schema}};
use serde::{{Deserialize, Serialize}};
#[derive(Clone, {native}, Serialize, Deserialize, Schema)]
pub struct Only(u32);
#[versioned(stable_name = "fields.only", {options})]
pub struct OnlyRecord {{ pub value: Only }}
pub fn works() {{ let value = OnlyRecord {{ value: Only(7) }}; {operation} }}
"#
        );
        // Separate history prevents unrelated removed-history diagnostics.
        fixture.write(
            "src/lib.rs",
            &(NATIVE.to_owned()
                + &source
                    .replace("use history_api::{versioned, Schema};", "")
                    .replace("use serde::{Deserialize, Serialize};", "")),
        );
        success(&fixture.cargo(&["check", "--locked", "--offline"]));
    }
    for (argument, expected) in [
        ("derive_debug = false, derive_debug = true", "duplicate"),
        (
            "derive_partial_eq = true, derive_partial_eq = false",
            "duplicate",
        ),
        ("derive_debug = 0", "boolean"),
        ("derive_partial_eq = \"false\"", "boolean"),
        (
            "derive_order = false",
            "expected `stable_name`, `derive_debug`, or `derive_partial_eq`",
        ),
    ] {
        let source = replace(
            NATIVE,
            "stable_name = \"fields.native\"",
            &format!("stable_name = \"fields.native\", {argument}"),
        );
        fixture.write("src/lib.rs", &source);
        failure(
            &fixture.cargo(&["check", "--locked", "--offline"]),
            expected,
        );
    }
}

#[test]
fn field_support_documented_tuple_options_compile_and_freeze() {
    let fixture = fixture(&[], "");
    let mut source = String::new();
    for (module, document) in [
        (
            "integration_example",
            include_str!("../../../../../../book/src/integration.md"),
        ),
        (
            "macro_example",
            include_str!("../../../../../../book/src/macro.md"),
        ),
    ] {
        let snippets: Vec<_> = document
            .split("```rust\n")
            .skip(1)
            .map(|block| block.split_once("```").unwrap().0)
            .filter(|block| block.contains("derive_debug = false"))
            .collect();
        assert_eq!(snippets.len(), 1, "one complete tuple example in {module}");
        let snippet = replace(
            snippets[0],
            "use type_history::versioned;",
            "use history_api::versioned;",
        );
        source.push_str(&format!("pub mod {module} {{\n{snippet}\n}}\n"));
    }
    fixture.write("src/lib.rs", &source);
    tests(&fixture);
    freeze(&fixture);
    check(&fixture);
}
