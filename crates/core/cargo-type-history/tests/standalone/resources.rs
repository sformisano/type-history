//! Scoped immutable prerequisites for independent standalone consumers.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, Weak};
use tempfile::{Builder, TempDir};
use toml::{Table, Value as TomlValue};

#[path = "resources/tests.rs"]
mod tests;

// The lookup cannot keep a directory alive after the last queued fixture exits.
static FAMILY: FamilyCache = FamilyCache(Mutex::new(Weak::new()));

struct FamilyCache(Mutex<Weak<Family>>);

impl FamilyCache {
    fn lease(&self, create: impl FnOnce() -> Family) -> Arc<Family> {
        let mut lookup = self.0.lock().unwrap_or_else(|error| error.into_inner());
        let family = lookup.upgrade().unwrap_or_else(|| Arc::new(create()));
        // Creation publishes only a complete source copy, including after poison.
        *lookup = Arc::downgrade(&family);
        self.0.clear_poison();
        family
    }

    fn acquire<'a>(
        &self,
        serial: &'a Mutex<()>,
        create: impl FnOnce() -> Family,
    ) -> (Arc<Family>, MutexGuard<'a, ()>) {
        let family = self.lease(create);
        // lease() releases the lookup before this wait. Retain its strong owner.
        let guard = serial.lock().unwrap_or_else(|error| error.into_inner());
        (family, guard)
    }
}

pub(super) fn acquire(serial: &Mutex<()>) -> (Arc<Family>, MutexGuard<'_, ()>) {
    FAMILY.acquire(serial, || Family::create(copy_family))
}

pub(super) struct Family {
    // Nested ready owners drop before the parent directory.
    cli: OnceLock<TempDir>,
    frozen: OnceLock<FrozenTemplate>,
    root: PathBuf,
    owner: TempDir,
}

impl Family {
    fn create(copy: impl FnOnce(&Path)) -> Self {
        let owner = Builder::new()
            .prefix("family-")
            .tempdir()
            .expect("owned family below the configured short TMPDIR");
        let root = owner.path().join("vendor");
        copy(&root);
        Self {
            cli: OnceLock::new(),
            frozen: OnceLock::new(),
            root,
            owner,
        }
    }

    pub(super) fn root(&self) -> &Path {
        &self.root
    }

    // Call only while holding the fixture's SERIAL guard, including initialization.
    pub(super) fn copy_cli(&self, destination: &Path, build: impl FnOnce(&Path) -> PathBuf) {
        let saved = self.cli.get_or_init(|| {
            let stage = Builder::new()
                .prefix("cli-")
                .tempdir_in(self.owner.path())
                .expect("owned CLI staging outside captured source");
            let built = build(&self.root);
            fs::copy(built, stage.path().join("cargo-type-history"))
                .expect("retain the independently built default-feature CLI");
            stage
        });
        // Ordinary copies preserve independent inodes for CLI rename/write tests.
        fs::copy(saved.path().join("cargo-type-history"), destination)
            .expect("private fixture CLI copy");
    }

    // The first initializer includes its ordinary build before production freeze.
    pub(super) fn prepare_frozen(
        &self,
        root: &Path,
        initialize: impl FnOnce(),
        warm: impl FnOnce(),
    ) {
        if let Some(template) = self.frozen.get() {
            template.materialize(root);
            warm();
        } else {
            self.frozen.get_or_init(|| {
                initialize();
                FrozenTemplate::capture(root)
            });
        }
    }
}

struct FrozenTemplate {
    inputs: Vec<(PathBuf, Vec<u8>)>,
}

impl FrozenTemplate {
    fn capture(root: &Path) -> Self {
        // Audit the real init/build/freeze output before publishing reusable bytes.
        let expected: BTreeSet<_> = [
            "Cargo.lock",
            "Cargo.toml",
            "build.rs",
            "src",
            "src/lib.rs",
            "type-history",
            "type-history/.schemas.lock",
            "type-history/schemas.json",
        ]
        .map(PathBuf::from)
        .into_iter()
        .collect();
        assert_eq!(
            inventory(root, root),
            expected,
            "frozen recipe output changed"
        );
        assert_eq!(
            fs::read(root.join("src/lib.rs")).expect("frozen source"),
            include_bytes!("fixtures/v1.rs"),
            "only the known V1 recipe can seed the template"
        );
        let private = root.parent().expect("private fixture owner");
        let private = private.to_str().expect("UTF-8 private owner").as_bytes();
        let mut inputs = Vec::new();
        for relative in [
            "Cargo.toml",
            "Cargo.lock",
            "build.rs",
            "src/lib.rs",
            "type-history/schemas.json",
            "type-history/.schemas.lock",
        ] {
            let bytes = fs::read(root.join(relative)).expect("checked frozen recipe input");
            assert!(
                !bytes.windows(private.len()).any(|window| window == private),
                "frozen template must not retain a private path in {relative}"
            );
            match relative {
                // Each consumer generated its own Cargo lock and must keep it.
                "Cargo.lock" => {}
                "type-history/.schemas.lock" => assert!(bytes.is_empty(), "empty lock file"),
                _ => inputs.push((PathBuf::from(relative), bytes)),
            }
        }
        Self { inputs }
    }

    fn materialize(&self, root: &Path) {
        for (relative, bytes) in &self.inputs {
            let path = root.join(relative);
            fs::create_dir_all(path.parent().expect("template input parent"))
                .expect("private template directories");
            fs::write(path, bytes).expect("private frozen input");
        }
        fs::write(root.join("type-history/.schemas.lock"), [])
            .expect("private empty authority lock");
    }
}

fn inventory(root: &Path, directory: &Path) -> BTreeSet<PathBuf> {
    let mut paths = BTreeSet::new();
    for entry in fs::read_dir(directory).expect("frozen recipe directory") {
        let entry = entry.expect("frozen recipe entry");
        let kind = entry.file_type().expect("frozen recipe entry kind");
        assert!(
            kind.is_file() || kind.is_dir(),
            "ordinary frozen recipe inputs"
        );
        paths.insert(
            entry
                .path()
                .strip_prefix(root)
                .expect("recipe relative path")
                .to_owned(),
        );
        if kind.is_dir() {
            paths.extend(inventory(root, &entry.path()));
        }
    }
    paths
}

fn copy_family(vendor: &Path) {
    let repository = repository();
    copy_tree(&repository.join("crates/core"), &vendor.join("crates/core"));
    copy_tree(&repository.join("book/src"), &vendor.join("book/src"));
    fs::copy(repository.join("README.md"), vendor.join("README.md"))
        .expect("preserve compiled tutorial source");
    let manifest: TomlValue = toml::from_str(
        &fs::read_to_string(repository.join("Cargo.toml")).expect("checked root manifest"),
    )
    .expect("root TOML");
    let mut dependencies = Table::new();
    for name in [
        "serde",
        "serde_json",
        "schemars",
        "thiserror",
        "sha2",
        "typed_floats",
        "uuid",
        "rust_decimal",
        "chrono",
        "time",
    ] {
        dependencies.insert(
            name.into(),
            manifest["workspace"]["dependencies"][name].clone(),
        );
    }
    let mut workspace = Table::new();
    workspace.insert(
        "members".into(),
        TomlValue::Array(
            [
                "type-history-core",
                "type-history-codegen",
                "type-history-macros",
                "type-history",
                "type-history-build",
                "cargo-type-history",
            ]
            .into_iter()
            .map(|name| TomlValue::String(format!("crates/core/{name}")))
            .collect(),
        ),
    );
    workspace.insert("resolver".into(), "2".into());
    workspace.insert("package".into(), manifest["workspace"]["package"].clone());
    workspace.insert("lints".into(), manifest["workspace"]["lints"].clone());
    workspace.insert("dependencies".into(), dependencies.into());
    fs::write(
        vendor.join("Cargo.toml"),
        toml::to_string(&Table::from_iter([("workspace".into(), workspace.into())]))
            .expect("minimal workspace"),
    )
    .expect("write minimal workspace");
    fs::copy(
        repository.join("rust-toolchain.toml"),
        vendor.join("rust-toolchain.toml"),
    )
    .expect("preserve exact compiler");
}

pub(super) fn copy_tree(source: &Path, destination: &Path) {
    fs::create_dir_all(destination).expect("vendor directory");
    for entry in fs::read_dir(source).expect("checked family source") {
        let entry = entry.expect("source entry");
        if ["target", ".git", ".agents"]
            .iter()
            .any(|name| entry.file_name() == *name)
        {
            continue;
        }
        let kind = entry.file_type().expect("source kind");
        assert!(
            !kind.is_symlink(),
            "family sources cannot silently escape the copied tree"
        );
        let to = destination.join(entry.file_name());
        if kind.is_dir() {
            copy_tree(&entry.path(), &to);
        } else {
            fs::copy(entry.path(), to).expect("checked source copy");
        }
    }
}

pub(super) fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("repository root")
        .to_owned()
}
