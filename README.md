# Type History

Type History is a Rust library that lets you evolve structs while preserving
support for data written using earlier versions. It uses simple field attributes
to describe changes and compile-time checks to catch missing or incorrect
attributes before they cause runtime failures in production.

## Table of contents

- [When to use Type History](#when-to-use-type-history)
- [How it works](#how-it-works)
- [Start an empty package](#start-an-empty-package)
- [Design principles](#design-principles)
- [What the macro generates](#what-the-macro-generates)
- [An invoice through V1, V2, and V3](#evolve-an-invoice-through-v1-v2-and-v3)
- [Field history reference](#field-history-reference)
- [Rejected declarations](#rejected-declarations)
- [Reading records and handling failures](#decode-stored-records-and-handle-failures)
- [Frozen schemas and conversion meaning](#frozen-shapes-and-business-meaning)
- [Lifecycle commands](#lifecycle-commands)
- [Build profiles and environment](#build-profiles-and-environment)
- [Modules, dependency names, and nested values](#modules-dependency-names-and-nested-values)
- [Supported types and limits](#supported-scope)
- [Optional Redactable integration](#optional-redactable-integration)
- [Crates and further reading](#crates-and-further-reading)

## When to use Type History

Persisted data often outlives the code that wrote it. A production application
may need to read years of records after its Rust structs have changed.

Type History helps when those records need to become the types your application
uses today. Common examples include:

- **Stored events:** Read events written before a field was added or changed.
- **Saved snapshots:** Restore a record written by an earlier application version.
- **Versioned documents:** Read records with different stored versions through
  one current Rust type.

## How it works

Suppose a shop originally accepted only US dollars. Each purchase produced a
`ReceiptCreated` event with an amount in cents. The shop now accepts other currencies,
so new events need a `currency` field.

### 1. Describe the change

This example declares a versioned `ReceiptCreated` struct with the new `currency`
field:

<!-- quick-example:v2.rs -->
```rust
use type_history::versioned;

#[versioned(stable_id = "shop.receipt.created")]
pub struct ReceiptCreated {
    pub amount_cents: u64,

    #[history(added_at = v2, backfill_value = "USD".to_owned())]
    pub currency: String,
}
```

- `#[versioned(...)]` marks `ReceiptCreated` as a versioned struct.
  - Its `stable_id` argument identifies this history.
  - Once frozen, changing it fails the build (more on this later).
- `#[history(...)]` describes the change to the `currency` field.
  - `added_at = v2` introduces `currency` in V2.
  - `backfill_value` supplies `"USD"` when converting V1 events, because the shop accepted only US dollars then.

### 2. Use the current type

Type History generates `ReceiptCreatedV1` with `amount_cents`, `ReceiptCreatedV2`
with both fields, and the conversion between them. It keeps the name you
declared as an alias for the latest struct:

```rust
pub type ReceiptCreated = ReceiptCreatedV2;
```

When you introduce V3, the generated alias changes to `ReceiptCreatedV3`.
Application code keeps using `ReceiptCreated` to construct current values and
read earlier data:

<!-- overview:read-old.rs -->
```rust
use serde_json::from_slice;

let stored = from_slice(
    br#"{"shop.receipt.created":{"1":{"amount_cents":1200}}}"#,
)?;
let receipt = ReceiptCreated::from_versioned(stored)?;

assert_eq!(receipt.amount_cents, 1200);
assert_eq!(receipt.currency, "USD");
```

This example uses JSON. Your application chooses storage and a supported Serde
format. `Versioned<ReceiptCreated>` keeps the stable identity, version, and data together.

### 3. Check changes at compile time

Once V1 is frozen, adding `currency` without its history attribute fails the build:
it would change V1 instead of introducing V2. A backfill with the wrong type also
fails compilation. Conversion functions receive the same type checks.

The setup below enables these checks and walks through freezing V1 before adding
V2. Later snippets reuse these types; snippets with `?` belong in a function that
returns a compatible `Result`.

## Start an empty package

### 1. Create the package

Install the Cargo subcommand from crates.io:

```sh
cargo install cargo-type-history --version 0.1 --locked
```

Type History requires Rust 1.97 or later. The Cargo package is `type-history`;
Rust imports use `type_history`. Three packages have different jobs:

| Package | Why the example needs it |
| --- | --- |
| `type-history` | Provides the history declaration, generated types, and versioned values |
| `type-history-build` | Makes Cargo check that previously frozen versions remain intact |
| `cargo-type-history` | Creates the schema file and freezes new versions |

Create a library package:

```sh
cargo new --lib receipt-history --edition 2024
cd receipt-history
```

Replace `Cargo.toml` with this manifest:

```toml
[package]
name = "receipt-history"
version = "0.1.0"
edition = "2024"

[dependencies]
type-history = "0.1"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

[build-dependencies]
type-history-build = "0.1"
```

The tests use `serde_json` to write receipts. The nested record example later in
this README also uses Serde derives.

### 2. Initialize the schema file

A **schema** describes a record's serialized field names, field types, and nested
structure. Type History saves frozen schemas in a file called the **ledger**.
Later builds compare your declaration with that file to check that older versions
remain intact.

The setup uses three files:

- `type-history.toml` enables Type History for the package.
- `build.rs` runs the schema checks during Cargo builds.
- `type-history/schemas.json` stores the ledger. The `init` command creates it.

Create `type-history.toml`:

```toml
format = 1
```

Create `build.rs`:

```rust
use type_history_build::compile;

fn main() {
    compile();
}
```

Empty `src/lib.rs` before initialization.

```sh
cargo generate-lockfile
cargo fetch --locked
cargo type-history init --package receipt-history
```

`init` creates an empty ledger at `type-history/schemas.json`. Run it before adding
the first history declaration. It refuses to replace an existing ledger.
Cargo builds require this file even when the package has no histories yet.

Lifecycle commands run with `--locked --offline`. Fetching dependencies first
allows their compiler checks to run without network access. After changing the
manifest, update the lockfile and fetch the required dependencies before using
those commands again.

Add this line to `.gitignore`:

```gitignore
**/type-history/.schemas.lock
```

Commit `Cargo.toml`, `Cargo.lock`, `type-history.toml`, `build.rs`, your source, and
`type-history/schemas.json` with your project. The `.schemas.lock` file prevents
concurrent commands from changing the ledger at the same time. It can be recreated.

### 3. Declare V1

Write this complete `src/lib.rs`:

<!-- quick-example:v1.rs -->
```rust
use type_history::versioned;

#[versioned(stable_id = "shop.receipt.created")]
pub struct ReceiptCreated {
    pub amount_cents: u64,
}
```

This declaration gives Type History two pieces of information:

- **Stable identity:** `shop.receipt.created` identifies this history in stored records.
  Once frozen, changing it fails the build. Rust type and module names can change
  without changing this identity.
- **Initial fields:** `amount_cents` belongs to V1. Fields without history
  attributes exist from V1 by default.

Type History generates `ReceiptCreatedV1` with an `amount_cents` field. It also makes
`ReceiptCreated` an alias of `ReceiptCreatedV1`, so application code can use the original name.

### 4. Freeze V1

The first version starts as a **draft**, which you can edit during development.
Development builds allow drafts with a warning. Freezing V1 saves its schema in
the ledger and makes later builds reject changes to that schema.

Build the declaration, then freeze V1:

```sh
cargo build --locked
cargo type-history freeze --package receipt-history
```

Changing `amount_cents` from `u64` to `String` now fails compilation because it
would change frozen V1. Describe further changes in V2 so V1 remains available.

### 5. Add currency in V2

Replace `src/lib.rs` with the [V2 `ReceiptCreated` declaration above](#1-describe-the-change).
It adds `currency` with `"USD"` as the value for older receipts.

One declaration now generates both record versions:

| Type | Fields |
| --- | --- |
| `ReceiptCreatedV1` | `amount_cents: u64` |
| `ReceiptCreatedV2` | `amount_cents: u64`, `currency: String` |
| `ReceiptCreated` | Alias of `ReceiptCreatedV2` |

The conversion keeps the amount and fills in the currency. V1's schema stays
unchanged. V2 becomes the new draft, and application code using `ReceiptCreated` now
sees both fields.

### 6. Read V1 and write V2

A V1 `Versioned<ReceiptCreated>` has this JSON representation:

```json
{"shop.receipt.created":{"1":{"amount_cents":1200}}}
```

The outer map identifies the history. The inner map identifies the payload version.
Both maps contain exactly one entry. The payload keeps its original fields.

Deserialize the bytes, then pass the result to `ReceiptCreated::from_versioned`.
Type History checks the identity and selects V1 before reading the payload.
`from_versioned` converts it into the current `ReceiptCreated`, assigning `"USD"` to the new field.
Application code supplies neither a numbered Rust type nor separate metadata.

Append these tests to `src/lib.rs`:

<!-- quick-example:runtime-tests.rs -->
```rust
#[cfg(test)]
mod receipt_tests {
    use super::ReceiptCreated;
    use serde_json::{from_slice, to_vec};

    #[test]
    fn read_an_old_receipt() {
        let stored = from_slice(
            br#"{"shop.receipt.created":{"1":{"amount_cents":1200}}}"#,
        ).unwrap();
        let receipt = ReceiptCreated::from_versioned(stored).unwrap();

        assert_eq!(receipt.amount_cents, 1200);
        assert_eq!(receipt.currency, "USD");
    }

    #[test]
    fn write_a_current_receipt() {
        let receipt = ReceiptCreated {
            amount_cents: 3000,
            currency: "EUR".to_owned(),
        };
        let stored = receipt.into_versioned();
        let bytes = to_vec(&stored).unwrap();

        // The application stores and retrieves these bytes.
        let restored = ReceiptCreated::from_versioned(from_slice(&bytes).unwrap()).unwrap();
        assert_eq!(restored.amount_cents, 3000);
        assert_eq!(restored.currency, "EUR");
    }
}
```

These tests check two uses of the same current type:

- **Read an old receipt:** V1 bytes become a `ReceiptCreated` with amount 1200 and currency
  `"USD"`.
- **Write a new receipt:** A `ReceiptCreated` with currency `"EUR"` keeps that value when
  serialized and read back as V2.

The backfill runs only when converting older records. New `ReceiptCreated` values must
supply their currency explicitly. Decoding leaves the stored bytes unchanged.
`into_versioned` stamps the current identity and version automatically.
`Versioned<ReceiptCreated>` keeps them with the matching data and provides no metadata setters.
No trait import is needed for `into_versioned` or `from_versioned`.

Existing raw JSON such as `{"amount_cents":1200}` has no identity or version.
It remains readable through the [low-level JSON API](#existing-raw-json-payloads).
It does not become a `Versioned` automatically.

Run the tests:

```sh
cargo test --lib --locked
```

### 7. Freeze V2 for release

Development builds let you revise V2 while testing its conversion. Release builds
require all histories to be frozen, so an unfinished draft cannot enter a release.

Freeze V2, then check the release build:

```sh
cargo type-history freeze --package receipt-history \
  --type shop.receipt.created --version 2
cargo build --release --locked
```

The ledger now records both V1 and V2. Commit its update with the changed
declaration and conversion tests. The next change can introduce V3.

## Design principles

The receipt example uses a few rules that also apply to longer histories:

- **One history, one identity:** The identity connects stored records to their
  history even when Rust type names change.
- **One Rust type per version:** Earlier types keep the fields that existed in
  that version. Application code reads and writes through the current alias.
- **One conversion per step:** Reading V1 as V3 first converts V1 to V2, then V2
  to V3. Each new version builds on the existing conversions.
- **Explicit values for old data:** You choose what an added field means for
  older records. Type History cannot infer that choice from the field's type.

The ledger is needed when building the application. The running application uses
generated types and conversion functions, so it needs neither the ledger nor Cargo.

## What the macro generates

The macro replaces your declaration with numbered structs and an alias under
the original name. A new history starts at V1, even when no field has a history
attribute. Version numbers in those attributes describe later changes; several
fields can change in the same version.

| Generated item | What it provides |
| --- | --- |
| `ReceiptCreatedV1`, `ReceiptCreatedV2`, and later types | One struct with the fields and types for that version |
| `ReceiptCreated` | Alias of the latest generated type, including an allowed draft |
| `ReceiptCreated::into_versioned()` | Wraps a current value in `Versioned<ReceiptCreated>` with its generated identity and version |
| `ReceiptCreated::from_versioned(...)` | Converts a deserialized `Versioned<ReceiptCreated>` into the current `ReceiptCreated` |
| Record implementations | `Clone`, `Debug`, `PartialEq`, Serde serialization and deserialization, and schema support |
| `HasHistory` on the current type | `TYPE` identifies the history, `VERSION` identifies the current version, and `history()` provides its decoder |
| `History<ReceiptCreated>` | Lists supported versions and decodes their bytes into the current `ReceiptCreated` |

The alias and numbered structs keep the declaration's visibility.
Explicitly imported histories can [start above V1](#import-a-retained-floor).

By default, every field type must implement `Clone`, `PartialEq`, and `Debug`,
including types used only in earlier versions. Generated `Debug` calls each
field's own `Debug` implementation. Histories that opt into
[Redactable support](#optional-redactable-integration) use different formatting and error contracts.

The macro owns the generated implementations, so do not add derives to the history
declaration. You can add documentation and `allow`, `warn`, or `deny` attributes.
Put field naming lints on the record, following Rust's usual lint scope.
Other attributes that transform the declaration are unsupported.

A history identity can be a simple name such as `receipt_created`.
Use dots if a namespace helps, as in `shop.receipt`; no segment count is required.
The rules are:

- Start with a lowercase ASCII letter, followed by lowercase letters, digits,
  or underscores.
- Optional dots separate segments that follow the same rule. Empty segments
  are invalid, so dots cannot be leading, trailing, or consecutive.
- At most 255 ASCII bytes in total.

Declare it with `stable_id = "receipt_created"`.
Existing dotted IDs such as `billing.invoice.issued` remain valid.
Builds and lifecycle commands reject changes to a frozen identity, just as they
reject changes to historical field shapes. Rust types and modules can be renamed
without changing that identity. Resetting a schema does not allow changing its identity.

## Evolve an invoice through V1, V2, and V3

The receipt needed a fixed value for its new field. An invoice shows what happens
when a conversion needs data from the previous version.

This example covers:

- **Type changes:** Convert a count from `u32` to `u64`, then to `String`.
- **Renames:** Read `legacy` from the previous record to populate the new `label` field.
- **Failures:** Find the original stored version and the conversion step that failed.

### Prepare a separate invoice package

Create a fresh package called `invoice-history`. It needs its own ledger so you
can declare and freeze V1 before adding V2.

Create it alongside `receipt-history`, in a separate directory.

```sh
cargo new --lib invoice-history --edition 2024
cd invoice-history
```

1. Copy the [setup manifest](#1-create-the-package).
2. Set `name = "invoice-history"` in its `[package]` section.
3. Create `type-history.toml` and `build.rs` using the [initialization instructions](#2-initialize-the-schema-file).
4. Empty `src/lib.rs`.

Initialize the package:

```sh
cargo generate-lockfile
cargo fetch --locked
cargo type-history init --package invoice-history
```

Continue in `invoice-history` for the following steps.

### V1: retain the original fields

The first invoice has a `legacy` label and a count stored as `u32`.

Write this complete `src/lib.rs`:

<!-- example:versions/v1.rs -->
```rust
use type_history::versioned;

#[versioned(stable_id = "billing.invoice.issued")]
pub struct Invoice {
    pub legacy: String,
    pub count: u32,
}
```

`Invoice` currently aliases `InvoiceV1`. Freeze this starting point before changing it:

```sh
cargo build --locked
cargo type-history freeze --package invoice-history
```

### V2: remove, convert, and add fields

V2 makes three changes:

- **Rename `legacy` to `label`:** Mark `legacy` as removed and add `label` in V2.
  Keep the `legacy` declaration so Type History can generate `InvoiceV1`.
- **Widen `count` to `u64`:** Record its previous type, `u32`, and provide a conversion.
- **Add `revision`:** Assign 7 to earlier invoices. This value is a choice made
  for the example; your application needs a value that fits its own history.

Replace `src/lib.rs` with this complete source:

<!-- example:versions/v2.rs -->
```rust
use std::{
    error::Error,
    fmt::{Display, Formatter, Result as FmtResult},
};
use type_history::versioned;

#[versioned(stable_id = "billing.invoice.issued")]
pub struct Invoice {
    #[history(removed_at = v2)]
    pub legacy: String,
    #[history(updated_at = v2, previous_type = u32, cast_from_previous_value = widen)]
    pub count: u64,
    #[history(added_at = v2, cast_from_previous_payload = label)]
    pub label: String,
    #[history(added_at = v2, backfill_value = 7_u32)]
    pub revision: u32,
}

fn widen(value: u32) -> Result<u64, ConvertError> {
    Ok(u64::from(value))
}

fn label(previous: &InvoiceV1) -> Result<String, ConvertError> {
    Ok(previous.legacy.clone())
}

#[derive(Debug)]
pub struct ConvertError(pub u64);

impl Display for ConvertError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        write!(formatter, "count {} cannot be converted", self.0)
    }
}

impl Error for ConvertError {}
```

The two conversion functions take different inputs:

- **`widen` takes the old field value:** `cast_from_previous_value` passes it the
  previous `count` by value. It returns the count as `u64`.
- **`label` takes the old record:** `cast_from_previous_payload` passes it
  `&InvoiceV1`. It clones `legacy` to produce the new label.

The rename needs access to the whole previous record because V1 has no field
called `label`. Type History calls record conversions before moving any fields,
so `legacy` remains available until `label` has read it.

Conversion functions, also called callbacks, return `Result`. These two always
return `Ok`; V3 introduces a conversion that can fail.

Build and freeze V2:

```sh
cargo build --locked
cargo type-history freeze --package invoice-history \
  --type billing.invoice.issued --version 2
```

### V3: keep both adjacent conversions

V3 changes `count` from `u64` to `String`. The declaration keeps the earlier
`u32`-to-`u64` conversion because V1 records still need that step.

Replace `src/lib.rs` with this complete source:

<!-- example:versions/v3.rs -->
```rust
use std::{
    error::Error,
    fmt::{Display, Formatter, Result as FmtResult},
};
use type_history::versioned;

#[versioned(stable_id = "billing.invoice.issued")]
pub struct Invoice {
    #[history(removed_at = v2)]
    pub legacy: String,
    #[history(updated_at = v3, previous_type = u64, cast_from_previous_value = stringify)]
    #[history(updated_at = v2, previous_type = u32, cast_from_previous_value = widen)]
    pub count: String,
    #[history(added_at = v2, cast_from_previous_payload = label)]
    pub label: String,
    #[history(added_at = v2, backfill_value = 7_u32)]
    pub revision: u32,
}

fn widen(value: u32) -> Result<u64, ConvertError> {
    Ok(u64::from(value))
}

fn label(previous: &InvoiceV1) -> Result<String, ConvertError> {
    Ok(previous.legacy.clone())
}

fn stringify(value: u64) -> Result<String, ConvertError> {
    if value == 13 {
        Err(ConvertError(value))
    } else {
        Ok(value.to_string())
    }
}

#[derive(Debug)]
pub struct ConvertError(pub u64);

impl Display for ConvertError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        write!(formatter, "count {} cannot be converted", self.0)
    }
}

impl Error for ConvertError {}
```

The newest update appears first on `count`. Its history now describes `u32` in
V1, `u64` in V2, and `String` in V3.

Reading V1 runs both conversions in order:

| Stored count | V1 to V2: `widen` | V2 to V3: `stringify` |
| --- | --- | --- |
| 42 | Produces `42_u64` | Produces `"42"` |
| 13 | Produces `13_u64` | Returns `ConvertError(13)` |

The rejection of 13 exists only to demonstrate failure handling. The resulting
error records V1 as the source version and V2-to-V3 as the failed step.

Append this test module to `src/lib.rs`:

<!-- example:runtime-tests.rs -->
```rust
#[cfg(test)]
mod tests {
    use super::{ConvertError, Invoice};
    use serde_json::from_slice;
    use std::error::Error;
    use type_history::DecodeFailureKind;

    #[test]
    fn old_bytes_reach_the_current_type() {
        let stored = from_slice(
            br#"{"billing.invoice.issued":{"1":{"legacy":"INV-42","count":42}}}"#,
        ).unwrap();
        let value = Invoice::from_versioned(stored).unwrap();
        assert_eq!(value.count, "42");
        assert_eq!(value.label, "INV-42");
        assert_eq!(value.revision, 7);
        let _: Invoice = Invoice {
            count: "42".to_owned(),
            label: "current".to_owned(),
            revision: 7,
        };
    }

    #[test]
    fn failure_keeps_the_original_version_and_failing_step() {
        let stored = from_slice(
            br#"{"billing.invoice.issued":{"1":{"legacy":"INV-13","count":13}}}"#,
        ).unwrap();
        let error = Invoice::from_versioned(stored).unwrap_err();
        assert_eq!(error.source_version().get(), 1);
        assert_eq!(error.adjacent_from().unwrap().get(), 2);
        assert_eq!(error.adjacent_to().unwrap().get(), 3);
        assert_eq!(error.kind(), DecodeFailureKind::Upcast);
        let cause = error
            .source()
            .unwrap()
            .downcast_ref::<ConvertError>()
            .unwrap();
        assert_eq!(cause.0, 13);
    }
}
```

Run the tests while V3 remains a draft. Then freeze V3 and check the release build:

```sh
cargo test --lib --locked
cargo type-history freeze --package invoice-history \
  --type billing.invoice.issued --version 3
cargo build --release --locked
```

## Field history reference

Compile-time checks cover:

- **Additions:** An attribute records which version introduced a field and what
  fallback value to use when reading older records.
- **Type changes:** An attribute records which version changed a field’s type and
  specifies a conversion from the old type to the new type.
- **Renames:** You mark the old field as removed and add the new field in the same
  version. A conversion reads the whole previous record and returns the value for
  the new field.
- **Removals:** The field stays in the history declaration, with an attribute
  recording which version removed it. Earlier versions retain it so older data
  remains readable. The current type omits it, so consumer code that still accesses
  it fails to compile.

Field attributes record when each change happened and how to construct the next
version. Write versions as `v1`, `v2`, and so on. Version zero and values that
overflow the version number are rejected.

Two terms appear in this reference:

- **Boundary:** A version where a field is added, changed, or removed.
- **Retained floor:** The earliest supported version, normally V1. A field without
  history attributes exists from this version.

The latest boundary determines the current version. If no fields have changed,
the retained floor is also the current version.

| Attribute key | Where it goes and what it declares |
| --- | --- |
| `stable_id = "billing.invoice.issued"` | On the struct; fixes the history's identity independently of its Rust name |
| `retained_from = vN` | On the struct; starts an imported history at N; requires a matching ledger and explicit import |
| `added_at = vN` | Field first exists in N; provide one backfill or conversion function |
| `updated_at = vN` | Field changes in N; provide its previous type and one conversion function |
| `removed_at = vN` | Field disappears in N; keep its source declaration for earlier versions |
| `previous_type = Old` | Required on an update; the field's type in the immediately previous version |
| `backfill_value = expression` | Addition only; produces the field's value in the version where it was added |
| `cast_from_previous_value = path` | Update only; a synchronous function consumes the previous field value |
| `cast_from_previous_payload = path` | Addition or update; a synchronous function borrows the complete previous record |

### Values and conversion functions

An addition or update must explain how to construct the destination field.
Choose one rule for each change:

| Change | Required rule | Conversion signature |
| --- | --- | --- |
| Addition | One backfill value or one record callback | `fn(&InvoiceVn) -> Result<AddedType, E>` for a callback |
| Update | One field callback or one record callback, plus `previous_type` | `fn(PreviousType) -> Result<Destination, E>` or `fn(&InvoiceVn) -> Result<Destination, E>` |
| Removal | None | None; record callbacks can still read the removed field from the previous version |

For ordinary histories, `E` must implement `std::error::Error + Send + Sync + 'static`.
For redacted histories, it must implement `ToRedacted`; see the
[callback error contract](docs/redactable.md#callback-errors-and-causes).
Return `Ok(value)` when a conversion cannot fail.

An optional field still needs an explicit value for older data. For example,
adding a reference to `ReceiptCreated` in V3 could use `None`:

<!-- reference:optional-field.rs -->
```rust
#[history(added_at = v3, backfill_value = None)]
pub reference: Option<String>,
```

This is a field fragment for the history declaration. New `ReceiptCreated` constructors
must supply `reference`, even when its value is `None`.
`backfill_value = Type::default()` is also allowed when that default is right for older records.

### Order and lifetime of a field

A field can be added once, updated several times, and removed once.
Write its history attributes newest first, as on the invoice's `count` field:

<!-- reference:ordered-field.rs -->
```rust
#[history(updated_at = v3, previous_type = u64, cast_from_previous_value = stringify)]
#[history(updated_at = v2, previous_type = u32, cast_from_previous_value = widen)]
pub count: String,
```

The field uses `String` in V3. The attributes preserve its earlier types: `u64` in
V2 and `u32` in V1.

- Each attribute uses a different version above the retained floor.
- Updates follow the field's addition and precede its removal.
- A removed field name stays reserved for its earlier versions.

When an added field later changes type, its backfill must produce the type it
had when first added. The next update's `previous_type` records that earlier type.
Each conversion then receives the type from the immediately previous version.

### Rename, merge, or split fields

Use `cast_from_previous_payload` when a new value depends on other fields.
The conversion borrows the complete previous record and returns one destination
field's value. For the invoice's rename, the field declarations are:

<!-- reference:rename-fields.rs -->
```rust
#[history(removed_at = v2)]
pub legacy: String,

#[history(added_at = v2, cast_from_previous_payload = label)]
pub label: String,
```

The callback reads `legacy` from the previous version:

<!-- reference:rename-callback.rs -->
```rust
fn label(previous: &InvoiceV1) -> Result<String, ConvertError> {
    Ok(previous.legacy.clone())
}
```

The current `Invoice` has `label` and no `legacy` field. Code accessing
`invoice.legacy` therefore fails compilation. Keep `legacy` in the declaration
so Type History can still read V1.

The same mechanism supports:

- **Merging fields:** Remove the old fields and add a field whose conversion combines their values.
- **Splitting a field:** Remove the old field and add new fields, each with its own conversion.
  Each conversion reads the original value from the previous record.

All record callbacks run before fields move. They read the previous record's
values, including removed fields. They cannot read another callback's output.
Clone a value when the new field needs ownership.

### Changes that keep the same type

A field can change meaning while keeping its Rust type. Record that change with
`updated_at`, the same type as `previous_type`, and a conversion function. For
example, V3 could require uppercase currency codes in the receipt:

<!-- reference:same-type-field.rs -->
```rust
#[history(updated_at = v3, previous_type = String, cast_from_previous_value = uppercase)]
#[history(added_at = v2, backfill_value = "USD".to_owned())]
pub currency: String,
```

The conversion is an ordinary function. `Infallible` is useful when it always succeeds:

<!-- reference:same-type-callback.rs -->
```rust
use std::convert::Infallible;

fn uppercase(value: String) -> Result<String, Infallible> {
    Ok(value.to_ascii_uppercase())
}
```

A V2 receipt with `"eur"` now loads as a current receipt with `"EUR"`.
New values must already follow the application's currency rules; historical
conversions do not validate current constructors. Fields without a change at V3
carry their values forward.

Every new version needs a field boundary. There is no separate version header
for adding a version with no field changes.

## Rejected declarations

The compiler checks both the history attributes and the functions they name.
For example, it rejects an addition without a value for older records or a
conversion that accepts the wrong previous type.

For example, this field says when `revision` appears but leaves its earlier value
undefined:

```text
#[history(added_at = v2)]
pub revision: u32,
```

The V2 example supplies `backfill_value = 7_u32` to complete the declaration.

Each edit below applies to the complete V2 invoice source and fails a Cargo build:

| Rejected edit | Diagnostic excerpt | Correction |
| --- | --- | --- |
| Remove `backfill_value = 7_u32` | `an added field requires` | Supply the value for earlier invoices through a backfill or record callback |
| Replace `previous_type = u32` with `from = u32` | `unknown history key` | Use `previous_type` |
| Remove `cast_from_previous_value = widen` | `an updated field requires` | Supply one conversion |
| Add a record callback beside the backfill | `exactly one assignment rule` | Choose either the backfill or the callback |
| Change `widen(value: u32)` to `widen(value: u16)` | `mismatched types` | Accept the previous field's exact type, `u32` |
| Return `Result<String, ConvertError>` from `widen` | `mismatched types` | Return the V2 field type, `u64` |
| Change `label` to accept `&InvoiceV2` | `mismatched types` | Borrow the previous version, `&InvoiceV1` |
| Set `updated_at = v0` | `history versions are positive` | Use the next permitted positive version |
| Set `updated_at = v4294967295` above frozen V1 | `exceeds the one permitted successor V2` | Introduce V2 before a later version |

Other rejected declarations include unsupported error types, async callbacks,
capturing closures, duplicate attribute keys, and attributes in the wrong version order.
The [evolution matrix](docs/evolution-matrix.md) records the supported and rejected combinations.

## Decode stored records and handle failures

Wrap a current receipt before serializing it. After deserializing, convert the
versioned value back to the current type:

<!-- decoding:json.rs -->
```rust
use serde_json::{from_slice, to_vec};

let receipt = ReceiptCreated {
    amount_cents: 3000,
    currency: "EUR".to_owned(),
};
let bytes = to_vec(&receipt.into_versioned())?;

// After the application retrieves the bytes:
let stored = from_slice(&bytes)?;
let restored = ReceiptCreated::from_versioned(stored)?;
assert_eq!(restored.currency, "EUR");
```

This uses the V2 receipt from the walkthrough. The generated methods need no
trait imports or manually assembled metadata.

The application chooses where to save the serialized bytes and how to retrieve them.
Type History does not call a database, filesystem, or transport.

### Choose a Serde format

`Versioned` uses two single-entry maps with string keys.
The outer key is the exact stable identity. The inner key is a positive version written as a canonical decimal string.
Its value is the unchanged historical payload.

JSON supports this representation. MessagePack supports it with
[`to_vec_named`](https://docs.rs/rmp-serde/1.3.1/rmp_serde/encode/fn.to_vec_named.html),
which serializes structs with their field names:

<!-- decoding:messagepack.rs -->
```rust
use rmp_serde::{from_slice, to_vec_named};

let bytes = to_vec_named(&receipt.into_versioned())?;
let receipt = ReceiptCreated::from_versioned(from_slice(&bytes)?)?;
```

This fragment assumes an existing `receipt`, an `rmp-serde` dependency, and a function that propagates errors.
Use `serde_json::to_vec` and `serde_json::from_slice` for the equivalent JSON path.

Not every Serde format supports the required maps and named payload fields.
MessagePack's default struct-array encoding does not match the generated map-only payload decoder.
Each chosen format must also support the field types used by the history.

Deserializing selects the historical type but does not run conversions.
`from_versioned` applies the declared adjacent conversions and returns the current alias.
You can inspect the source version before choosing to convert:

<!-- decoding:source-version.rs -->
```rust
use serde_json::from_slice;
use type_history::Versioned;

let stored: Versioned<ReceiptCreated> = from_slice(
    br#"{"shop.receipt.created":{"1":{"amount_cents":1200}}}"#,
)?;
assert_eq!(stored.source_version().get(), 1);

let receipt = ReceiptCreated::from_versioned(stored)?;
let current = receipt.into_versioned();
assert_eq!(current.source_version().get(), 2);
```

Serializing `stored` before conversion would preserve V1. Serializing `current`
writes V2. Neither operation changes bytes already held in storage.

### Invalid records

The historical payload must match its declared version. For example, V1 has no
`currency` field, so this input is rejected:

<!-- decoding:invalid.rs -->
```rust
use serde_json::from_slice;
use type_history::Versioned;

let invalid = br#"{"shop.receipt.created":{"1":{"amount_cents":1200,"currency":"USD"}}}"#;
assert!(from_slice::<Versioned<ReceiptCreated>>(invalid).is_err());
```

Deserializing a `Versioned` fails for:

- A different stable identity or an unsupported version, before reading the payload.
- Empty maps, extra entries, or a noncanonical version such as `"01"`.
- Malformed input or a sequence in place of a record map.
- Unknown fields, duplicate fields, or missing required fields.

A missing supported `Option` field may decode as `None`.
The selected Serde format returns these errors. With JSON, their type is `serde_json::Error`.
The fields present in a payload never determine its version.

### Conversion failures

`ReceiptCreated::from_versioned` returns a `DecodeError` if an ordinary history's conversion fails.
This separate step preserves the callback's concrete error without reducing it to a Serde error message.

The invoice rejects a count of 13 during its V2-to-V3 conversion. Read such an
invoice from V1 to inspect both the original version and the failed step:

<!-- decoding:conversion-error.rs -->
```rust
use serde_json::from_slice;
use std::error::Error;

let stored = from_slice(
    br#"{"billing.invoice.issued":{"1":{"legacy":"INV-13","count":13}}}"#,
)?;
let error = Invoice::from_versioned(stored).unwrap_err();

assert_eq!(error.source_version().get(), 1);
assert_eq!(error.adjacent_from().unwrap().get(), 2);
assert_eq!(error.adjacent_to().unwrap().get(), 3);
let cause = error.source().unwrap().downcast_ref::<ConvertError>().unwrap();
assert_eq!(cause.0, 13);
```

`Error::source()` preserves the concrete error returned by the conversion.

`DecodeError` implements `Debug`, `Display`, and `std::error::Error`.
It does not implement `Serialize`, `Clone`, or equality traits.
Histories that opt into Redactable use [separate error types](docs/redactable.md).

### Existing raw JSON payloads

The low-level JSON APIs remain available for data whose metadata is already stored separately.
Pass the original identity, source version, and raw payload together:

<!-- decoding:raw-json.rs -->
```rust
use type_history::{decode, PayloadVersion, TypeIdentity};

let identity = TypeIdentity::try_from("shop.receipt.created".to_owned())?;
let version = PayloadVersion::try_from_raw(1)?;
let bytes = br#"{"amount_cents":1200}"#;
let receipt = decode::<ReceiptCreated>(&identity, version, bytes)?;

assert_eq!(receipt.currency, "USD");
```

The two constructors validate metadata supplied at runtime.

This API checks identity and version before parsing the exact historical payload.
It then applies adjacent conversions. `ReadError` distinguishes identity mismatch from `DecodeError`.
The source chain preserves the original JSON or callback error when one exists.
`Invoice::history().decode(...)` skips the identity check; `history()` requires the `HasHistory` trait in scope.

Raw JSON has no versioned wrapper, so deserializing it as `Versioned<ReceiptCreated>` fails.
After loading it through `decode`, `into_versioned` prepares the current value for
serialization with its identity and version.
Any decision to replace stored data remains with the application.

## Frozen shapes and business meaning

Freezing protects the structure of earlier records. The build checks the actual
types resolved by the compiler against the saved schema, including nested types.
This catches changes made through type aliases or dependencies as well as direct edits.

### What builds reject

- **Changed stable identity:** Changing a frozen history's `stable_id` fails the
  build, even when its Rust name or module changes.
- **Changed historical fields:** A frozen version must keep its serialized field
  names, types, and nested structure.
- **Deleted history:** A frozen history or retained version cannot disappear from
  the declaration during ordinary development.
- **Invalid ledger:** A missing or malformed ledger fails the build. Duplicate
  JSON keys, version gaps, and inconsistent metadata are rejected too.

If a nested record needs a new structure, keep its earlier definition and add a
field update that converts it to the new type.

### What conversion tests must check

Both of these values pass the same type check:

<!-- reference:business-values.rs -->
```rust
let usd: String = "USD".to_owned();
let eur: String = "EUR".to_owned();
assert_ne!(usd, eur);
```

Only `"USD"` matches the shop's earlier receipts. The compiler checks types;
the [receipt conversion test](#6-read-v1-and-write-v2) checks that business rule.
A conversion can return the wrong value while preserving the expected type.

Schemas describe serialized structure, so some source edits leave them unchanged:

- Replacing a Rust type with another that has an equivalent serialized structure.
- Reordering fields, which can change the order in which callbacks run.
- Removing a conversion that preserves the schema, if other field boundaries
  still describe every retained version.

Test conversions with representative historical records. Make each conversion
depend on its input: avoid I/O, clocks, randomness, and externally visible side effects.

The generated conversion chain runs from older versions to the current type.
It does not provide conversions back to older versions. Rust callers also need
updating when the current type's fields change, as the removal example shows.

## Lifecycle commands

The CLI creates and updates the ledger used by Cargo builds. Run its commands
from your application's workspace.

- **Choose a package:** Every command that writes a ledger requires `--package NAME`.
- **Choose a version:** Supply `--type EXACT_ID` and `--version N` together when
  selecting one version. Wildcards are rejected.
- **Choose build settings:** Use `--features LIST` or `--target TRIPLE` when needed.
  Custom target files are unsupported.

The tutorial uses `init` and `freeze`. `check` validates existing histories.
The remaining commands handle experimental resets or imported histories.

| Command | Effect |
| --- | --- |
| `cargo type-history init --package invoice-history` | Create the ledger before any history declaration exists |
| `cargo type-history check` | Check configured workspace libraries without writing package files |
| `cargo type-history check --package invoice-history` | Check one configured library |
| `cargo type-history freeze --package invoice-history` | Freeze all ordinary drafts after validating the complete package |
| `cargo type-history freeze --package invoice-history --type billing.invoice.issued --version 3` | Freeze exactly one ordinary draft or reset reservation |

### Iterate and discard an ordinary draft

WARNING: Draft edits can make development records unreadable. Freeze a version
before relying on its representation for data you must retain.

After freezing V2, you can edit V3 repeatedly while developing its conversion.
Each history allows one new draft at a time. Freeze V3 before adding V4.

To discard an unfrozen version:

1. Restore the complete source for the last frozen version.
2. Update callers to use that version's fields and types.
3. Build again. No ledger command is needed.

For the invoice, restoring V2 makes `Invoice` an alias of `InvoiceV2` again.
Removing only the V3 attribute while keeping `count: String` would change frozen
V2, so the build rejects it. Any remaining V3 boundary keeps the draft present.

A history with no frozen versions can be deleted entirely.

### Reset and undo

WARNING: Replacing a frozen schema can make stored records unreadable. Use reset
only for disposable experimental data. Add a successor version for durable records.

Reset lets you replace the highest frozen version during experiments. It is
allowed only when there is no newer draft. The ledger keeps the original schema
and marks that version as open for replacement, called a **reset reservation**.

```sh
cargo type-history reset --package invoice-history \
  --type billing.invoice.issued --version 3
```

While the reservation exists:

- Development builds allow edits to that version. Builds requiring frozen
  versions, including release builds, reject the reservation.
- Earlier versions remain protected.
- You cannot add a newer version or clear the reservation by deleting source.

Finish a reset in one of two ways:

**Accept the replacement:** Freeze the exact identity and version after editing
and testing the declaration. Freezing the whole package rejects pending reservations.

```sh
cargo type-history freeze --package invoice-history \
  --type billing.invoice.issued --version 3
```

**Undo the reset:** Restore source that matches the saved schema, then clear the
reservation. Undo does not edit source; a schema mismatch leaves the reservation open.

```sh
cargo type-history reset --undo --package invoice-history \
  --type billing.invoice.issued --version 3
```

### Import a retained floor

Use import when bringing an existing history and its ledger into a package.
Some histories start above V1. Their retained floor is the earliest version the
program supports.

Import requires a complete ledger in the current format. It cannot change or
remove entries already frozen in the destination ledger.

Each imported floor above V1 requires a matching `--floor EXACT_ID=N` argument.
The declaration must state the same floor:

```text
use type_history::versioned;

#[versioned(stable_id = "billing.invoice.issued", retained_from = v4)]
```

This fragment belongs to a separate history starting at V4. Unannotated fields
start at V4, and Type History generates no V1, V2, or V3 types for it.
Do not apply it to the tutorial's existing V1-to-V3 history to discard old versions.

The source and imported ledger must agree:

- The ledger contains every declared identity and supported version.
- The floor annotation matches the imported floor and its `--floor` argument.
- The declaration's floor or field boundaries describe its latest version.

The annotation alone cannot supply missing historical schemas.

With the declaration and `imported.json` prepared, import them together:

```sh
cargo type-history import --package invoice-history \
  --from imported.json --floor billing.invoice.issued=4
```

### Failed commands and recovery

Ledger writes are validated before the file is replaced. A package lock prevents
concurrent writes, and the command checks that its source inputs have not changed.
The lock stays held until the command reads back the result.

Recovery depends on whether the replacement happened:

- **Before replacement:** A busy lock, changed input, or validation failure leaves
  the ledger unchanged. Temporary candidate files can be discarded.
- **After replacement:** The error includes `COMMITTED`. Inspect the live ledger
  before retrying because the operation has already changed it.

Each package is updated separately. These commands do not modify stored record bytes.

## Build profiles and environment

Development builds let you work on a draft. Release builds require every version
to be frozen. To apply the same requirement during development:

```sh
TYPE_HISTORY_REQUIRE_FROZEN=1 cargo build --locked
```

| Consumer profile or setting | Ordinary draft or reset reservation |
| --- | --- |
| Development | Allowed with warning |
| `TYPE_HISTORY_REQUIRE_FROZEN=1 cargo build` | Rejected |
| `AXIS_REQUIRE_FROZEN_EVENTS=1 cargo build` | Rejected |
| Release family | Rejected |
| Custom profile inheriting `dev` | Development rules |
| Custom profile inheriting `release` | Release rules |

`AXIS_REQUIRE_FROZEN_EVENTS` is also accepted for compatibility with Axis tooling.
Both strict variables must be unset or exactly `1`. Empty strings, `0`, and `true`
fail, even when the other variable equals `1`. Neither can relax release checks.
Custom profiles follow their inherited family, regardless of optimization or `debug_assertions`.

### When checks run

Cargo builds check source and configuration against the committed ledger.
Changes to the ledger, relevant modules, manifests, or strict settings rerun the
checks. Compiler assertions also check changed nested dependencies when build
output is cached. Builds do not freeze, repair, or rewrite the ledger.

### Package configuration

- A consumer needs a library target, the `build.rs` hook, and `format = 1` in
  `type-history.toml`. Depending on the runtime alone does not enable ledger checks.
- The ledger lives at `type-history/schemas.json`; its location is fixed.
- No `links` value, manifest metadata, or second declaration file is needed.
- A package cannot configure both standalone and Axis ledgers.

### Variables used by tooling

The build hook sets `TYPE_HISTORY_LEDGER_PATH`, `TYPE_HISTORY_SCHEMA_ID_PREFIX`, and
`TYPE_HISTORY_AUTHORITY_KIND`. Do not set them yourself.

`TYPE_HISTORY_SCHEMA_EXPORT` and `AXIS_SCHEMA_EXPORT` are used by export tooling.
They must be unset or `1` and cannot relax release or explicit strict checks.
Lifecycle export uses a captured copy of development sources. Forced Cargo
environment settings remain effective; conflicting forced settings fail.

## Modules, dependency names, and nested values

### Modules and dependency names

Put histories in ordinary Rust modules and import the macro explicitly.
Types, conversion functions, and expressions resolve where the declaration
appears, including supported `self`, `super`, and `crate` paths.

For a renamed runtime dependency, Cargo's dependency key becomes the crate name:

```toml
[dependencies]
history_api = { package = "type-history", version = "0.1" }
```

Use that name in the module containing the history:

<!-- reference:renamed-dependency.rs -->
```rust
use history_api::versioned;

#[versioned(stable_id = "shop.delivery.requested")]
pub struct Delivery {
    pub postal_code: String,
}
```

If you rename `type-history-build`, use that name in `build.rs` too.
The following rules also apply:

- **Dependency tables:** Runtime dependencies can be target-specific or inherited
  from the workspace. History declarations themselves must be unconditional.
- **Generated names:** Avoid collisions with names such as `InvoiceV1` and
  `InvoiceV2`. Raw identifiers keep their Rust spelling; `r#type` is stored as `type`.
  Duplicate serialized field names are rejected.
- **Source directories:** A source directory can be named `target`. Lifecycle
  snapshots exclude Cargo's actual build output directory.

### Nested records

A field can contain another named struct. Type History needs to know that struct's
schema so a later edit cannot silently change a frozen record's nested fields.

For a supporting struct such as `Address`:

1. Derive `type_history::Schema` to describe its structure for the build checks.
2. Derive Serde's `Serialize` and `Deserialize` for reading and writing its values.
3. Add `#[serde(deny_unknown_fields)]` so decoding rejects unexpected fields.

`Schema` supplies `ResolvedSchema` and schema export support. The other derives
supply serialization; constructors and application validation remain your code's responsibility.

Add this module to an initialized library:

<!-- example:nested-example.rs -->
```rust
pub mod addresses {
    use serde::{Deserialize, Serialize};
    use type_history::{versioned, Schema};

    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Schema)]
    #[serde(deny_unknown_fields)]
    pub struct Address {
        pub number: u32,
    }

    #[versioned(stable_id = "billing.address.record")]
    pub struct AddressRecord {
        pub address: Address,
    }

    #[test]
    fn nested_values_keep_their_closed_codec() {
        use serde_json::from_slice;
        use type_history::Versioned;

        let stored = from_slice(
            br#"{"billing.address.record":{"1":{"address":{"number":7}}}}"#,
        ).unwrap();
        assert!(AddressRecord::from_versioned(stored).is_ok());
        assert!(from_slice::<Versioned<AddressRecord>>(
            br#"{"billing.address.record":{"1":{"address":{"number":7,"unknown":1}}}}"#,
        ).is_err());
    }
}
```

The test accepts an address with `number` and rejects one with an unknown field.
Adding this module creates a new V1 draft. Freeze it before a release build.

Nested types follow these additional rules:

- Public supporting records may contain private nested field types.
- Generated history versions and current aliases can also be nested fields,
  including through `Option`. Their schema is the selected version's structure.
- Other Serde or Schemars attributes that change the representation are rejected.
- If a supporting type uses Schemars directly, use `schemars = "=1.2.2"`.

## Supported scope

History declarations use structs with named fields, including empty structs.
Supported field types include:

| Kind | Supported types |
| --- | --- |
| Scalars | `bool`, `String`, and signed or unsigned integers from 8 to 128 bits |
| Containers | Vectors, fixed arrays, and optional values with supported element types |
| Nested records | Named records with schema support, including generated history versions |

Array length is part of the schema. `Vec<u8>` has a dedicated byte schema;
other vectors use a sequence schema.

The authoring API currently rejects:

- **Unsupported shapes:** Enums, tuple structs, unit structs, generics, and recursive histories.
- **Unsupported field types:** Floats, pointer-sized integers, maps, sets, and nested options.
- **Unsupported callbacks:** Async functions and functions that consume the whole previous record.
- **Conditional declarations:** The set of histories must not change with conditional compilation.
  Supported feature-selected type aliases still undergo frozen schema checks.
- **Serialization overrides:** Serde `rename`, `default`, `flatten`, `skip`, `alias`, and `with`.
  Use history attributes to describe field changes.

Generated code grows with the number of supported versions. The ledger limits
which version ranges can be generated, and these checks run before expansion.
A history starting above V1 needs an explicit [import](#import-a-retained-floor).

## Optional Redactable integration

If records contain sensitive data, the optional Redactable integration provides
redacted output and errors for the histories you select. It is disabled by default.

To use it:

1. Enable the `redactable` feature on `type-history`, `type-history-build`, and the CLI.
2. Select each history with `redacted` and declare its field policies.

For example, this separate receipt history hides the customer in redacted output:

<!-- reference:redacted-history.rs -->
```rust
use type_history::versioned;

#[versioned(stable_id = "billing.receipt.recorded", redacted)]
pub struct ReceiptRecorded {
    #[sensitive]
    pub customer: String,
    #[not_sensitive]
    pub amount: u32,
}
```

Request a redacted projection when displaying the value:

<!-- reference:redacted-output.rs -->
```rust
use type_history::redactable::ToRedacted;

let receipt = ReceiptRecorded { customer: "Alice".to_owned(), amount: 42 };
let safe = receipt.to_redacted();
assert_eq!(safe.json()["customer"], "[REDACTED]");
assert_eq!(safe.json()["amount"], 42);
```

Enabling the feature alone does not change ordinary histories, even when another
dependency enables it. They keep the tutorial's traits and error types.
Ordinary Serde serialization still writes the original field values, including
for redacted histories. Persist those payloads; use redacted output for display.

The [integration guide](docs/redactable.md) covers setup, field policies, nested
records, callback errors, and renamed dependencies, using Redactable 0.12.0.

## Crates and further reading

Applications normally depend on `type-history` and `type-history-build`.
The workspace separates runtime code, code generation, and development tooling:

| Crate | Responsibility |
| --- | --- |
| `type-history` | Public macros, history traits, and decoding API |
| `type-history-core` | Runtime decoding, identities, and structural schema contracts |
| `type-history-macros` | Procedural macro entry points |
| `type-history-codegen` | Parsing, schema checks, and Rust generation |
| `type-history-build` | Build integration and checked ledger operations |
| `cargo-type-history` | Cargo subcommand for lifecycle operations |

For more detail:

- The [evolution matrix](docs/evolution-matrix.md) lists supported and rejected cases.
- The [example guide](docs/guide.md) explains what the receipt and invoice tests check.
- The [tutorial integration tests](crates/cargo-type-history/tests/standalone/tutorial.rs) check intermediate edits and expected failures.

The [invoice reference application](examples/invoice/README.md) includes a complete
history declaration, frozen ledger, and tests. See [Contributing](CONTRIBUTING.md)
for repository build and test commands.

## License

Licensed under either [MIT](LICENSE-MIT) or [Apache 2.0](LICENSE-APACHE).
