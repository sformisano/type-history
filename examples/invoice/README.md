# Invoice reference application

This Cargo package reads a stored V1 invoice into the current V3 type, then
serializes a new current invoice. Its [declaration](src/lib.rs), build hook,
configuration, and frozen schema ledger are committed, so it runs from a fresh
checkout without initialization or freezing.

Run from the repository root:

```sh
cargo run -p invoice-history --locked
cargo test -p invoice-history --locked
cargo run -p invoice-history --release --locked
```

The application prints the converted record and the current payload's identity,
version, and JSON. Its [tests](tests/history.rs) check historical conversion,
current payload round trips, strict decoding, and preserved callback errors.

The [README tutorial](../../README.md#evolve-an-invoice-through-v1-v2-and-v3)
explains how to author and freeze each version. Those intermediate edits,
expected compiler failures, and lifecycle operations belong to the
[Rust integration tests](../../crates/cargo-typehistory/tests/standalone/tutorial.rs):

```sh
cargo test -p cargo-typehistory --test standalone tutorial --locked
```

These tests compile the exact marked Rust blocks from the README in temporary
consumer packages. No shell runner or extra scripting runtime is required.
