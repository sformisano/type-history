use super::support::{failure, success, text, Fixture, LEDGER, V1};
use serde_json::Value as JsonValue;
use std::fs;
use toml::Value;

#[test]
fn repeated_source_expansions_cannot_reuse_module_admission() {
    let fixture = Fixture::frozen();
    fixture.write("src/records.rs", V1);
    let direct = "#[path=\"records.rs\"] pub mod records;";
    fixture.write("src/lib.rs", direct);
    success(&fixture.cargo(&["build", "--lib", "--release", "--locked", "--offline"]));
    for copied in [
        "pub mod copied { include!(\"records.rs\"); }",
        "pub mod r#type { include!(\"records.rs\"); }",
    ] {
        fixture.write("src/lib.rs", &format!("{direct}\n{copied}"));
        failure(
            &fixture.cargo(&["build", "--lib", "--release", "--locked", "--offline"]),
            "unsupported history declaration",
        );
        failure(
            &fixture.cargo_env(
                &["check", "--lib", "--locked", "--offline"],
                &[("TYPE_HISTORY_REQUIRE_FROZEN", Some("1"))],
            ),
            "unsupported history declaration",
        );
        failure(&fixture.cli(&["check"]), "unsupported history declaration");
    }
    // Attribute macros can copy original source spans into a function or const.
    // These scopes have the same module_path!(), but declare distinct types.
    fixture.write(
        "copy-source/Cargo.toml",
        "[package]\nname='copy-source'\nversion='0.0.0'\nedition='2024'\n[lib]\nproc-macro=true\n",
    );
    fixture.write("copy-source/src/lib.rs", COPY_SOURCE);
    let mut manifest: Value = toml::from_str(&fixture.read("Cargo.toml")).unwrap();
    manifest["dependencies"].as_table_mut().unwrap().insert(
        "copy-source".into(),
        toml::from_str::<Value>("path='copy-source'").unwrap(),
    );
    fixture.write("Cargo.toml", &toml::to_string(&manifest).unwrap());
    success(&fixture.cargo(&["generate-lockfile", "--offline"]));
    for block in ["function", "constant"] {
        fixture.write(
            "src/lib.rs",
            &format!("#[copy_source::duplicate({block})] pub mod records {{ {V1} }}"),
        );
        failure(
            &fixture.cargo(&["build", "--lib", "--release", "--locked", "--offline"]),
            "unsupported history declaration",
        );
    }
    fixture.write("src/lib.rs", "#[path=\"records.rs\"] pub mod r#type;");
    success(&fixture.cargo(&["build", "--lib", "--release", "--locked", "--offline"]));
    manifest.as_table_mut().unwrap().insert(
        "lib".into(),
        toml::from_str::<Value>("name='custom_history'").unwrap(),
    );
    fixture.write("Cargo.toml", &toml::to_string(&manifest).unwrap());
    success(&fixture.cargo(&["build", "--lib", "--release", "--locked", "--offline"]));
    fixture.write("src/main.rs", "include!(\"lib.rs\"); fn main() {}");
    failure(
        &fixture.cargo(&["build", "--bins", "--release", "--locked", "--offline"]),
        "unsupported history declaration",
    );
}

#[test]
fn renamed_copies_cannot_reuse_a_discovered_source_position() {
    let fixture = Fixture::frozen();
    fixture.write(
        "copy-source/Cargo.toml",
        "[package]\nname='copy-source'\nversion='0.0.0'\nedition='2024'\n[lib]\nproc-macro=true\n",
    );
    fixture.write("copy-source/src/lib.rs", COPY_SOURCE);
    let mut manifest: Value = toml::from_str(&fixture.read("Cargo.toml")).unwrap();
    manifest["dependencies"].as_table_mut().unwrap().insert(
        "copy-source".into(),
        toml::from_str::<Value>("path='copy-source'").unwrap(),
    );
    fixture.write("Cargo.toml", &toml::to_string(&manifest).unwrap());
    success(&fixture.cargo(&["generate-lockfile", "--offline"]));
    let direct = V1
        .replace("use history_api::versioned;", "")
        .replace("#[versioned(", "#[history_api::versioned(");
    fixture.write(
        "src/lib.rs",
        &format!("#[copy_source::duplicate(renamed)] pub mod records {{ {direct} }}"),
    );
    failure(
        &fixture.cargo(&["build", "--lib", "--release", "--locked", "--offline"]),
        "unsupported history declaration",
    );
}

const COPY_SOURCE: &str = r#"
use proc_macro::{Delimiter, Group, Ident, TokenStream, TokenTree};
#[proc_macro_attribute]
pub fn duplicate(kind: TokenStream, item: TokenStream) -> TokenStream {
    let mut item: Vec<_> = item.into_iter().collect();
    let TokenTree::Group(body) = item.pop().unwrap() else { panic!("module body") };
    let source = body.stream();
    if kind.to_string() == "renamed" {
        let mut name_next = false;
        let copied: TokenStream = source.clone().into_iter().map(|token| match token {
            TokenTree::Ident(name) if name_next => {
                name_next = false;
                TokenTree::Ident(Ident::new(&format!("Copy{name}"), name.span()))
            }
            TokenTree::Ident(name) if name.to_string() == "struct" => {
                name_next = true;
                TokenTree::Ident(name)
            }
            token => token,
        }).collect();
        item.push(TokenTree::Group(Group::new(Delimiter::Brace, source.into_iter().chain(copied).collect())));
        return item.into_iter().collect();
    }
    let wrapper = if kind.to_string() == "function" { "pub fn local() {}" } else { "const _: () = {};" };
    let wrapper: TokenStream = wrapper.parse().unwrap();
    let wrapper: TokenStream = wrapper.into_iter().map(|token| match token {
        TokenTree::Group(group) if group.delimiter() == Delimiter::Brace =>
            TokenTree::Group(Group::new(Delimiter::Brace, source.clone())),
        token => token,
    }).collect();
    let body: TokenStream = source.into_iter().chain(wrapper).collect();
    item.push(TokenTree::Group(Group::new(Delimiter::Brace, body)));
    item.into_iter().collect()
}
"#;

#[test]
fn ordinary_module_builds_stay_fresh_and_detect_new_alternatives() {
    let fixture = Fixture::frozen();
    fixture.write("src/lib.rs", "pub mod records;");
    fixture.write("src/records.rs", V1);
    let arguments = ["check", "--lib", "--locked", "--offline", "-vv"];
    success(&fixture.cargo(&arguments));
    let unchanged = fixture.cargo(&arguments);
    success(&unchanged);
    let output = text(&unchanged);
    assert!(
        output.contains("Fresh standalone-history-consumer"),
        "{output}"
    );
    assert!(!output.contains("Running "), "{output}");

    let messages = fixture.cargo(&[
        "check",
        "--lib",
        "--locked",
        "--offline",
        "--message-format=json",
    ]);
    success(&messages);
    let admission = String::from_utf8(messages.stdout)
        .unwrap()
        .lines()
        .filter_map(|line| serde_json::from_str::<JsonValue>(line).ok())
        .filter(|message| message["reason"] == "build-script-executed")
        .flat_map(|message| message["env"].as_array().cloned().unwrap_or_default())
        .find(|entry| entry[0] == "TYPE_HISTORY_ADMISSION")
        .expect("admission environment")[1]
        .as_str()
        .unwrap()
        .to_owned();
    let modified = fs::metadata(&admission).unwrap().modified().unwrap();
    fixture.write("src/unrelated.txt", "trigger the watched source directory");
    success(&fixture.cargo(&arguments));
    assert_eq!(
        fs::metadata(&admission).unwrap().modified().unwrap(),
        modified
    );

    fixture.write("src/records/mod.rs", V1);
    failure(&fixture.cargo(&arguments), "ambiguous");
    fixture.remove("src/records/mod.rs");
    success(&fixture.cargo(&arguments));
    let unchanged = fixture.cargo(&arguments);
    success(&unchanged);
    assert!(text(&unchanged).contains("Fresh standalone-history-consumer"));
    fixture.write("src/records.rs", &V1.replace("u32", "String"));
    failure(&fixture.cargo(&arguments), "frozen");
}

#[cfg(unix)]
#[test]
fn symlinked_sources_keep_the_authored_module_directory() {
    use std::os::unix::fs::symlink;

    let fixture = Fixture::frozen();
    fixture.write("src/physical/lib.rs", "pub mod records;");
    fixture.write("src/records.rs", V1);
    fixture.remove("src/lib.rs");
    symlink("physical/lib.rs", fixture.root().join("src/lib.rs")).unwrap();
    success(&fixture.cargo(&["build", "--lib", "--release", "--locked", "--offline"]));
    success(&fixture.cli(&["check"]));

    fixture.write("src/physical/records.rs", "pub mod nested;");
    fixture.write("src/records/nested.rs", V1);
    fixture.remove("src/records.rs");
    symlink("physical/records.rs", fixture.root().join("src/records.rs")).unwrap();
    success(&fixture.cargo(&["build", "--lib", "--release", "--locked", "--offline"]));
    success(&fixture.cli(&["check"]));
    fixture.write("src/records/nested.rs", &V1.replace("u32", "String"));
    failure(
        &fixture.cargo(&["check", "--lib", "--locked", "--offline"]),
        "frozen",
    );
}

#[test]
fn excluded_package_snapshot_keeps_its_own_workspace_boundary() {
    let fixture = Fixture::frozen();
    let ledger = fixture.read(LEDGER);
    let mut manifest: Value = toml::from_str(&fixture.read("Cargo.toml")).unwrap();
    manifest.as_table_mut().unwrap().remove("workspace");
    fixture.write("Cargo.toml", &toml::to_string(&manifest).unwrap());
    let parent = fixture.root().parent().unwrap();
    fs::write(
        parent.join("Cargo.toml"),
        "[workspace]\nmembers=[]\nexclude=['consumer', 'vendor']\n",
    )
    .unwrap();
    let original_manifest = fixture.read("Cargo.toml");
    success(&fixture.cargo(&["check", "--lib", "--locked", "--offline"]));
    success(&fixture.cli_env(&["check"], &[("TMPDIR", Some(parent.to_str().unwrap()))]));
    assert_eq!(fixture.read("Cargo.toml"), original_manifest);
    assert_eq!(fixture.read(LEDGER), ledger);
    fixture.write("src/lib.rs", &V1.replace("u32", "String"));
    failure(
        &fixture.cli_env(&["check"], &[("TMPDIR", Some(parent.to_str().unwrap()))]),
        "frozen",
    );
}
