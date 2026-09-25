# Type History

Suppose a shop starts by accepting only US dollars. Its `ReceiptCreated` event stores an amount in cents. Later, the shop accepts other currencies and adds a `currency` field. The old events have only an amount, but they still mean USD.

Type History lets the current application read both versions. You describe when each field changed and how to convert older values. It generates the historical Rust types and the conversions from each version to the next. Freezing a version saves its schema. Build checks then reject a later edit that changes that version's structure.

Use it when persisted data outlives the code that wrote it:

- An event log contains events written before a field existed.
- A saved snapshot must load after the application changes its state type.
- Documents written by several releases must become one current Rust type.

Application code uses the current type. When it reads an old receipt, Type History decodes the receipt as its stored version. Calling `from_versioned` then converts that value to the current type. You supply the backfill that makes an old receipt's currency `"USD"`. The compiler checks the conversion's types; your tests check that it preserves the receipt's meaning.

Your application chooses how to store the bytes and which supported Serde format to use. Services that deploy separately also need to [coordinate when they start writing a new version](quick-start.md#using-type-history-in-a-distributed-system).

## Start here

1. [Set up a package](setup.md) with the build hook and schema ledger.
2. [Evolve the shop's `ReceiptCreated` event type](quick-start.md) through its first
   draft, freezing, field changes, and reading earlier data.
3. [Understand the generated types](core-model.md), including how
   `Versioned<ReceiptCreated>` holds any supported version until you convert it.

The remaining chapters cover individual tasks and their rules. The
[crates chapter](crates.md) links to the API reference for public types,
methods, and traits.
