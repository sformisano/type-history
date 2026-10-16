//! The current alias exposes the documented fields, methods, and traits.

use super::support::{success, Fixture, V2};
use std::fs;
use std::path::Path;

#[test]
fn current_alias_links_to_documented_public_fields_methods_and_traits() {
    let fixture = Fixture::frozen();
    let source = V2
        .replace(
            "pub struct Invoice",
            "/// Current invoice documentation.\npub struct Invoice",
        )
        .replace(
            "pub count: u64",
            "/// Number of billed units.\n    pub count: u64",
        );
    fixture.write("src/lib.rs", &source);
    success(&fixture.cargo_env(
        &["doc", "--no-deps", "--locked", "--offline"],
        &[("RUSTDOCFLAGS", Some("-Dwarnings"))],
    ));
    let docs = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("repository root")
        .join("target/doc/standalone_history_consumer");
    let alias = fs::read_to_string(docs.join("type.Invoice.html")).expect("current alias page");
    assert!(alias.contains("Current invoice documentation."));
    assert!(alias.contains("struct.InvoiceV2.html"));
    assert!(alias.contains("pub count:"));
    let current = fs::read_to_string(docs.join("struct.InvoiceV2.html"))
        .expect("current numbered record page");
    for visible in [
        "Current invoice documentation.",
        "Number of billed units.",
        "id=\"structfield.count\"",
        "id=\"method.from_versioned\"",
        "id=\"method.into_versioned\"",
        "id=\"trait-implementations\"",
    ] {
        assert!(current.contains(visible), "missing rendered API: {visible}");
    }
    let index = fs::read_to_string(docs.join("index.html")).expect("consumer documentation index");
    assert!(!index.contains("struct.InvoiceV1.html"));
}
