# Crates and API reference

The shop application needs `type-history` to declare and read receipts, plus `type-history-build` to check frozen schemas during Cargo builds. Install `cargo-type-history` to initialize and update the ledger, as shown in [package setup](setup.md).

The remaining crates support those three components. They are useful when building a framework or macro that reuses Type History's generation and schema checks:

| Crate | Responsibility |
| --- | --- |
| [`type-history`](https://docs.rs/type-history/latest/type_history/) | Public macros, history traits, and decoding API |
| [`type-history-core`](https://docs.rs/type-history-core/latest/type_history_core/) | Runtime decoding, stable names, and schema descriptions |
| [`type-history-macros`](https://docs.rs/type-history-macros/latest/type_history_macros/) | Procedural macro entry points |
| [`type-history-codegen`](https://docs.rs/type-history-codegen/latest/type_history_codegen/) | Parsing, schema checks, and Rust generation |
| [`type-history-build`](https://docs.rs/type-history-build/latest/type_history_build/) | Build integration and checked ledger operations |
| [`cargo-type-history`](https://docs.rs/crate/cargo-type-history/latest) | Cargo subcommand for lifecycle operations |

Type History is unreleased, so use the source checkout for now. To generate the API reference locally, run `cargo doc --workspace --no-deps --all-features --locked` from that checkout and open `target/doc/type_history/index.html`.

## Examples and tests

The [invoice reference application](https://github.com/sformisano/type-history/blob/main/crates/examples/invoice-history/README.md) includes a complete declaration, frozen ledger, and conversion tests.
The [tutorial integration tests](https://github.com/sformisano/type-history/blob/main/crates/core/cargo-type-history/tests/standalone/tutorial.rs) compile the quick start snippets and check rejected edits.
The [CI workflow](https://github.com/sformisano/type-history/blob/main/.github/workflows/ci.yml) lists the repository build and test commands.
