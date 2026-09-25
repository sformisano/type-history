# Generated API and stable names

The shop's [V2 declaration](quick-start.md#3-add-a-field-in-v2) contains `amount_cents` and `currency`. Its history attribute says that `currency` was added in V2. From that declaration, the macro generates V1 without `currency`, V2 with both fields, and a conversion that supplies `"USD"` for old receipts.

The original name, `ReceiptCreated`, becomes an alias for V2. Application code uses that alias. Callbacks name the earlier type they read.

A new history starts at V1, even when no field has a history attribute. [Imported histories](lifecycle.md#import-a-complete-history) also start at V1. Later version numbers come from field changes. Until V1 is frozen, builds reject a field that declares a later version. Several fields can change in the same version.

## Generated items

| Generated item | What it provides |
| --- | --- |
| `ReceiptCreatedV1`, `ReceiptCreatedV2`, and later types | One struct with the fields and types for that version |
| `ReceiptCreated` | Alias of the latest generated type, including an allowed draft |
| `ReceiptCreated::into_versioned()` | Wraps a current value in `Versioned<ReceiptCreated>` with its stable name and current version |
| `ReceiptCreated::from_versioned(...)` | Converts a deserialized `Versioned<ReceiptCreated>` into the current `ReceiptCreated` |
| Record implementations | `Clone`, optional `Debug` and `PartialEq`, Serde serialization and deserialization, and schema support |
| `HasHistory` on the current type | `STABLE_NAME` names the history, `VERSION` identifies the current version, and `history()` provides its decoder |
| `History<ReceiptCreated>` | Lists supported versions and decodes their JSON payloads into the current `ReceiptCreated` |

The alias and numbered structs keep the declaration's visibility. A `pub struct ReceiptCreated` produces public numbered types and a public alias.

## Traits and declaration attributes

The macro always generates `Clone` for every version, including versions used
only to read old data. Each retained field type therefore needs `Clone`. It also
needs Serde's `Serialize` and `Deserialize`, plus schema support.

`Debug` and `PartialEq` are enabled by default. Generated implementations call
the native field traits, so custom field behavior remains intact.

Disable either trait for a complete history when a field lacks that native
trait. Bare tuples with 13 through 16 elements need both traits disabled because
Rust's standard tuple implementations stop at arity 12:

```rust
use type_history::versioned;

#[versioned(
    stable_name = "example.long_tuple",
    derive_debug = false,
    derive_partial_eq = false,
)]
pub struct Record {
    pub value: (u8, u8, u8, u8, u8, u8, u8, u8, u8, u8, u8, u8, u8),
}
```

Both arguments require boolean literals and default to `true`. Each choice
applies to every retained version. A field used only by an old version still
follows the same choice.

Disabling one trait removes its generated implementation and field bound.
It does not create a fallback implementation. It also does not change schema
identity, frozen ledgers, payload bytes, codecs, or migrations.

[Frontends](crates.md) that construct `type_history_codegen::RecordInput`
directly must set its `derives: RecordDerives` field. Use
`RecordDerives::default()` to keep both traits enabled.

See [supported field contracts](integration.md#supported-field-contracts) for
tuple, set, adapter, and codec limits.

The macro owns the generated implementations, so do not add derives to the history
declaration. You can add documentation and `allow`, `warn`, or `deny` attributes.
Put field naming lints on the record, following Rust's usual lint scope.
Fields also accept `#[history(...)]`. Every other attribute is rejected.

## Stable names

A stable name can be a simple name such as `receipt_created`.
Use dots if a namespace helps, as in `shop.receipt`; no segment count is required.
The rules are:

- Start with a lowercase ASCII letter, followed by lowercase letters, digits,
  or underscores.
- Optional dots separate segments that follow the same rule. Empty segments
  are invalid, so dots cannot be leading, trailing, or consecutive.
- At most 255 ASCII bytes in total.

Declare it with `stable_name = "receipt_created"`.
Names such as `billing.invoice.issued` follow the same rules.
Builds and lifecycle commands reject changes to a frozen stable name, just as they
reject changes to historical field shapes. Rust types and modules can be renamed
without changing that stable name. Resetting a schema does not allow changing its stable name.

## Design principles

Every generated history follows these rules:

- **One history, one stable name:** The name connects stored records to their
  history even when Rust type names change.
- **One Rust type per version:** Earlier types keep the fields that existed in
  that version. Application code reads and writes through the current alias.
- **One conversion per step:** Reading V1 as V3 first converts V1 to V2, then V2
  to V3. Each new version builds on the existing conversions.
- **Explicit values for old data:** You choose what an added field means for
  older records. Type History cannot infer that choice from the field's type.

The ledger is needed when building the application. The running application uses
generated types and conversions, so it needs neither the ledger nor Cargo.
