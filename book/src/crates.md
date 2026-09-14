# Crates and API reference

The shop application needs `type-history` to declare and read receipts, plus `type-history-build` to check frozen schemas during Cargo builds. Install `cargo-type-history` to initialize and update the ledger, as shown in [package setup](setup.md).

The remaining crates support those three components. They are useful when building a framework or macro that reuses Type History's generation and schema checks:

| Crate | Responsibility |
| --- | --- |
| `type-history` | Public macros, history traits, and decoding API |
| `type-history-core` | Runtime decoding, stable names, and schema descriptions |
| `type-history-macros` | Procedural macro entry points |
| `type-history-codegen` | Parsing, schema checks, and Rust generation |
| `type-history-build` | Build integration and checked ledger operations |
| `cargo-type-history` | Cargo subcommand for lifecycle operations |

Type History is unreleased, so use the source checkout for now. To generate the API reference locally, run `cargo doc --workspace --no-deps --all-features --locked` from that checkout and open `target/doc/type_history/index.html`.

## Examples and tests

The [invoice reference application](https://github.com/sformisano/type-history/blob/main/crates/examples/invoice-history/README.md) includes a complete declaration, frozen ledger, and conversion tests.
The [tutorial integration tests](https://github.com/sformisano/type-history/blob/main/crates/core/cargo-type-history/tests/standalone/tutorial.rs) compile the quick start snippets and check rejected edits.
The [CI workflow](https://github.com/sformisano/type-history/blob/main/.github/workflows/ci.yml) lists the repository build and test commands.
