# Crates and API reference

Applications normally depend on `type-history` and `type-history-build`.
The workspace separates runtime code, code generation, and development tooling:

| Crate | Responsibility |
| --- | --- |
| [`type-history`](https://docs.rs/type-history/latest/type_history/) | Public macros, history traits, and decoding API |
| [`type-history-core`](https://docs.rs/type-history-core/latest/type_history_core/) | Runtime decoding, stable names, and structural schema contracts |
| [`type-history-macros`](https://docs.rs/type-history-macros/latest/type_history_macros/) | Procedural macro entry points |
| [`type-history-codegen`](https://docs.rs/type-history-codegen/latest/type_history_codegen/) | Parsing, schema checks, and Rust generation |
| [`type-history-build`](https://docs.rs/type-history-build/latest/type_history_build/) | Build integration and checked ledger operations |
| [`cargo-type-history`](https://docs.rs/crate/cargo-type-history/latest) | Cargo subcommand for lifecycle operations |

## Examples and tests

The [invoice reference application](https://github.com/sformisano/type-history/blob/main/examples/invoice/README.md) includes a complete declaration, frozen ledger, and conversion tests.
The [tutorial integration tests](https://github.com/sformisano/type-history/blob/main/crates/cargo-type-history/tests/standalone/tutorial.rs) compile the quick start snippets and check rejected edits.
See [Contributing](https://github.com/sformisano/type-history/blob/main/CONTRIBUTING.md) for repository build and test commands.
