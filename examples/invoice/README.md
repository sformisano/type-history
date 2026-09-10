# Executable invoice history

The example follows a Rust record from V1 to V3. It checks that earlier fields
remain available, conversions produce the current type, and frozen schemas reject
incompatible edits. The complete sources match the marked blocks in the
[Typehistory README](../../README.md).

The script also runs the introductory Receipt example first. It creates separate
`receipt-history` and `invoice-history` consumer packages outside this workspace,
so each history starts with its own empty ledger.

## Run the examples

Use Rust 1.97.1 with Bash, Git, and ripgrep installed. The lifecycle commands
run offline, so their dependencies must already be cached.

Create a short temporary directory on a disk with enough room for consumer builds.
Set `TMPDIR` to that directory in the following commands.

Run from the repository root:

```sh
export RUSTUP_TOOLCHAIN=1.97.1
export TMPDIR=/absolute/path/to/owned-short-directory
export CARGO_TARGET_DIR="$PWD/target"
cargo fetch --locked
cargo build -p cargo-typehistory --locked
export PATH="$CARGO_TARGET_DIR/debug:$PATH"
bash examples/invoice/verify.sh
```

Keep strict and export environment variables unset for ordinary draft steps.
The script supplies explicit strict settings only for checks that require them.
It creates its own consumers under `TMPDIR` and removes them on exit.

## What the script checks

Before compiling a consumer, `verify.sh` compares eight marked README blocks with
their Rust source files. A mismatch stops the example, so the documented code and
executed code cannot diverge unnoticed.

For Receipt, it freezes V1 before introducing currency in V2. The tests read a V1
receipt through its backfill and write a current V2 receipt with an explicit currency.
Freezing V2 then permits the release build.

For Invoice, it initializes an empty ledger and freezes V1. Rejected edits exercise
the declaration rules before V2 and V3 are introduced. The runtime tests convert
V1 into V3 and check a failed V2-to-V3 conversion. That failure keeps both the
version metadata and its original Rust error.

The script also checks development and release profiles, discarding an unfrozen
draft, reset, undo, exact refreeze, read-only checks, and import into another package.
The [guide](../../docs/guide.md) explains how those operations relate to the evolving type.

## Additional compiler coverage

The [standalone compiler suite](../../crates/cargo-typehistory/tests/standalone.rs)
adds retained-floor import, renamed dependencies, copied source isolation,
invalid ledger checks, and cache invalidation.

Run it from the repository root:

```sh
cargo test -p cargo-typehistory --test standalone --locked
```

These examples check their stated conversions. An application needs its own tests
to establish that backfill values and callbacks match the meaning of its historical records.
