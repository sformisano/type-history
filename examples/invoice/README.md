# Invoice reference application

This Cargo package reads a stored V1 invoice into the current V3 type, then
serializes a new current invoice. Its [declaration](src/lib.rs), build hook,
configuration, and frozen schema ledger are included in the example.

Run from the repository root:

```sh
cargo run -p invoice-history --locked
cargo test -p invoice-history --locked
cargo run -p invoice-history --release --locked
```

The application uses `Invoice` throughout. It wraps current values with
`into_versioned`, serializes them, then calls `Invoice::from_versioned` after
deserialization. It prints the resulting invoices.

The [versioned value tests](tests/versioned.rs) cover JSON and MessagePack,
historical conversion, metadata validation, and preserved callback errors.
The [raw JSON tests](tests/history.rs) cover the existing low-level API for
applications that already keep metadata separately.

The [invoice walkthrough](../../docs/guide.md)
explains how to author and freeze each version. Those intermediate edits,
expected compiler failures, and lifecycle operations belong to the
[Rust integration tests](../../crates/cargo-type-history/tests/standalone/tutorial.rs):

```sh
cargo test -p cargo-type-history --test standalone tutorial --locked
```

These tests compile the exact marked Rust blocks from the documentation in temporary
consumer packages. No shell runner or extra scripting runtime is required.
