//! Deterministic I/O faults use the same commit function as the public transaction.
use super::{encoded, CommitOperation, Transaction, IO_FAULT};
use crate::contract::STANDALONE;
use type_history_codegen::ledger::{HistoryLedger, RecordMetadata, SchemaIdentity};
fn empty() -> HistoryLedger<RecordMetadata> {
    HistoryLedger::empty(SchemaIdentity::new(STANDALONE.schema_id_prefix))
}
use std::fs;
use std::fs::Permissions;
use tempfile::Builder as TempBuilder;

#[cfg(unix)]
#[test]
fn authority_directory_cannot_redirect_a_transaction_to_another_package() {
    use std::os::unix::fs::symlink;
    let root = tempfile::tempdir().unwrap();
    let other = tempfile::tempdir().unwrap();
    symlink(other.path(), root.path().join("type-history")).unwrap();
    let Err(error) = Transaction::acquire(root.path(), "redirected", &STANDALONE) else {
        panic!("symlinked package authority must be rejected");
    };
    assert!(error
        .to_string()
        .contains("directory must not be a symlink"));
    assert_eq!(fs::read_dir(other.path()).unwrap().count(), 0);
}

#[cfg(unix)]
#[test]
fn committing_a_candidate_preserves_existing_ledger_permissions() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("type-history")).unwrap();
    let path = root.path().join("type-history/schemas.json");
    fs::write(&path, b" { } \n").unwrap();
    fs::set_permissions(&path, Permissions::from_mode(0o640)).unwrap();
    let transaction = Transaction::acquire(root.path(), "permissions", &STANDALONE).unwrap();
    transaction.commit_checked(&empty(), |_| Ok(())).unwrap();
    assert_eq!(
        fs::metadata(path).unwrap().permissions().mode() & 0o777,
        0o640
    );
}

struct Fault;
impl Fault {
    fn at(operation: CommitOperation) -> Self {
        IO_FAULT.with(|fault| fault.set(Some(operation)));
        Self
    }
}
impl Drop for Fault {
    fn drop(&mut self) {
        IO_FAULT.with(|fault| fault.set(None));
    }
}

#[test]
fn atomic_failures_before_rename_keep_exact_original_bytes_and_cleanup() {
    for operation in [
        CommitOperation::Write,
        CommitOperation::Flush,
        CommitOperation::Rename,
    ] {
        let root = TempBuilder::new()
            .prefix("history-commit-")
            .tempdir()
            .unwrap();
        fs::create_dir(root.path().join("type-history")).unwrap();
        let original = b" { } \n";
        let path = root.path().join("type-history/schemas.json");
        fs::write(&path, original).unwrap();
        let transaction = Transaction::acquire(root.path(), "fault-fixture", &STANDALONE).unwrap();
        let _fault = Fault::at(operation);
        let error = transaction
            .commit_checked(&empty(), |_| Ok(()))
            .unwrap_err()
            .to_string();
        assert!(!error.contains("COMMITTED"), "{error}");
        assert_eq!(fs::read(&path).unwrap(), original);
        assert!(fs::read_dir(path.parent().unwrap()).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".history-candidate-")
        }));
    }
}
#[test]
fn atomic_failures_after_rename_report_committed_and_keep_exact_candidate() {
    for operation in [CommitOperation::Readback, CommitOperation::DirectorySync] {
        let root = TempBuilder::new()
            .prefix("history-commit-")
            .tempdir()
            .unwrap();
        fs::create_dir(root.path().join("type-history")).unwrap();
        let path = root.path().join("type-history/schemas.json");
        fs::write(&path, b" { } \n").unwrap();
        let transaction = Transaction::acquire(root.path(), "fault-fixture", &STANDALONE).unwrap();
        let _fault = Fault::at(operation);
        let candidate = empty();
        let error = transaction
            .commit_checked(&candidate, |_| Ok(()))
            .unwrap_err()
            .to_string();
        assert!(error.starts_with("COMMITTED:"), "{error}");
        assert_eq!(fs::read(&path).unwrap(), encoded(&candidate).unwrap());
    }
}
#[test]
fn atomic_failures_candidate_freshness_and_original_readback_precede_commit() {
    let root = TempBuilder::new()
        .prefix("history-commit-")
        .tempdir()
        .unwrap();
    fs::create_dir(root.path().join("type-history")).unwrap();
    let path = root.path().join("type-history/schemas.json");
    let original = b" { } \n";
    fs::write(&path, original).unwrap();
    let transaction = Transaction::acquire(root.path(), "fault-fixture", &STANDALONE).unwrap();
    let error = transaction
        .commit_checked(&empty(), |_| Err("InputsChanged".into()))
        .unwrap_err();
    assert_eq!(error.to_string(), "InputsChanged");
    assert_eq!(fs::read(&path).unwrap(), original);
    let concurrent = b"\n{}\n";
    let error = transaction
        .commit_checked(&empty(), |_| {
            fs::write(&path, concurrent)?;
            Ok(())
        })
        .unwrap_err();
    assert!(error.to_string().contains("InputsChanged"));
    assert_eq!(fs::read(&path).unwrap(), concurrent);
}

#[path = "tests/public_cli.rs"]
mod public_cli;

#[test]
fn initialization_preserves_a_ledger_created_after_lock_acquisition() {
    let root = tempfile::tempdir().unwrap();
    let transaction = Transaction::acquire(root.path(), "init", &STANDALONE).unwrap();
    assert!(transaction.original().is_none());
    let concurrent = b"user-owned ledger bytes";
    let error = transaction
        .commit_checked(&empty(), |_| {
            fs::write(&transaction.path, concurrent)?;
            Ok(())
        })
        .unwrap_err();
    assert!(error.to_string().contains("InputsChanged"));
    assert_eq!(fs::read(&transaction.path).unwrap(), concurrent);
}
