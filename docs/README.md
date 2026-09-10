# Type History documentation

The [README walkthrough](../README.md#how-it-works) follows one type from its first declaration to reading earlier data.
Use these pages when you need setup instructions or a specific rule.

## Guides

- [Package setup](setup.md): Dependencies, the build hook, and the initial schema ledger.
- [Invoice example](guide.md): A field rename, repeated type changes, and a conversion that can fail.
- [Optional Redactable integration](redactable.md): Field policies, safe output, and redacted conversion errors.

## Reference

- [Generated API and stable names](macro.md): Numbered types, the current alias, generated methods, traits, and name rules.
- [Field history](field-history.md): Attributes, callback signatures, ordering, removals, and rejected declarations.
- [Serialization and decoding](decoding.md): `Versioned`, supported formats, error handling, and raw JSON with separate metadata.
- [Freezing and lifecycle commands](lifecycle.md): Frozen schema checks, drafts, reset, import, and build profiles.
- [Integration and supported types](integration.md): Rust modules, renamed dependencies, and nested records.
- [Evolution matrix](evolution-matrix.md): Supported and rejected combinations of history rules.

## Crates

Applications normally depend on `type-history` and `type-history-build`.
The workspace separates runtime code, code generation, and development tooling:

| Crate | Responsibility |
| --- | --- |
| `type-history` | Public macros, history traits, and decoding API |
| `type-history-core` | Runtime decoding, stable names, and structural schema contracts |
| `type-history-macros` | Procedural macro entry points |
| `type-history-codegen` | Parsing, schema checks, and Rust generation |
| `type-history-build` | Build integration and checked ledger operations |
| `cargo-type-history` | Cargo subcommand for lifecycle operations |

## Examples and tests

The [invoice reference application](../examples/invoice/README.md) includes a complete declaration, frozen ledger, and conversion tests.
The [tutorial integration tests](../crates/cargo-type-history/tests/standalone/tutorial.rs) compile the walkthrough snippets and check rejected edits.
See [Contributing](../CONTRIBUTING.md) for repository build and test commands.
