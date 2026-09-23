use super::{copy_tree, Family, FamilyCache};
use std::cell::RefCell;
use std::env;
use std::fs;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex, Weak};
use std::thread;
use std::time::{Duration, Instant};
use tempfile::{Builder, TempDir};

const WAIT: Duration = Duration::from_secs(10);

fn family() -> Family {
    Family::create(|root| {
        fs::create_dir_all(root).unwrap();
        fs::write(root.join("source"), "controlled immutable source").unwrap();
    })
}

fn cache() -> FamilyCache {
    FamilyCache(Mutex::new(Weak::new()))
}

fn consumer() -> (TempDir, PathBuf) {
    let owner = Builder::new().prefix("resource-test-").tempdir().unwrap();
    let root = owner.path().join("consumer");
    fs::create_dir_all(&root).unwrap();
    (owner, root)
}

fn recipe(root: &Path, family: &Family) {
    fs::create_dir_all(root.join("src")).unwrap();
    fs::create_dir_all(root.join("type-history")).unwrap();
    fs::write(
        root.join("Cargo.toml"),
        format!("family = {:?}\n", family.root()),
    )
    .unwrap();
    fs::write(root.join("Cargo.lock"), "private original lock").unwrap();
    fs::write(root.join("build.rs"), "fn main() {}\n").unwrap();
    fs::write(root.join("src/lib.rs"), include_bytes!("../fixtures/v1.rs")).unwrap();
    fs::write(root.join("type-history/schemas.json"), "{\"baseline\":1}\n").unwrap();
    fs::write(root.join("type-history/.schemas.lock"), []).unwrap();
}

#[test]
fn queued_leases_release_lookup_before_serial_and_keep_family_alive() {
    let cache = Arc::new(cache());
    let serial = Arc::new(Mutex::new(()));
    let (first, guard) = cache.acquire(&serial, family);
    let owned = first.owner.path().to_owned();
    let expected = owned.clone();
    let worker_cache = Arc::clone(&cache);
    let worker_serial = Arc::clone(&serial);
    let (ready_send, ready_receive) = mpsc::channel();
    let (finish_send, finish_receive) = mpsc::channel();
    let (complete_send, complete_receive) = mpsc::channel();
    let worker = thread::spawn(move || {
        let (queued, guard) = worker_cache.acquire(&worker_serial, || panic!("live family"));
        assert_eq!(queued.owner.path(), expected);
        ready_send.send(()).unwrap();
        finish_receive.recv_timeout(WAIT).unwrap();
        assert!(queued.root().join("source").is_file());
        drop(queued);
        drop(guard);
        complete_send.send(()).unwrap();
    });
    let deadline = Instant::now() + WAIT;
    while Arc::strong_count(&first) < 2 && Instant::now() < deadline {
        thread::yield_now();
    }
    let retained_before_serial = Arc::strong_count(&first) >= 2;
    let lookup_released = loop {
        if cache.0.try_lock().is_ok() {
            break true;
        }
        if Instant::now() >= deadline {
            break false;
        }
        thread::yield_now();
    };
    drop(first);
    let survived_first_owner = owned.exists();
    // Unblock the worker even if the lock-order assertions will fail.
    drop(guard);
    let became_ready = ready_receive.recv_timeout(WAIT).is_ok();
    let _ = finish_send.send(());
    match complete_receive.recv_timeout(WAIT) {
        Ok(()) | Err(RecvTimeoutError::Disconnected) => worker.join().unwrap(),
        // A broken lock order must fail within the bound. The worker retains its
        // own resources if stuck; joining it here would conceal the deadlock.
        Err(RecvTimeoutError::Timeout) => {
            panic!("resource worker failed to finish within {WAIT:?}")
        }
    }
    assert!(
        retained_before_serial,
        "queued fixture must lease before SERIAL"
    );
    assert!(
        lookup_released,
        "lookup must not be held while waiting for SERIAL"
    );
    assert!(survived_first_owner && became_ready);
    assert!(
        !owned.exists(),
        "last owner must remove the actual family directory"
    );
    assert!(cache.0.lock().unwrap().upgrade().is_none());
}

#[test]
fn failed_source_copy_cleans_before_poisoned_lookup_retries() {
    let cache = cache();
    let attempted = RefCell::new(PathBuf::new());
    assert!(catch_unwind(AssertUnwindSafe(|| {
        cache.lease(|| {
            Family::create(|root| {
                *attempted.borrow_mut() = root.parent().unwrap().to_owned();
                fs::create_dir_all(root).unwrap();
                fs::write(root.join("partial"), "incomplete").unwrap();
                copy_tree(&root.join("missing-source"), &root.join("incomplete-copy"));
            })
        });
    }))
    .is_err());
    assert!(
        !attempted.borrow().exists(),
        "failed copy owner must already be gone"
    );
    assert!(cache.0.is_poisoned());
    let recovered = cache.lease(family);
    assert!(!cache.0.is_poisoned());
    assert!(recovered.root().join("source").is_file());
    assert!(!recovered.root().join("partial").exists());
    let owned = recovered.owner.path().to_owned();
    drop(recovered);
    assert!(!owned.exists());
}

#[test]
fn failed_cli_initialization_retries_after_serial_poison_without_partial_ready_state() {
    let cache = cache();
    let serial = Mutex::new(());
    let retained = cache.lease(family);
    let owned = retained.owner.path().to_owned();
    let (_private, destination) = consumer();
    let cli = destination.join("cli");
    for fail_build in [true, false] {
        assert!(catch_unwind(AssertUnwindSafe(|| {
            let (leased, _guard) = cache.acquire(&serial, || panic!("retain family"));
            leased.copy_cli(&cli, |root| {
                assert!(!fail_build, "controlled build failure");
                root.join("missing-executable")
            });
        }))
        .is_err());
        assert!(serial.is_poisoned());
        assert!(retained.cli.get().is_none());
        assert!(!cli.exists());
        assert_eq!(
            fs::read_dir(&owned).unwrap().count(),
            1,
            "failed CLI stage removed"
        );
    }
    let (leased, guard) = cache.acquire(&serial, || panic!("retain family"));
    leased.copy_cli(&cli, |_| env::current_exe().unwrap());
    assert!(cli.is_file());
    assert!(retained.cli.get().is_some());
    drop(leased);
    drop(guard);
    drop(retained);
    assert!(
        !owned.exists(),
        "failed attempts must not retain a family owner"
    );
}

#[test]
fn private_cli_mutation_preserves_other_copies_and_running_child() {
    let family = family();
    let owned = family.owner.path().to_owned();
    let (_first_owner, first) = consumer();
    let (_second_owner, second) = consumer();
    let first = first.join("cli");
    let second = second.join("cli");
    family.copy_cli(&first, |_| env::current_exe().unwrap());
    family.copy_cli(&second, |_| panic!("ready CLI must be reused"));
    let saved = family.cli.get().unwrap().path().join("cargo-type-history");
    assert!(!saved.starts_with(family.root()));
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        assert_ne!(
            fs::metadata(&first).unwrap().ino(),
            fs::metadata(&second).unwrap().ino()
        );
        assert_ne!(
            fs::metadata(&first).unwrap().ino(),
            fs::metadata(&saved).unwrap().ino()
        );
    }
    fs::rename(&first, first.with_extension("hidden")).unwrap();
    fs::write(first.with_extension("hidden"), "mutated private executable").unwrap();
    let child = Command::new(&second).arg("--list").output().unwrap();
    assert!(
        child.status.success(),
        "second executable must remain usable"
    );
    assert!(String::from_utf8(child.stdout)
        .unwrap()
        .contains("private_cli_mutation"));
    assert!(owned.exists(), "family retained until child completion");
    drop(family);
    assert!(!owned.exists());
}

#[test]
fn frozen_inputs_are_private_and_keep_each_consumer_lock_and_warm_build() {
    let family = family();
    let (_first_owner, first) = consumer();
    family.prepare_frozen(
        &first,
        || recipe(&first, &family),
        || panic!("recipe already built"),
    );
    let baseline_manifest = fs::read(first.join("Cargo.toml")).unwrap();
    fs::write(
        first.join("type-history/schemas.json"),
        "mutated first ledger",
    )
    .unwrap();
    fs::write(first.join("src/lib.rs"), "mutated first source").unwrap();
    fs::write(first.join("Cargo.toml"), "mutated first manifest").unwrap();
    let (_second_owner, second) = consumer();
    fs::write(second.join("Cargo.lock"), "second private lock").unwrap();
    let mut warmed = false;
    family.prepare_frozen(
        &second,
        || panic!("ready template must be reused"),
        || {
            assert_eq!(
                fs::read(second.join("Cargo.toml")).unwrap(),
                baseline_manifest
            );
            assert_eq!(
                fs::read(second.join("src/lib.rs")).unwrap(),
                include_bytes!("../fixtures/v1.rs")
            );
            assert_eq!(
                fs::read_to_string(second.join("type-history/schemas.json")).unwrap(),
                "{\"baseline\":1}\n"
            );
            warmed = true;
        },
    );
    assert!(
        warmed,
        "each materialized consumer must run its ordinary build"
    );
    assert_eq!(
        fs::read_to_string(second.join("Cargo.lock")).unwrap(),
        "second private lock"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        for path in ["type-history/schemas.json", "type-history/.schemas.lock"] {
            assert_ne!(
                fs::metadata(first.join(path)).unwrap().ino(),
                fs::metadata(second.join(path)).unwrap().ino()
            );
        }
    }
    fs::write(
        second.join("type-history/.schemas.lock"),
        "private mutation",
    )
    .unwrap();
    assert!(fs::read(first.join("type-history/.schemas.lock"))
        .unwrap()
        .is_empty());
    assert_eq!(
        fs::read_to_string(first.join("Cargo.toml")).unwrap(),
        "mutated first manifest"
    );
}

#[test]
fn failed_frozen_recipe_drops_private_owner_and_allows_retry() {
    let family = family();
    let attempted = RefCell::new(PathBuf::new());
    assert!(catch_unwind(AssertUnwindSafe(|| {
        let (private, root) = consumer();
        *attempted.borrow_mut() = private.path().to_owned();
        family.prepare_frozen(
            &root,
            || {
                recipe(&root, &family);
                panic!("controlled freeze failure");
            },
            || panic!("not ready"),
        );
    }))
    .is_err());
    assert!(
        !attempted.borrow().exists(),
        "failed consumer removed before test teardown"
    );
    assert!(family.frozen.get().is_none());
    let (_private, root) = consumer();
    family.prepare_frozen(&root, || recipe(&root, &family), || panic!("not ready"));
    assert!(family.frozen.get().is_some());
}

#[test]
fn frozen_capture_rejects_unexpected_inventory_and_private_paths() {
    let family = family();
    for bad_input in ["file", "directory", "path"] {
        let (private, root) = consumer();
        let owned = private.path().to_owned();
        assert!(catch_unwind(AssertUnwindSafe(|| {
            family.prepare_frozen(
                &root,
                || {
                    recipe(&root, &family);
                    match bad_input {
                        "file" => fs::write(root.join("unexpected"), "input").unwrap(),
                        "directory" => fs::create_dir(root.join("unexpected")).unwrap(),
                        "path" => fs::write(root.join("build.rs"), root.to_str().unwrap()).unwrap(),
                        _ => unreachable!(),
                    }
                },
                || panic!("not ready"),
            );
        }))
        .is_err());
        assert!(family.frozen.get().is_none());
        drop(private);
        assert!(!owned.exists());
    }
    let (_private, root) = consumer();
    family.prepare_frozen(&root, || recipe(&root, &family), || panic!("not ready"));
    assert!(family.frozen.get().is_some());
}

#[test]
fn expired_lookup_recreates_empty_family_for_serial_filtered_runs() {
    let cache = cache();
    let serial = Mutex::new(());
    let (first, guard) = cache.acquire(&serial, family);
    let first_path = first.owner.path().to_owned();
    let (_private, root) = consumer();
    first.prepare_frozen(&root, || recipe(&root, &first), || panic!("first recipe"));
    drop(first);
    drop(guard);
    assert!(!first_path.exists());
    let (second, guard) = cache.acquire(&serial, family);
    let second_path = second.owner.path().to_owned();
    assert_ne!(first_path, second_path);
    assert!(second.frozen.get().is_none());
    assert!(second.cli.get().is_none());
    assert!(second.root().join("source").is_file());
    drop(second);
    drop(guard);
    assert!(!second_path.exists());
}
