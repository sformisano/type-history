use super::super::support::{failure, success, text};
use super::{fixture, replace};
use serde_json::Value;
use std::collections::BTreeMap;

const DEPENDENCIES: &str = r#"
float_native = { package = "typed_floats", version = "1.0.7", default-features = false, features = ["std", "serde"], optional = true }
uuid_native = { package = "uuid", version = "1.26.1", default-features = false, features = ["std"], optional = true }
decimal_native = { package = "rust_decimal", version = "1.43.0", default-features = false, features = ["std"], optional = true }
chrono_native = { package = "chrono", version = "0.4.45", default-features = false, features = ["std"], optional = true }
time_native = { package = "time", version = "0.3.55", default-features = false, features = ["std"], optional = true }
"#;
const FEATURES: &str = r#"
[features]
default = []
typed-floats = ["history_api/typed-floats", "dep:float_native"]
uuid = ["history_api/uuid", "dep:uuid_native"]
rust-decimal = ["history_api/rust-decimal", "dep:decimal_native"]
chrono = ["history_api/chrono", "dep:chrono_native"]
time = ["history_api/time", "dep:time_native"]
rc = ["history_api/rc"]
all-integrations = ["typed-floats", "uuid", "rust-decimal", "chrono", "time", "rc"]
unified = ["all-integrations", "decimal_native/serde-float", "time_native/serde-human-readable", "time_native/large-dates", "uuid_native/serde", "chrono_native/serde"]
native-only = ["dep:float_native"]
"#;

#[test]
fn field_support_individual_and_unified_feature_graphs_preserve_contracts() {
    let mut individual = BTreeMap::new();
    for selection in [
        "",
        "typed-floats",
        "uuid",
        "rust-decimal",
        "chrono",
        "time",
        "rc",
        "all-integrations",
        "unified",
    ] {
        // Each graph owns a separate consumer directory and resolved metadata.
        let fixture = fixture(&[], DEPENDENCIES);
        fixture.write("Cargo.toml", &(fixture.read("Cargo.toml") + FEATURES));
        success(&fixture.cargo(&["generate-lockfile", "--offline"]));
        fixture.write(
            "src/lib.rs",
            include_str!("../fixtures/fields/feature_matrix.rs"),
        );
        success(&fixture.cargo(&[
            "test",
            "--lib",
            "--locked",
            "--offline",
            "--features",
            selection,
        ]));
        let metadata = fixture.cargo(&[
            "metadata",
            "--format-version=1",
            "--locked",
            "--offline",
            "--features",
            selection,
        ]);
        success(&metadata);
        let metadata: Value = serde_json::from_slice(&metadata.stdout).unwrap();
        let tree = fixture.cargo(&[
            "tree",
            "--locked",
            "--offline",
            "-e",
            "features",
            "--features",
            selection,
        ]);
        success(&tree);
        println!("FIELD-FEATURE-GRAPH {selection}\n{}", text(&tree));
        fixture.write(
            "feature-metadata.json",
            &serde_json::to_string(&metadata).unwrap(),
        );
        let snapshot: BTreeMap<String, Value> =
            serde_json::from_str(&fixture.read("feature-snapshots.json")).unwrap();
        if matches!(selection, "all-integrations" | "unified") {
            assert_eq!(
                snapshot, individual,
                "feature unification changed schema, bytes or values"
            );
        } else {
            for (name, value) in snapshot {
                assert!(individual.insert(name, value).is_none());
            }
        }
        if selection == "unified" {
            for (package, feature) in [
                ("time", "large-dates"),
                ("time", "serde-human-readable"),
                ("rust_decimal", "serde-float"),
                ("uuid", "serde"),
                ("chrono", "serde"),
            ] {
                let id = metadata["packages"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|p| p["name"] == package)
                    .unwrap()["id"]
                    .as_str()
                    .unwrap();
                let node = metadata["resolve"]["nodes"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|p| p["id"] == id)
                    .unwrap();
                assert!(
                    node["features"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|v| v == feature),
                    "missing {package}/{feature}"
                );
            }
            let source = replace(
                include_str!("../fixtures/fields/temporal.rs"),
                "use history_api::adapters::chrono as temporal;",
                "use history_api::adapters::time as temporal;",
            );
            fixture.write(
                "src/lib.rs",
                &(source + "\n#[cfg(test)] mod large_dates;\n"),
            );
            fixture.write(
                "src/large_dates.rs",
                include_str!("../fixtures/fields/large_dates.rs"),
            );
            success(&fixture.cargo(&[
                "test",
                "--lib",
                "--locked",
                "--offline",
                "--features",
                selection,
            ]));
        }
        if selection.is_empty() {
            for (source, expected) in [
                ("use history_api::adapters::UuidText;", "UuidText"),
                ("use history_api::adapters::DecimalText;", "DecimalText"),
                ("use history_api::adapters::chrono;", "chrono"),
                ("use history_api::adapters::time;", "time"),
                ("#[derive(history_api::Schema)] struct Missing { value: std::rc::Rc<u32> }", "persisted field"),
                ("#[derive(history_api::Schema)] struct Missing { value: float_native::NonNaNFinite<f32> }", "persisted field"),
            ] {
                fixture.write("src/lib.rs", source);
                failure(&fixture.cargo(&["check", "--locked", "--offline", "--features", "native-only"]), expected);
            }
        }
    }
}
