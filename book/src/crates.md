# Crates and API reference

The shop application needs `type-history` to declare and read receipts, plus `type-history-build` to check frozen schemas during Cargo builds. Install `cargo-type-history` to initialize and update the ledger, as shown in [package setup](setup.md).

The remaining crates support those three components. They are useful when building a framework or macro, called a **frontend**, that reuses Type History's generation and schema checks:

| Crate | Responsibility |
| --- | --- |
| `type-history` | Public macros, history traits, and decoding API |
| `type-history-core` | Runtime decoding, stable names, and schema descriptions |
| `type-history-macros` | Procedural macro entry points |
| `type-history-codegen` | Parsing, schema checks, and Rust generation |
| `type-history-build` | Build integration and checked ledger operations |
| `cargo-type-history` | Cargo subcommand for lifecycle operations |

The API reference is published on [docs.rs](https://docs.rs/type-history). To generate it locally, clone the repository, run `cargo doc --workspace --no-deps --all-features --locked`, and open `target/doc/type_history/index.html`.

## Framework integration

`generate_history` retains native payload traits and unconditional frozen-shape
assertions. `generate_history_with_options` lets a frontend supply its own
payload traits through `PayloadTraits::Frontend`. Each `GeneratedVersion::contract`
contains the actual field types, visibility, and authored field attributes.
A frontend can read its own field attributes there, such as a masking policy,
when it implements payload traits.

`GeneratedVersion::shape_expression` exports the resolved structural shape.
Frozen export readers use `JsonSchemaDocument::try_from_shape`, which rejects
malformed shapes before writing a document. The separate `schema_expression`
exports descriptive JSON Schema through each field type's `JsonSchemaField`
implementation.

A configured `observation_cfg` suppresses shape assertions only when both that
cfg and `test` are active. The frontend build hook must reject that cfg in
strict and release builds. `ToolContract` and `LifecycleOps` keep each frontend's
configuration, metadata, discovery, and ledger operations separate. Lifecycle
snapshots for every frontend share `type-history-snapshot` and
`TYPE_HISTORY_PRIVATE_SNAPSHOT`; neither depends on the contract.

`Declaration::with_source` attaches optional `DeclarationSource` metadata.
Locations use one-based lines and columns from source discovery. Differences in
`check --format json` reports point to the containing authored field, with the
declaration as fallback. `Declaration::resolve` leaves `source` empty for
frontends without this metadata.

Since 0.3.0, the generator support API no longer includes the `SameSchema` trait
family, the const diagnostic encoder, or the marker parser. `WireNode::schema()`
defaults to `Self::SHAPE.schema()`. Explicit implementations remain available.
The standalone schema export build prints one `TYPE_HISTORY_SCHEMA_EXPORT_V2` row
per history with each version's structural shape. Frozen ledgers and stored
payload formats remain unchanged.

## Examples and tests

The [invoice reference application](https://github.com/sformisano/type-history/blob/main/crates/examples/invoice-history/README.md) includes a complete declaration, frozen ledger, and conversion tests.
The [tutorial integration tests](https://github.com/sformisano/type-history/blob/main/crates/core/cargo-type-history/tests/standalone/tutorial.rs) compile the quick start snippets and check rejected edits.
The [CI workflow](https://github.com/sformisano/type-history/blob/main/.github/workflows/ci.yml) lists the repository build and test commands.
