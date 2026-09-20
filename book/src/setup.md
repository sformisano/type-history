# Set up a package

Before the shop can declare `ReceiptCreated`, its package needs the Type History library, a build hook, and a place to save frozen schemas. This guide creates `my-shop-demo-project`, the Cargo library used by the [receipt walkthrough](quick-start.md).

The current setup installs from git revision
`fdfda383685bad2253f4d7dc1dfbc1d5de2fd8f3` and requires Rust 1.97 or later.
Linux is the tested host for lifecycle commands. Windows and macOS have not been validated.

This guide does not use the crates.io 0.1.0 release. The optional adapters in
[field integration](integration.md#optional-adapters) and the current field grammar
were added after 0.1.0 was published, and the version number cannot separate the two
artifacts: all six published manifests still read `version = "0.1.0"` twelve
commits past publication. A package that depends on `"0.1.0"` and then asks for an adapter
feature fails to resolve, because the published manifests carry no `[features]`
section at all.

## 1. Install the components

Install the CLI:

<!-- setup:release-install.sh -->
```sh
cargo install --git https://github.com/sformisano/type-history \
  --rev fdfda383685bad2253f4d7dc1dfbc1d5de2fd8f3 \
  cargo-type-history --locked
```

Pin the library, build hook, and CLI to the same revision so they agree on the schema format and supported attributes. The Cargo package is `type-history`; Rust imports use `type_history`.

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
type-history = { git = "https://github.com/sformisano/type-history", rev = "fdfda383685bad2253f4d7dc1dfbc1d5de2fd8f3" }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

[build-dependencies]
type-history-build = { git = "https://github.com/sformisano/type-history", rev = "fdfda383685bad2253f4d7dc1dfbc1d5de2fd8f3" }
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

Add this line to `.gitignore`:

```gitignore
**/type-history/.schemas.lock
```

Commit `Cargo.toml`, `Cargo.lock`, `build.rs`, your source, and
`type-history/schemas.json` with your project. The `.schemas.lock` file prevents
concurrent commands from changing the ledger at the same time. It can be recreated.

Continue with [declaring V1](quick-start.md#1-declare-v1).
