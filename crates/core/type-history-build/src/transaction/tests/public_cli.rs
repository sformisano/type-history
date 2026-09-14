//! The production public entry propagates private test-only I/O faults through a subprocess.
use super::Fault;
use crate::contract::STANDALONE;
use crate::lifecycle::{run, LifecycleOps};
use crate::transaction::CommitOperation;
use std::env;
use std::ffi::OsString;
use std::fs;
use std::process::Command;
use tempfile::Builder as TempBuilder;

const SELECTOR: &str = "TYPE_HISTORY_TEST_ONLY_CLI_IO_OPERATION";

#[test]
fn child_calls_public_cli_with_a_private_fault() {
    let Ok(operation) = env::var(SELECTOR) else {
        return;
    };
    let _fault = match operation.as_str() {
        "write" => Some(Fault::at(CommitOperation::Write)),
        "flush" => Some(Fault::at(CommitOperation::Flush)),
        "rename" => Some(Fault::at(CommitOperation::Rename)),
        "readback" => Some(Fault::at(CommitOperation::Readback)),
        "directory-sync" => Some(Fault::at(CommitOperation::DirectorySync)),
        "none" => None,
        _ => panic!("unknown private test operation"),
    };
    match run(
        ["init", "--package", "io-fixture"]
            .into_iter()
            .map(OsString::from),
        &STANDALONE,
        &LifecycleOps {
            discover: crate::discover::read,
            decode_export: crate::standalone_export::decode,
        },
    ) {
        Ok(()) => {}
        Err(error) => {
            eprintln!("cargo-type-history: {error}");
            std::process::exit(2);
        }
    }
}

#[test]
fn atomic_failures_public_cli_propagates_precommit_and_committed_errors() {
    for operation in [
        "write",
        "flush",
        "rename",
        "readback",
        "directory-sync",
        "none",
    ] {
        let root = TempBuilder::new()
            .prefix("type-history-cli-io-")
            .tempdir()
            .unwrap();
        let scratch = TempBuilder::new()
            .prefix("type-history-cli-io-scratch-")
            .tempdir()
            .unwrap();
        fs::create_dir(root.path().join("src")).unwrap();
        fs::write(root.path().join("Cargo.toml"),"[package]\nname = \"io-fixture\"\nversion = \"0.0.0\"\nedition = \"2024\"\nbuild = \"build.rs\"\n[workspace]\n").unwrap();
        fs::write(root.path().join("build.rs"), "fn main() {}\n").unwrap();
        fs::write(root.path().join("src/lib.rs"), "").unwrap();
        let lock = Command::new(env!("CARGO"))
            .current_dir(root.path())
            .args(["generate-lockfile", "--offline"])
            .output()
            .unwrap();
        assert!(
            lock.status.success(),
            "{}",
            String::from_utf8_lossy(&lock.stderr)
        );
        let output = Command::new(env::current_exe().unwrap())
            .current_dir(root.path())
            .args([
                "--exact",
                "transaction::tests::public_cli::child_calls_public_cli_with_a_private_fault",
                "--nocapture",
            ])
            .env(SELECTOR, operation)
            .env("TMPDIR", scratch.path())
            .env_remove("TYPE_HISTORY_REQUIRE_FROZEN")
            .output()
            .unwrap();
        let text = format!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let path = root.path().join("type-history/schemas.json");
        match operation {
            "write" | "flush" | "rename" => {
                assert_eq!(output.status.code(), Some(2), "{operation}: {text}");
                assert!(text.contains("cargo-type-history: injected"), "{text}");
                assert!(!text.contains("COMMITTED"), "{text}");
                assert!(
                    !path.exists(),
                    "precommit failure must preserve the exact absent original"
                );
            }
            "readback" | "directory-sync" => {
                assert_eq!(output.status.code(), Some(2), "{operation}: {text}");
                assert!(text.contains("cargo-type-history: COMMITTED:"), "{text}");
                assert_eq!(fs::read(&path).unwrap(), b"{}\n");
            }
            "none" => {
                assert!(output.status.success(), "{text}");
                assert_eq!(fs::read(&path).unwrap(), b"{}\n");
            }
            _ => unreachable!(),
        }
        assert!(
            text.contains("running 1 test"),
            "the selected public-entry worker must execute: {text}"
        );
        assert_eq!(
            fs::read_dir(scratch.path()).unwrap().count(),
            0,
            "all child-owned snapshot output must be gone"
        );
    }
}
