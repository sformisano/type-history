# Type History

Type History helps you change Rust structs while keeping earlier data readable.
You describe field changes with attributes. It generates each version's type
and conversion code, then checks that your declarations preserve frozen versions.

Use it when persisted data outlives the code that wrote it:

- An event log contains events written before a field existed.
- A saved snapshot must load after the application changes its state type.
- Documents written by several releases must become one current Rust type.

Application code works with the current type. Type History selects the older
type when reading earlier data and applies conversions in order. Your application
chooses the storage system and a supported Serde format.

## Start here

1. [Set up a package](setup.md) with the build hook and schema ledger.
2. [Evolve the shop's `ReceiptCreated` event type](quick-start.md) through its first
   draft, freezing, field changes, and reading earlier data.
3. [Understand the generated types](core-model.md), including how the current
   alias can read every supported version.

The remaining chapters cover individual tasks and their rules. The
[API reference](https://docs.rs/type-history/latest/type_history/) describes
public types, methods, and traits.
