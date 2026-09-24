# Set up a package

Before the shop can declare `ReceiptCreated`, its package needs the Type History library, a build hook, and a place to save frozen schemas. This guide creates `my-shop-demo-project`, the Cargo library used by the [receipt walkthrough](quick-start.md).

This guide uses Type History 0.3.1 from crates.io and requires Rust 1.97 or later.
Linux is the tested host for lifecycle commands. Windows and macOS have not been validated.

## 1. Install the components

Install the CLI:

<!-- setup:release-install.sh -->
```sh
cargo install cargo-type-history --version '=0.3.1' --locked
```

Pin the library, build hook, and CLI to the same release so they agree on the schema format and supported attributes. The Cargo package is `type-history`; Rust imports use `type_history`.

| Package | Why the example needs it |
| --- | --- |
| `type-history` | Provides the history declaration, generated types, and versioned values |
| `type-history-build` | Makes Cargo check that previously frozen versions remain intact |
| `cargo-type-history` | Creates the schema file and freezes new versions |

Create a new library package:

<!-- setup:release-create.sh -->
```sh
cargo new --lib my-shop-demo-project --edition 2024
cd my-shop-demo-project
```

Replace `Cargo.toml` with this manifest:

<!-- setup:release-Cargo.toml -->
```toml
[package]
name = "my-shop-demo-project"
version = "0.1.0"
edition = "2024"

[dependencies]
type-history = "=0.3.1"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

[build-dependencies]
type-history-build = "=0.3.1"
```

`serde_json` reads and writes the receipt examples; Serde's derives support the [nested records](integration.md#nested-records) used later.

Continue with [initializing the schema file](#3-initialize-the-schema-file).

## 2. Source checkout alternative

Use this alternative only when developing against a source checkout.
Clone Type History beside the application:

```sh
git clone https://github.com/sformisano/type-history.git type-history
```

Install the CLI from that checkout:

<!-- setup:install.sh -->
```sh
cargo install --path type-history/crates/core/cargo-type-history --locked
```

Create the sibling application:

<!-- setup:create.sh -->
```sh
cargo new --lib my-shop-demo-project --edition 2024
cd my-shop-demo-project
```

Use this manifest instead:

<!-- setup:Cargo.toml -->
```toml
[package]
name = "my-shop-demo-project"
version = "0.1.0"
edition = "2024"

[dependencies]
type-history = { path = "../type-history/crates/core/type-history" }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

[build-dependencies]
type-history-build = { path = "../type-history/crates/core/type-history-build" }
```

Keep the checkout beside `my-shop-demo-project` while building the application.

## 3. Initialize the schema file

A **schema** describes a record's serialized field names, field types, and nested structure.
**Freezing** saves a version's schema so later builds can reject changes to it.
Type History keeps those saved schemas in a file called the **ledger**.

The setup uses two files:

- `build.rs` runs the schema checks during Cargo builds.
- `type-history/schemas.json` stores the ledger. The `init` command creates it.

Create `build.rs`:

<!-- setup:build.rs -->
```rust
use type_history_build::compile;

fn main() {
    compile();
}
```

Empty `src/lib.rs` before initialization.

<!-- setup:initialize.sh -->
```sh
cargo generate-lockfile
cargo fetch --locked
cargo type-history init --package my-shop-demo-project
```

`--package` selects the Cargo package by its `[package].name` in `Cargo.toml`.
Use your own package's name when running these commands in an existing project.

`init` creates an empty ledger at `type-history/schemas.json`. Run it before adding
the first history declaration. It refuses to replace an existing ledger.
Cargo builds require this file even when the package has no histories yet.
The ledger also identifies initialized packages for `cargo type-history check`.
No separate configuration file is needed. Existing `type-history.toml` files are
ignored and can be removed.

Commands such as `freeze` build a temporary copy of the package to check its schemas. They run Cargo with `--locked --offline`, so the lockfile and downloaded dependencies must already be ready. After changing dependencies, update the lockfile and fetch them before running these commands again.

Each command makes a fresh copy of the package in `type-history-snapshot` inside Cargo's target directory and removes the copy when it finishes. Compiled registry and Git dependencies stay there, so later commands rebuild only local path packages; `cargo clean` removes them. A command uses a private temporary directory instead while another command is using `type-history-snapshot`, before Cargo has created its target directory, or when `TYPE_HISTORY_PRIVATE_SNAPSHOT=1` is set. Any other value of that variable fails the command.

Type History captures each local Cargo package, its workspace manifest and lockfile,
and the effective Cargo configuration. Declare any build input outside a package:

```toml
[package.metadata.type-history]
snapshot-inputs = ["../../generated/schema.json"]
```

Each entry is relative to that package. It must name an existing file or directory.
Type History copies it into the snapshot and rejects changes during the command.
For a path containing symlinks, the snapshot preserves the declared route and resolved
contents. Adding, removing, or retargeting a route symlink invalidates the snapshot even
when the resolved bytes are unchanged.
Type History rejects a relative symlink whose unchanged target would escape the
snapshot.

Add this line to `.gitignore`:

```gitignore
**/type-history/.schemas.lock
```

Commit `Cargo.toml`, `Cargo.lock`, `build.rs`, your source, and
`type-history/schemas.json` with your project. The `.schemas.lock` file prevents
concurrent commands from changing the ledger at the same time. It can be recreated.

Continue with [declaring V1](quick-start.md#1-declare-v1).
