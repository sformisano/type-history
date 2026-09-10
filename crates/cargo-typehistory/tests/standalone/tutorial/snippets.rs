//! Compile marked README fragments in the tutorial's existing consumers.

use super::documented;

pub(super) fn test_module(cases: &[(&str, String)]) -> String {
    let mut source = String::from(
        "#[cfg(test)] mod readme_snippets {\nuse super::*;\nuse std::error::Error as TestError;\n",
    );
    for (name, body) in cases {
        source.push_str(&format!(
            "#[test] fn {name}() -> Result<(), Box<dyn TestError>> {{\n{body}\nOk(())\n}}\n"
        ));
    }
    source.push_str("}\n");
    source
}

pub(super) fn receipt_tests() -> String {
    let mut cases = [
        ("overview", "overview:read-old.rs"),
        ("json", "decoding:json.rs"),
        ("source_version", "decoding:source-version.rs"),
        ("invalid_input", "decoding:invalid.rs"),
        ("raw_json", "decoding:raw-json.rs"),
        ("business_values", "reference:business-values.rs"),
    ]
    .into_iter()
    .map(|(name, marker)| (name, documented(marker).to_owned()))
    .collect::<Vec<_>>();
    cases.push((
        "messagepack",
        format!(
            "let receipt = Receipt {{ amount_cents: 3000, currency: \"EUR\".to_owned() }};\n\
             let expected = receipt.clone();\n{}\nassert_eq!(receipt, expected);\n",
            documented("decoding:messagepack.rs")
        ),
    ));
    test_module(&cases)
}

pub(super) fn invoice_tests() -> String {
    test_module(&[(
        "conversion_error",
        documented("decoding:conversion-error.rs").to_owned(),
    )])
}

pub(super) fn check_invoice_fragments(v3: &str) {
    let source = normalize_indentation(v3);
    for marker in ["reference:ordered-field.rs", "reference:rename-callback.rs"] {
        assert!(
            source.contains(&normalize_indentation(documented(marker))),
            "{marker} must match the invoice source compiled by this tutorial"
        );
    }
    for field in documented("reference:rename-fields.rs")
        .trim()
        .split("\n\n")
    {
        assert!(
            source.contains(&normalize_indentation(field)),
            "the rename field must match the compiled invoice source: {field}"
        );
    }
}

fn normalize_indentation(source: &str) -> String {
    source
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn receipt_v3() -> String {
    let v2 = documented("quick-example:v2.rs");
    let old_currency = "    #[history(added_at = v2, backfill_value = \"USD\".to_owned())]\n    pub currency: String,";
    assert_eq!(v2.matches(old_currency).count(), 1, "V2 currency field");
    let mut v3 = v2.replacen(old_currency, documented("reference:same-type-field.rs"), 1);
    let end = v3.rfind('}').expect("receipt declaration's closing brace");
    v3.insert_str(end, documented("reference:optional-field.rs"));
    v3.push_str(documented("reference:same-type-callback.rs"));
    v3.push_str(&test_module(&[(
        "v2_gains_reference_and_normalizes_currency",
        r##"
let stored = serde_json::from_slice(
    br#"{"shop.receipt.created":{"2":{"amount_cents":1200,"currency":"eur"}}}"#,
)?;
let receipt = Receipt::from_versioned(stored)?;
assert_eq!(receipt.amount_cents, 1200);
assert_eq!(receipt.currency, "EUR");
assert_eq!(receipt.reference, None);
assert_eq!(receipt.into_versioned().source_version().get(), 3);
"##
        .to_owned(),
    )]));
    v3
}
