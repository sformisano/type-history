//! Public transaction failures preserve live authority and hide staged candidates.

use super::support::{failure, success, text, Fixture, LEDGER, STABLE_NAME, V1, V2};
use serde_json::Value;
use std::env;
use std::fs::{self, Permissions};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Output, Stdio};
use std::time::{Duration, Instant};
use tempfile::Builder as TempBuilder;

const PACKAGE: &str = "standalone-history-consumer";

struct Running {
    child: Option<Child>,
    release: PathBuf,
}

#[cfg(unix)]
struct RestorePermissions {
    path: PathBuf,
    permissions: Permissions,
}

#[cfg(unix)]
impl RestorePermissions {
    fn capture(path: &Path) -> Self {
        Self {
            path: path.to_owned(),
            permissions: fs::metadata(path).unwrap().permissions(),
        }
    }
}

#[cfg(unix)]
impl Drop for RestorePermissions {
    fn drop(&mut self) {
        let _ = fs::set_permissions(&self.path, self.permissions.clone());
    }
}

impl Running {
    fn finish(mut self) -> Output {
        fs::write(&self.release, b"continue").unwrap();
        self.child.take().unwrap().wait_with_output().unwrap()
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        let _ = fs::write(&self.release, b"continue");
        if let Some(child) = self.child.take() {
            let _ = child.wait_with_output();
        }
    }
}

#[test]
fn public_freeze_detects_live_input_changes_and_cleans_owned_snapshot() {
    let fixture = Fixture::frozen();
    fixture.write("src/lib.rs", V2);
    let source = fixture.read("src/lib.rs");
    let authority = fixture.read(LEDGER);
    let scratch = TempBuilder::new()
        .prefix("type-history-transaction-")
        .tempdir()
        .unwrap();

    let source_path = fixture.root().join("src/lib.rs");
    fixture.write(
        "build.rs",
        &mutating_build_script(&source_path, "// concurrent source edit\n"),
    );
    failure(
        &fixture.cli_env(
            &["freeze", "--package", PACKAGE],
            &[
                ("TMPDIR", Some(scratch.path().to_str().unwrap())),
                ("TYPE_HISTORY_PRIVATE_SNAPSHOT", Some("1")),
            ],
        ),
        "InputsChanged",
    );
    assert_eq!(fixture.read(LEDGER), authority);
    assert_owned_state_clean(&fixture, scratch.path());

    fixture.write("src/lib.rs", &source);
    let config_path = fixture.root().join(".cargo/config.toml");
    let config = "# original effective configuration\n";
    fixture.write(".cargo/config.toml", config);
    fixture.write(
        "build.rs",
        &mutating_build_script(&config_path, "# concurrent configuration edit\n"),
    );
    failure(
        &fixture.cli_env(
            &["freeze", "--package", PACKAGE],
            &[
                ("TMPDIR", Some(scratch.path().to_str().unwrap())),
                ("TYPE_HISTORY_PRIVATE_SNAPSHOT", Some("1")),
            ],
        ),
        "InputsChanged",
    );
    assert_eq!(fixture.read(LEDGER), authority);
    assert_eq!(
        fixture.read(".cargo/config.toml"),
        "# original effective configuration\n# concurrent configuration edit\n"
    );
    assert_owned_state_clean(&fixture, scratch.path());
}

#[cfg(unix)]
#[test]
fn public_freeze_permission_failure_preserves_exact_authority() {
    let fixture = Fixture::frozen();
    fixture.write("src/lib.rs", V2);
    let authority = fixture.read(LEDGER);
    let directory = fixture.root().join("type-history");
    let restore = RestorePermissions::capture(&directory);
    fixture.write(
        "build.rs",
        &format!(
            r#"use std::fs::Permissions;
use std::os::unix::fs::PermissionsExt;
fn main() {{
    history_build::compile();
    std::fs::set_permissions({directory:?}, Permissions::from_mode(0o500)).unwrap();
}}
"#
        ),
    );

    let output = fixture.cli(&["freeze", "--package", PACKAGE]);
    drop(restore);
    failure(&output, "Permission denied");
    assert_eq!(fixture.read(LEDGER), authority);
    assert_no_candidate(&fixture);
}

#[cfg(unix)]
#[test]
fn public_export_transport_rejects_missing_and_corrupt_rows() {
    let fixture = Fixture::frozen();
    fixture.write("src/lib.rs", V2);
    let authority = fixture.read(LEDGER);
    let tools = TempBuilder::new()
        .prefix("type-history-export-transport-")
        .tempdir()
        .unwrap();
    let wrapper = tools.path().join("cargo");
    let cargo = quote(Path::new(env!("CARGO")));
    let path = env::join_paths(
        std::iter::once(tools.path().to_owned())
            .chain(env::split_paths(&env::var_os("PATH").unwrap())),
    )
    .unwrap();

    for (filter, expected) in [
        (
            "sed '/TYPE_HISTORY_SCHEMA_EXPORT_V2/d'",
            "incomplete history export",
        ),
        ("sed 's/\"kind\":\"u32\"/\"kind\":\"u64\"/g'", "frozen"),
        ("sed 's/{\"kind\":\"u32\"}/{\"kind\":\"tuple\",\"items\":[]}/g'", "tuples require"),
        ("sed 's/{\"name\":\"count\",\"presence\":\"required\",\"schema\":{\"kind\":\"u32\"}}/{\"name\":\"count\",\"presence\":\"optional\",\"schema\":{\"kind\":\"u32\"}},{\"name\":\"count\",\"presence\":\"optional\",\"schema\":{\"kind\":\"u32\"}}/g'", "record fields must have unique names"),
    ] {
        fs::write(
            &wrapper,
            format!(
                "#!/usr/bin/env bash\nset -euo pipefail\nif [[ ${{1-}} == test ]]; then\n  {cargo} \"$@\" | {filter}\nelse\n  exec {cargo} \"$@\"\nfi\n"
            ),
        )
        .unwrap();
        fs::set_permissions(&wrapper, Permissions::from_mode(0o755)).unwrap();
        failure(
            &fixture.cli_env(
                &["freeze", "--package", PACKAGE],
                &[("PATH", Some(path.to_str().unwrap()))],
            ),
            expected,
        );
        assert_eq!(fixture.read(LEDGER), authority);
        assert_no_candidate(&fixture);
    }
}

#[test]
fn reset_candidate_is_invisible_until_public_commit() {
    let fixture = Fixture::frozen();
    let before = fixture.read(LEDGER);
    let controls = TempBuilder::new()
        .prefix("type-history-reset-watcher-")
        .tempdir()
        .unwrap();
    let entered = controls.path().join("export-entered");
    let release = controls.path().join("continue");
    fixture.write("src/lib.rs", &V1.replace("count: u32", "count: u64"));
    fixture.write(
        "build.rs",
        &format!(
            r#"use std::path::Path;
use std::time::{{Duration, Instant}};
fn main() {{
    history_build::compile();
    if std::env::var_os("TYPE_HISTORY_SCHEMA_EXPORT").is_some() {{
        std::fs::write({entered:?}, b"snapshot compilation").unwrap();
        let started = Instant::now();
        while !Path::new({release:?}).exists() {{
            assert!(started.elapsed() < Duration::from_secs(600), "watcher barrier timed out");
            std::thread::sleep(Duration::from_millis(10));
        }}
    }}
}}
"#
        ),
    );
    let child = fixture
        .cli_command_env(
            &[
                "reset",
                "--package",
                PACKAGE,
                "--type",
                STABLE_NAME,
                "--version",
                "1",
            ],
            &[],
        )
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut running = Running {
        child: Some(child),
        release,
    };
    let started = Instant::now();
    while !entered.exists() {
        assert!(
            running
                .child
                .as_mut()
                .unwrap()
                .try_wait()
                .unwrap()
                .is_none(),
            "CLI exited before its exporter barrier"
        );
        assert!(
            started.elapsed() < Duration::from_secs(600),
            "CLI did not reach exporter barrier"
        );
        std::thread::sleep(Duration::from_millis(10));
    }

    assert_eq!(fixture.read(LEDGER), before);
    failure(
        &fixture.cargo(&["check", "--locked", "--offline"]),
        "frozen",
    );
    assert_eq!(fixture.read(LEDGER), before);
    success(&running.finish());
    let after: Value = serde_json::from_str(&fixture.read(LEDGER)).unwrap();
    assert_eq!(after[STABLE_NAME]["1"]["reset_draft"], true);
    let check = fixture.cargo(&["check", "--locked", "--offline"]);
    success(&check);
    assert!(text(&check).to_lowercase().contains("draft"));
}

fn mutating_build_script(path: &Path, appended: &str) -> String {
    format!(
        r#"fn main() {{
    history_build::compile();
    let mut input = std::fs::read_to_string({path:?}).unwrap();
    if !input.contains({appended:?}) {{
        input.push_str({appended:?});
        std::fs::write({path:?}, input).unwrap();
    }}
}}
"#
    )
}

fn assert_owned_state_clean(fixture: &Fixture, scratch: &Path) {
    assert_eq!(
        fs::read_dir(scratch).unwrap().count(),
        0,
        "owned scratch must be removed after child exit"
    );
    assert_no_candidate(fixture);
}

fn assert_no_candidate(fixture: &Fixture) {
    assert!(fs::read_dir(fixture.root().join("type-history"))
        .unwrap()
        .all(|entry| !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".history-candidate-")));
}

#[cfg(unix)]
fn quote(path: &Path) -> String {
    format!("'{}'", path.to_str().unwrap().replace('\'', "'\\''"))
}
