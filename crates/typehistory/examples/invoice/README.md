# Executable invoice history

This example creates standalone libraries outside this repository's workspace. It uses
Typehistory, Redactable, and Serde. The complete sources match the runnable
blocks in the [Typehistory README](../../../../README.md).

Run from the repository root with Rust 1.97.1 and `rg` installed. Prepare an
owned short temporary directory and use the checkout's existing Cargo target.
The following setup uses a caller-selected directory on a disk with enough space:

```sh
export RUSTUP_TOOLCHAIN=1.97.1
export TMPDIR=/absolute/path/to/owned-short-directory
export CARGO_TARGET_DIR="$PWD/target"
cargo build -p cargo-typehistory --locked
export PATH="$CARGO_TARGET_DIR/debug:$PATH"
bash crates/typehistory/examples/invoice/verify.sh
```

The script requires cached dependencies because lifecycle commands run offline.
Use `cargo fetch --locked` before an offline run. Keep strict/export environment
variables unset for the ordinary draft steps. The script sets explicit strict
settings only on the commands that exercise them.

`verify.sh` checks README/source equality and runs the opening receipt example
in its own package. It freezes receipt V1, tests V1 decoding and V2 writing,
then freezes V2 and checks a release build.

For the invoice, it starts an empty library, initializes the ledger, and freezes V1.
It exercises rejected compiler edits, evolves V2/V3,
and decodes V1 bytes into the current type. It also checks strict and custom
profiles, source-based discard, reset, undo, exact refreeze, read-only checks,
and import into another empty package. It removes its owned consumers on exit.

The [standalone compiler suite](../../cargo-typehistory/tests/standalone.rs)
adds retained-floor import, renamed dependencies, isolated copied sources,
authority corruption, cache invalidation, and sensitive error renderings.
Run it with `cargo test -p cargo-typehistory --test standalone`.
See the [guide](../../docs/guide.md) for the sequence and expected outcomes.
