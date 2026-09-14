use super::support::{failure, success, Fixture, LEDGER, V1};
use std::env;
use std::ffi::OsString;
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use toml::{Table, Value};
use type_history_build::contract::STANDALONE;
use type_history_build::options::parse;

const BACKEND_GUARDS: &str = r#"
#[cfg(all(feature = "backend-default", feature = "backend-alt"))]
compile_error!("select exactly one backend");
#[cfg(not(any(feature = "backend-default", feature = "backend-alt")))]
compile_error!("select exactly one backend");
"#;

#[test]
fn parser_accepts_composed_no_default_selection() {
    let contract = &STANDALONE;
    let options = parse(
        [
            contract.command,
            "check",
            "--no-default-features",
            "--features",
            "backend-alt",
            "--target",
            "x86_64-unknown-linux-gnu",
        ]
        .into_iter()
        .map(OsString::from),
        contract,
    )
    .unwrap()
    .unwrap();
    assert!(options.no_default_features);
    assert_eq!(options.features.as_deref(), Some("backend-alt"));
    assert_eq!(options.target.as_deref(), Some("x86_64-unknown-linux-gnu"));

    let duplicate = parse(
        ["check", "--no-default-features", "--no-default-features"]
            .into_iter()
            .map(OsString::from),
        contract,
    )
    .unwrap_err();
    assert_eq!(duplicate.to_string(), "duplicate --no-default-features");

    let default = parse([OsString::from("check")], contract).unwrap().unwrap();
    assert!(!default.no_default_features);
}

#[test]
#[cfg(unix)]
fn alternative_backend_reaches_every_cargo_stage_without_mutating_failures() {
    let fixture = Fixture::empty();
    configure_backends(&fixture);
    success(&fixture.cargo(&["generate-lockfile", "--offline"]));

    let real_cargo = executable_on_path("cargo");
    let fixture_parent = fixture.root().parent().unwrap();
    let shim_directory = fixture_parent.join("cargo-shim");
    let shim = shim_directory.join("cargo");
    let log = fixture_parent.join("cargo-arguments.log");
    for captured_root in [
        fixture.root().to_owned(),
        fixture_parent.join("vendor"),
        fixture_parent.join("default-backend"),
        fixture_parent.join("alternative-backend"),
    ] {
        assert!(!shim.starts_with(&captured_root));
        assert!(!log.starts_with(&captured_root));
    }
    fs::create_dir_all(&shim_directory).unwrap();
    fs::write(
        &shim,
        "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$CARGO_ARGUMENT_LOG\"\nexec \"$REAL_CARGO\" \"$@\"\n",
    )
    .unwrap();
    let mut permissions = fs::metadata(&shim).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&shim, permissions).unwrap();

    let path = env::join_paths(
        std::iter::once(shim_directory.clone())
            .chain(env::split_paths(&env::var_os("PATH").unwrap())),
    )
    .unwrap();
    let environment = [
        ("PATH", Some(path.to_str().unwrap())),
        ("REAL_CARGO", Some(real_cargo.to_str().unwrap())),
        ("CARGO_ARGUMENT_LOG", Some(log.to_str().unwrap())),
    ];
    let selection = [
        "--package",
        "standalone-history-consumer",
        "--no-default-features",
        "--features",
        "backend-alt",
    ];

    success(&fixture.cli_env(
        &[
            "init",
            selection[0],
            selection[1],
            selection[2],
            selection[3],
            selection[4],
        ],
        &environment,
    ));
    fixture.write("src/lib.rs", &format!("{BACKEND_GUARDS}\n{V1}"));
    success(&fixture.cli_env(
        &[
            "freeze",
            selection[0],
            selection[1],
            selection[2],
            selection[3],
            selection[4],
        ],
        &environment,
    ));
    success(&fixture.cli_env(
        &[
            "check",
            selection[0],
            selection[1],
            selection[2],
            selection[3],
            selection[4],
        ],
        &environment,
    ));

    let frozen = fixture.read(LEDGER);
    failure(
        &fixture.cli(&[
            "freeze",
            "--package",
            "standalone-history-consumer",
            "--features",
            "backend-alt",
        ]),
        "select exactly one backend",
    );
    assert_eq!(fixture.read(LEDGER), frozen);

    assert_forwarded_selection(&fs::read_to_string(log).unwrap());
}

#[test]
fn default_backend_preserves_existing_lifecycle_calls_and_public_callers() {
    let fixture = Fixture::empty();
    configure_backends(&fixture);
    success(&fixture.cargo(&["generate-lockfile", "--offline"]));
    success(&fixture.cli(&["init", "--package", "standalone-history-consumer"]));
    fixture.write("src/lib.rs", &format!("{BACKEND_GUARDS}\n{V1}"));
    success(&fixture.cli(&["freeze", "--package", "standalone-history-consumer"]));
    success(&fixture.cli(&["check"]));
}

fn configure_backends(fixture: &Fixture) {
    for package in ["default-backend", "alternative-backend"] {
        fixture.write(
            &format!("../{package}/Cargo.toml"),
            &format!(
                "[package]\nname = {package:?}\nversion = '0.0.0'\nedition = '2024'\npublish = false\n[workspace]\n"
            ),
        );
        fixture.write(&format!("../{package}/src/lib.rs"), "pub struct Backend;\n");
        success(&fixture.cargo_at(
            &fixture.root().parent().unwrap().join(package),
            &["generate-lockfile", "--offline"],
        ));
    }

    let mut manifest: Table = toml::from_str(&fixture.read("Cargo.toml")).unwrap();
    let dependencies = manifest["dependencies"].as_table_mut().unwrap();
    for (name, path) in [
        ("default-backend", "../default-backend"),
        ("alternative-backend", "../alternative-backend"),
    ] {
        dependencies.insert(
            name.into(),
            Table::from_iter([
                ("path".into(), path.into()),
                ("optional".into(), true.into()),
            ])
            .into(),
        );
    }
    manifest.insert(
        "features".into(),
        Table::from_iter([
            (
                "default".into(),
                Value::Array(vec!["backend-default".into()]),
            ),
            (
                "backend-default".into(),
                Value::Array(vec!["dep:default-backend".into()]),
            ),
            (
                "backend-alt".into(),
                Value::Array(vec!["dep:alternative-backend".into()]),
            ),
        ])
        .into(),
    );
    fixture.write("Cargo.toml", &toml::to_string(&manifest).unwrap());
    fixture.write(
        "build.rs",
        r#"
use history_build::contract::STANDALONE;
use history_build::options::{parse, Action, LifecycleOptions};
use history_build::workspace::{self, CargoMetadata};
use history_build::Result;
use std::ffi::OsString;

#[allow(dead_code)]
fn public_api_compatibility() {
    let _: Result<Option<LifecycleOptions>> = parse(std::iter::empty::<OsString>(), &STANDALONE);
    let _: Result<CargoMetadata> = workspace::metadata(None);
    let _ = LifecycleOptions {
        action: Action::Check,
        package: None,
        selected: None,
        undo: false,
        from: None,
        features: None,
        no_default_features: false,
        target: None,
    };
}

fn main() {
    history_build::compile();
}
"#,
    );
}

fn executable_on_path(name: &str) -> PathBuf {
    let candidate = env::split_paths(&env::var_os("PATH").unwrap())
        .map(|directory| directory.join(name))
        .find(|candidate| candidate.is_file())
        .unwrap();
    if candidate.is_absolute() {
        candidate
    } else {
        env::current_dir().unwrap().join(candidate)
    }
}

fn assert_forwarded_selection(log: &str) {
    let lines = log.lines().collect::<Vec<_>>();
    let active_metadata = lines
        .iter()
        .filter(|line| line.starts_with("metadata ") && !line.contains("--no-deps"))
        .collect::<Vec<_>>();
    assert!(!active_metadata.is_empty());
    assert!(active_metadata.iter().all(|line| has_selection(line)));

    let inactive_metadata = lines
        .iter()
        .filter(|line| line.starts_with("metadata ") && line.contains("--no-deps"))
        .collect::<Vec<_>>();
    assert!(!inactive_metadata.is_empty());
    assert!(inactive_metadata
        .iter()
        .all(|line| !line.contains("--no-default-features") && !line.contains("--features")));

    let exports = lines
        .iter()
        .filter(|line| line.starts_with("test --lib "))
        .collect::<Vec<_>>();
    let validations = lines
        .iter()
        .filter(|line| line.starts_with("check --lib "))
        .collect::<Vec<_>>();
    assert!(!exports.is_empty());
    assert!(!validations.is_empty());
    assert!(exports.iter().all(|line| has_selection(line)));
    assert!(validations.iter().all(|line| has_selection(line)));
}

fn has_selection(arguments: &str) -> bool {
    arguments.contains("--no-default-features") && arguments.contains("--features backend-alt")
}
