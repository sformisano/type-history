# What the macro generates

The [quick start example](quick-start.md#3-add-a-field-in-v2) declares `ReceiptCreated` through V2.
This reference uses that declaration to describe the generated API.

The macro replaces your declaration with numbered structs and an alias under
the original name. A new history starts at V1, even when no field has a history
attribute. Version numbers in those attributes describe later changes; several
fields can change in the same version.

## Generated items

| Generated item | What it provides |
| --- | --- |
| `ReceiptCreatedV1`, `ReceiptCreatedV2`, and later types | One struct with the fields and types for that version |
| `ReceiptCreated` | Alias of the latest generated type, including an allowed draft |
| `ReceiptCreated::into_versioned()` | Wraps a current value in `Versioned<ReceiptCreated>` with its stable name and current version |
| `ReceiptCreated::from_versioned(...)` | Converts a deserialized `Versioned<ReceiptCreated>` into the current `ReceiptCreated` |
| Record implementations | `Clone`, `Debug`, `PartialEq`, Serde serialization and deserialization, and schema support |
| `HasHistory` on the current type | `STABLE_NAME` names the history, `VERSION` identifies the current version, and `history()` provides its decoder |
| `History<ReceiptCreated>` | Lists supported versions and decodes their bytes into the current `ReceiptCreated` |

The alias and numbered structs keep the declaration's visibility.
Explicitly imported histories can [start above V1](lifecycle.md#import-a-retained-floor).

## Traits and declaration attributes

By default, every field type must implement `Clone`, `PartialEq`, and `Debug`,
including types used only in earlier versions. Generated `Debug` calls each
field's own `Debug` implementation. Histories that opt into
[Redactable support](redactable.md) use different formatting and error contracts.

The macro owns the generated implementations, so do not add derives to the history
declaration. You can add documentation and `allow`, `warn`, or `deny` attributes.
Put field naming lints on the record, following Rust's usual lint scope.
Other attributes that transform the declaration are unsupported.

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
generated types and conversion functions, so it needs neither the ledger nor Cargo.
