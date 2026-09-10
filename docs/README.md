# Typehistory crates

These crates implement declarative histories for Rust record types. The
[repository README](../README.md) starts with a complete Receipt tutorial and
continues with field rules, lifecycle commands, decoding, and supported types.

The [guide](guide.md) explains the changes in the Receipt and Invoice examples
and what their tests establish. The [reference application](../examples/invoice/README.md)
is a runnable Cargo package; the [tutorial tests](../crates/cargo-typehistory/tests/standalone/tutorial.rs)
verify the intermediate edits in temporary consumers.

The [evolution matrix](evolution-matrix.md) records supported and rejected
combinations for readers checking a specific declaration rule.
