# Set up a package

Set up the Cargo dependencies and schema checks before declaring your first versioned type.
The example project is `my-shop-demo-project`, a Cargo library package.

## 1. Create the package

Install the Cargo subcommand from crates.io:

```sh
cargo install cargo-type-history --version 0.1 --locked
```

Type History requires Rust 1.97 or later. The Cargo package is `type-history`;
Rust imports use `type_history`. Three packages have different jobs:

| Package | Why the example needs it |
| --- | --- |
| `type-history` | Provides the history declaration, generated types, and versioned values |
| `type-history-build` | Makes Cargo check that previously frozen versions remain intact |
| `cargo-type-history` | Creates the schema file and freezes new versions |

Create a library package:

```sh
cargo new --lib my-shop-demo-project --edition 2024
cd my-shop-demo-project
```

Replace `Cargo.toml` with this manifest:

```toml
[package]
name = "my-shop-demo-project"
version = "0.1.0"
edition = "2024"

[dependencies]
type-history = "0.1"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

[build-dependencies]
type-history-build = "0.1"
```

The examples use `serde_json` for serialization. [Nested records](integration.md#nested-records) also use Serde derives.

## 2. Initialize the schema file

A **schema** describes a record's serialized field names, field types, and nested structure.
**Freezing** saves a version's schema so later builds can reject changes to it.
Type History keeps those saved schemas in a file called the **ledger**.

The setup uses three files:

- `type-history.toml` enables Type History for the package.
- `build.rs` runs the schema checks during Cargo builds.
- `type-history/schemas.json` stores the ledger. The `init` command creates it.

Create `type-history.toml`:

```toml
format = 1
```

Create `build.rs`:

```rust
use type_history_build::compile;

fn main() {
    compile();
}
```

Empty `src/lib.rs` before initialization.

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

Lifecycle commands run with `--locked --offline`. Fetching dependencies first
allows their compiler checks to run without network access. After changing the
manifest, update the lockfile and fetch the required dependencies before using
those commands again.

Add this line to `.gitignore`:

```gitignore
**/type-history/.schemas.lock
```

Commit `Cargo.toml`, `Cargo.lock`, `type-history.toml`, `build.rs`, your source, and
`type-history/schemas.json` with your project. The `.schemas.lock` file prevents
concurrent commands from changing the ledger at the same time. It can be recreated.

Continue with [declaring V1](quick-start.md#1-declare-v1).
