# Type History crates

These crates implement declarative histories for Rust record types. The
[repository README](../README.md) starts with a complete `ReceiptCreated` tutorial and
continues with `Versioned`, field rules, lifecycle commands, and supported types.

Applications serialize `receipt.into_versioned()` with a supported Serde format.
They deserialize a `Versioned<ReceiptCreated>` and call
`ReceiptCreated::from_versioned` to obtain the current type.
Type History keeps identity and version with the matching data; the application owns storage.

The [guide](guide.md) explains the changes in the `ReceiptCreated` and `Invoice` examples
and what their tests establish. The [reference application](../examples/invoice/README.md)
is a runnable Cargo package; the [tutorial tests](../crates/cargo-type-history/tests/standalone/tutorial.rs)
verify the intermediate edits in temporary consumers.

The [evolution matrix](evolution-matrix.md) records supported and rejected
combinations for readers checking a specific declaration rule.
