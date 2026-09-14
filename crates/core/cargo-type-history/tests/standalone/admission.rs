use super::support::{failure, success, Fixture};

const DIRECT: &str = "#[history_api::versioned(stable_name=\"admission.direct\")] pub struct Direct { pub value: u32 }";
const CUSTOM: &str = r#"fn main() {
    use std::path::PathBuf;
    use history_build::{contract::ToolContract, inventory::Admission};
    let root = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let source = history_build::discover::read(&root, Admission::Ordinary).unwrap();
    let contract = ToolContract { strict_env: "CUSTOM_HISTORY_STRICT", ..history_build::contract::STANDALONE };
    history_build::compile::compile_package(&root, &source, &contract).unwrap();
}"#;

#[test]
fn both_hooks_reject_hidden_declarations_and_enforce_profiles() {
    let fixture = Fixture::empty();
    success(&fixture.cli(&["init", "--package", "standalone-history-consumer"]));
    for (hook, strict) in [
        (
            "fn main() { history_build::compile(); }",
            "TYPE_HISTORY_REQUIRE_FROZEN",
        ),
        (CUSTOM, "CUSTOM_HISTORY_STRICT"),
    ] {
        fixture.write("build.rs", hook);
        fixture.write("src/lib.rs", DIRECT);
        success(&fixture.cargo(&["build", "--locked", "--offline"]));
        failure(
            &fixture.cargo_env(&["build", "--locked", "--offline"], &[(strict, Some("1"))]),
            "is a draft",
        );
        for profile in ["release", "release_child"] {
            failure(
                &fixture.cargo(&["build", "--locked", "--offline", "--profile", profile]),
                "is a draft",
            );
        }
        fixture.write("src/included.rs", DIRECT);
        let hidden = [
            format!("macro_rules! declaration {{ () => {{ {DIRECT} }} }} declaration!();"),
            "include!(\"included.rs\");".into(),
            format!("pub fn local() {{ {DIRECT} }}"),
        ];
        // The shared reader needs one custom-hook origin control. Ordinary
        // compilation covers all forms; direct declarations cover profile inheritance.
        let hidden_count = if hook == CUSTOM { 1 } else { hidden.len() };
        for source in hidden.into_iter().take(hidden_count) {
            fixture.write("src/lib.rs", &source);
            failure(
                &fixture.cargo(&["build", "--locked", "--offline"]),
                "unsupported history declaration",
            );
            if hook != CUSTOM {
                failure(
                    &fixture.cargo(&["build", "--locked", "--offline", "--release"]),
                    "unsupported history declaration",
                );
            }
            failure(
                &fixture.cargo_env(&["build", "--locked", "--offline"], &[(strict, Some("1"))]),
                "unsupported history declaration",
            );
        }
        fixture.write(
            "src/lib.rs",
            &format!("{DIRECT}\npub fn hidden() {{ {DIRECT} }}"),
        );
        failure(
            &fixture.cargo(&["build", "--locked", "--offline"]),
            "unsupported history declaration",
        );
    }
    fixture.write("build.rs", "fn main() { history_build::compile(); }");
    fixture.write("src/lib.rs", DIRECT);
    success(&fixture.cli(&["freeze", "--package", "standalone-history-consumer"]));
    for hook in ["fn main() { history_build::compile(); }", CUSTOM] {
        fixture.write("build.rs", hook);
        fixture.write(
            "src/lib.rs",
            &format!("{DIRECT}\npub fn hidden() {{ {DIRECT} }}"),
        );
        failure(
            &fixture.cargo(&["build", "--locked", "--offline"]),
            "unsupported history declaration",
        );
    }
}

#[test]
fn source_positions_and_reused_target_recover_without_false_rejection() {
    let fixture = Fixture::empty();
    success(&fixture.cli(&["init", "--package", "standalone-history-consumer"]));
    for hook in ["fn main() { history_build::compile(); }", CUSTOM] {
        fixture.write("build.rs", hook);
        fixture.write("src/lib.rs", "// café 語\nmod Cafe\u{301} { /* é語 */ #[history_api::versioned(stable_name=\"admission.unicode\")] pub struct Cafe\u{301} {} #[history_api::versioned(stable_name=\"admission.raw\")] pub struct r#type {} }");
        success(&fixture.cargo(&["build", "--locked", "--offline"]));
        fixture.write("src/moved.rs", DIRECT);
        fixture.write("src/lib.rs", "mod moved;");
        success(&fixture.cargo(&["build", "--locked", "--offline"]));
        fixture.write("src/lib.rs", DIRECT);
        success(&fixture.cargo(&["build", "--locked", "--offline"]));
        fixture.write("build.rs", "fn main() {}");
        failure(
            &fixture.cargo(&["build", "--locked", "--offline"]),
            "call type_history_build::compile()",
        );
        fixture.write("build.rs", hook);
        success(&fixture.cargo(&["build", "--locked", "--offline"]));
    }
}

#[test]
fn failed_public_hook_publication_stops_build_and_next_attempt_recovers() {
    let fixture = Fixture::frozen();
    let ledger = fixture.read("type-history/schemas.json");
    {
        let hook = "history_build::compile();";
        fixture.write(
            "build.rs",
            &format!(
                r#"fn main() {{
            use std::path::PathBuf;
            let parent=PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("type-history");
            if parent.is_dir() {{ std::fs::remove_dir_all(&parent).unwrap(); }}
            std::fs::write(&parent,b"fixture-owned conflicting parent").unwrap();
            {hook}
        }}"#
            ),
        );
        let rejected = fixture.cargo(&["build", "--locked", "--offline"]);
        failure(&rejected, "failed to run custom build command");
        assert!(
            !super::support::text(&rejected).contains("cargo::rustc-env=TYPE_HISTORY_ADMISSION=")
        );
        assert_eq!(fixture.read("type-history/schemas.json"), ledger);
        fixture.write(
            "build.rs",
            &format!(
                r#"fn main() {{
            use std::path::PathBuf;
            let parent=PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("type-history");
            if parent.is_file() {{ std::fs::remove_file(&parent).unwrap(); }}
            {hook}
        }}"#
            ),
        );
        success(&fixture.cargo(&["build", "--locked", "--offline"]));
    }
}
