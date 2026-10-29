//! The six published crates require each other at exactly one release, so an
//! exact `type-history` and `type-history-build` pin also selects the matching
//! macros, generator, and core crates.

use std::fs;
use std::path::Path;
use toml::Value;

const PUBLISHED: [&str; 6] = [
    "type-history-core",
    "type-history-codegen",
    "type-history-macros",
    "type-history",
    "type-history-build",
    "cargo-type-history",
];

#[test]
fn published_crates_require_each_other_at_the_exact_release() {
    let core = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("core crates directory");
    let release = env!("CARGO_PKG_VERSION");
    let exact = format!("={release}");
    let mut requirements = 0;
    for name in PUBLISHED {
        let manifest: Value =
            toml::from_str(&fs::read_to_string(core.join(name).join("Cargo.toml")).unwrap())
                .unwrap();
        assert_eq!(
            manifest["package"]["version"].as_str(),
            Some(release),
            "{name}"
        );
        for section in ["dependencies", "dev-dependencies", "build-dependencies"] {
            let Some(table) = manifest.get(section).and_then(Value::as_table) else {
                continue;
            };
            for (dependency, specification) in table {
                if PUBLISHED.contains(&dependency.as_str()) {
                    assert_eq!(
                        specification.get("version").and_then(Value::as_str),
                        Some(exact.as_str()),
                        "{name} {section} {dependency}"
                    );
                    requirements += 1;
                }
            }
        }
    }
    assert!(requirements > 0, "no inter-crate requirements found");
}
