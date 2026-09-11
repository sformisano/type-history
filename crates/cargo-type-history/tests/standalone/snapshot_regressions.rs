use super::support::{success, Fixture, LEDGER};
use std::fs;
use toml::Value;

#[cfg(unix)]
#[test]
fn symlinked_config_and_workspace_local_tmpdir_preserve_frozen_checks() {
    use std::os::unix::fs::symlink;

    let fixture = Fixture::frozen();
    let ledger = fixture.read(LEDGER);
    fixture.write(".cargo/config.toml", "");
    success(&fixture.cli(&["check"]));
    fixture.write("shared-config.toml", "");
    fixture.remove(".cargo/config.toml");
    let config = fixture.root().join(".cargo/config.toml");
    symlink("../shared-config.toml", &config).unwrap();
    success(&fixture.cli(&["check"]));
    fixture.remove(".cargo/config.toml");
    symlink(fixture.root().join("shared-config.toml"), &config).unwrap();
    success(&fixture.cli(&["check"]));

    let temporary = fixture.root().join("scratch");
    fs::create_dir_all(&temporary).unwrap();
    fixture.write("scratch/user-input.txt", "keep this source input");
    success(&fixture.cli_env(&["check"], &[("TMPDIR", Some(temporary.to_str().unwrap()))]));
    let remaining: Vec<_> = fs::read_dir(&temporary)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(remaining, ["user-input.txt"]);
    assert_eq!(fixture.read(LEDGER), ledger);
}

#[test]
fn absolute_cargo_members_and_build_scripts_use_the_snapshot() {
    let fixture = Fixture::frozen();
    let ledger = fixture.read(LEDGER);
    // This check passes in ordinary Cargo and for a relative copied build.rs.
    // A live absolute build script paired with a copied manifest fails it.
    fixture.write(
        "build.rs",
        r#"
use std::env;
use std::path::Path;
fn main() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let script = Path::new(file!());
    assert!(script.is_relative() || script.starts_with(&manifest_dir),
        "build script must come from the same captured package as its manifest");
    history_build::compile();
}
"#,
    );
    success(&fixture.cargo(&["build", "--locked", "--offline"]));
    success(&fixture.cli(&["check"]));

    let mut manifest: Value = toml::from_str(&fixture.read("Cargo.toml")).unwrap();
    manifest["package"]["build"] = fixture.root().join("build.rs").to_str().unwrap().into();
    // Cargo leaves arbitrary metadata to its consumer, including absolute paths.
    manifest["package"].as_table_mut().unwrap().insert(
        "metadata".into(),
        toml::from_str::<Value>(
            "[custom]\npath = '/custom/data'\nworkspace = '/custom/workspace'\n",
        )
        .unwrap(),
    );
    fixture.write("Cargo.toml", &toml::to_string(&manifest).unwrap());
    success(&fixture.cargo(&["build", "--locked", "--offline"]));
    success(&fixture.cli(&["check"]));

    fixture.write(
        "members/plain/Cargo.toml",
        "[package]\nname = 'plain-workspace-member'\nversion = '0.0.0'\nedition = '2024'\n",
    );
    fixture.write("members/plain/src/lib.rs", "pub struct Plain;\n");
    let member = fixture
        .root()
        .join("members/plain")
        .to_str()
        .unwrap()
        .to_owned();
    let workspace = manifest["workspace"].as_table_mut().unwrap();
    workspace.insert("members".into(), Value::Array(vec![member.into()]));
    workspace.insert(
        "default-members".into(),
        Value::Array(vec![fixture.root().to_str().unwrap().into()]),
    );
    workspace.insert(
        "exclude".into(),
        Value::Array(vec![fixture
            .root()
            .join("members/excluded")
            .to_str()
            .unwrap()
            .into()]),
    );
    fixture.write("Cargo.toml", &toml::to_string(&manifest).unwrap());
    success(&fixture.cargo(&["generate-lockfile", "--offline"]));
    success(&fixture.cargo(&["metadata", "--format-version", "1", "--locked", "--offline"]));
    success(&fixture.cli(&["check"]));
    assert_eq!(fixture.read(LEDGER), ledger);
}
