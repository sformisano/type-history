# Typehistory

Typehistory is a Rust library that lets you evolve structs while preserving
support for data written using earlier versions.

It works in three steps:

1. You describe field changes and conversion rules through attributes on your
   struct’s fields.
2. Typehistory generates a Rust type for each supported version and the code that
   applies your conversion rules between versions.
3. Compile-time checks reject struct changes that lack the required Typehistory
   attributes, so you catch those mistakes before deployment rather than at
   runtime in production.

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

## Table of contents

- [When to use Typehistory](#when-to-use-typehistory)
- [Quick example](#quick-example)
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
- [Origin in Axis](#axis-integration)
- [Crates and development](#crates-and-development)

## When to use Typehistory

Persisted data often outlives the code that wrote it. A production application
may need to read years of records after its Rust structs have changed.

Typehistory helps when those records need to become the types your application
uses today. Common examples include:

- **Stored events:** Read events written before a field was added or changed.
- **Saved snapshots:** Restore a record written by an earlier application version.
- **Versioned documents:** Read records with different stored versions through
  one current Rust type.

Your application decides how records are stored and retrieved. Typehistory
provides the versioned Rust types and conversions needed to work with earlier records.

## Quick example

Suppose a shop originally accepted only US dollars. Its `Receipt` record stored
an amount in cents. The shop now accepts other currencies, so new receipts need
a `currency` field as well.

Earlier receipts have no currency field, but their amounts still mean US dollars.
We can therefore assign `"USD"` when converting them to the new type.
Typehistory records that rule alongside the field and generates both versions
of `Receipt`.

The tutorial follows this change from start to finish:

1. Declare the original receipt as V1 and freeze its schema.
2. Add currency in V2, with `"USD"` as the value for older receipts.
3. Test reading V1 and writing V2, then freeze V2 for release.

## Start an empty package

### 1. Create the package

The crates are not yet published on crates.io. This tutorial uses a local
checkout of the private repository, which requires repository access.
You can also [depend on an exact Git revision](#use-a-git-revision).

Three packages have different jobs:

| Package | Why the example needs it |
| --- | --- |
| `typehistory` | Provides the history declaration, generated types, and decoder |
| `typehistory-build` | Makes Cargo check that previously frozen versions remain intact |
| `cargo-typehistory` | Creates the schema file and freezes new versions |

Use the repository's pinned Rust 1.97.1 toolchain. Choose a directory outside
this repository's workspace for the new package.

Set `TYPEHISTORY_CHECKOUT` to the absolute path of your checkout.

```sh
export RUSTUP_TOOLCHAIN=1.97.1
export TYPEHISTORY_CHECKOUT=/absolute/path/to/typehistory
cargo install --path "$TYPEHISTORY_CHECKOUT/crates/cargo-typehistory" --locked
cargo new --lib receipt-history --edition 2024
cd receipt-history
```

Replace `Cargo.toml` with this manifest.

```toml
[package]
name = "receipt-history"
version = "0.0.0"
edition = "2024"
publish = false
build = "build.rs"

[workspace]

[dependencies]
typehistory = { path = "/absolute/path/to/typehistory/crates/typehistory" }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0.149"

[build-dependencies]
typehistory-build = { path = "/absolute/path/to/typehistory/crates/typehistory-build" }
```

Replace both checkout paths with your actual absolute path. Cargo does not expand
environment variables inside TOML. The tests use `serde_json` to write receipts.
The nested record example later in this README also uses Serde derives.

### 2. Initialize the schema file

A **schema** describes a record's serialized field names, field types, and nested
structure. Typehistory saves frozen schemas in a file called the **ledger**.
Later builds compare your declaration with that file to check that older versions
remain intact.

The setup uses three files:

- `typehistory.toml` enables Typehistory for the package.
- `build.rs` runs the schema checks during Cargo builds.
- `typehistory/schemas.json` stores the ledger. The `init` command creates it.

Create `typehistory.toml`:

```toml
format = 1
```

Create `build.rs`:

```rust
fn main() {
    typehistory_build::compile();
}
```

Empty `src/lib.rs` before initialization.

```sh
cargo generate-lockfile
cargo fetch --locked
cargo typehistory init --package receipt-history
```

`init` creates an empty ledger at `typehistory/schemas.json`. Run it before adding
the first history declaration. It refuses to replace an existing ledger.
Cargo builds require this file even when the package has no histories yet.

Lifecycle commands run with `--locked --offline`. Fetching dependencies first
allows their compiler checks to run without network access. After changing the
manifest, update the lockfile and fetch the required dependencies before using
those commands again.

Add this line to `.gitignore`:

```gitignore
**/typehistory/.schemas.lock
```

Commit `Cargo.toml`, `Cargo.lock`, `typehistory.toml`, `build.rs`, your source, and
`typehistory/schemas.json` with your project. The `.schemas.lock` file prevents
concurrent commands from changing the ledger at the same time. It can be recreated.

### 3. Declare V1

Write this complete `src/lib.rs`:

<!-- quick-example:v1.rs -->
```rust
#[typehistory::history(id = "shop.receipt.created")]
pub struct Receipt {
    pub amount_cents: u64,
}
```

This declaration gives Typehistory two pieces of information:

- **Identity:** `shop.receipt.created` identifies this history in stored records.
  Keep it unchanged when renaming the Rust type or moving its module.
- **Initial fields:** `amount_cents` belongs to V1. Fields without history
  attributes exist from V1 by default.

Typehistory generates `ReceiptV1` with an `amount_cents` field. It also makes
`Receipt` an alias of `ReceiptV1`, so application code can use the original name.

### 4. Freeze V1

The first version starts as a **draft**, which you can edit during development.
Development builds allow drafts with a warning. Freezing V1 saves its schema in
the ledger and makes later builds reject changes to that schema.

Build the declaration, then freeze V1:

```sh
cargo build --locked
cargo typehistory freeze --package receipt-history
```

Changing `amount_cents` from `u64` to `String` now fails compilation because it
would change frozen V1. Describe further changes in V2 so V1 remains available.

### 5. Add currency in V2

Replace `src/lib.rs` with this complete declaration:

<!-- quick-example:v2.rs -->
```rust
#[typehistory::history(id = "shop.receipt.created")]
pub struct Receipt {
    pub amount_cents: u64,

    #[history(added_at = v2, backfill_value = "USD".to_owned())]
    pub currency: String,
}
```

The new field's attribute describes both parts of the addition:

- **Version:** `added_at = v2` introduces `currency` in V2.
- **Value for older records:** `backfill_value` supplies `"USD"`
  when converting a V1 receipt, because all V1 receipts used US dollars.

One declaration now generates both record versions:

| Type | Fields |
| --- | --- |
| `ReceiptV1` | `amount_cents: u64` |
| `ReceiptV2` | `amount_cents: u64`, `currency: String` |
| `Receipt` | Alias of `ReceiptV2` |

The conversion keeps the amount and fills in the currency. V1's schema stays
unchanged. V2 becomes the new draft, and application code using `Receipt` now
sees both fields.

### 6. Read V1 and write V2

A V1 receipt can be represented as this JSON:

```json
{"amount_cents":1200}
```

This example uses Typehistory's JSON decoder. Pass these bytes to
`decode::<Receipt>` with the history identity and version V1.
The decoder then:

1. Checks that the identity matches `Receipt::TYPE`.
2. Uses the version to read the JSON as `ReceiptV1`.
3. Converts it into `ReceiptV2`, assigning `"USD"` to the new field.

Append these tests to `src/lib.rs`:

<!-- quick-example:runtime-tests.rs -->
```rust
#[cfg(test)]
mod receipt_tests {
    use super::Receipt;
    use typehistory::{HasHistory, PayloadVersion, decode};

    #[test]
    fn read_an_old_receipt() {
        let receipt = decode::<Receipt>(
            &Receipt::TYPE,
            PayloadVersion::INITIAL,
            br#"{"amount_cents":1200}"#,
        )
        .unwrap();

        assert_eq!(receipt.amount_cents, 1200);
        assert_eq!(receipt.currency, "USD");
        assert_eq!(Receipt::VERSION.get(), 2);
    }

    #[test]
    fn write_a_current_receipt() {
        let receipt = Receipt {
            amount_cents: 3000,
            currency: "EUR".to_owned(),
        };
        let identity = Receipt::TYPE;
        let version = Receipt::VERSION;
        let bytes = serde_json::to_vec(&receipt).unwrap();

        // Persist identity, version, and bytes together in your storage format.
        let restored = decode::<Receipt>(&identity, version, &bytes).unwrap();
        assert_eq!(restored, receipt);
        assert_eq!(restored.currency, "EUR");
    }
}
```

These tests check two uses of the same current type:

- **Read an old receipt:** V1 bytes become a `Receipt` with amount 1200 and currency
  `"USD"`.
- **Write a new receipt:** A `Receipt` with currency `"EUR"` keeps that value when
  serialized and read back as V2.

The backfill runs only when converting older records. New `Receipt` values must
supply their currency explicitly. Decoding leaves the stored bytes unchanged.
The decoder needs the history identity and version alongside its JSON input.
Your application decides how to store or obtain that information.

Run the tests:

```sh
cargo test --lib --locked
```

### 7. Freeze V2 for release

Development builds let you revise V2 while testing its conversion. Release builds
require all histories to be frozen, so an unfinished draft cannot enter a release.

Freeze V2, then check the release build:

```sh
cargo typehistory freeze --package receipt-history \
  --type shop.receipt.created --version 2
cargo build --release --locked
```

The ledger now records both V1 and V2. Commit its update with the changed
declaration and conversion tests. The next change can introduce V3.

### Use a Git revision

An exact Git revision can replace the local checkout dependencies. Both library
packages and the installed CLI must come from that revision.

Replace the dependency sections in `Cargo.toml` with these entries:

```toml
[dependencies]
typehistory = { git = "ssh://git@github.com/sformisano/typehistory.git", rev = "FULL_COMMIT_ID" }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0.149"

[build-dependencies]
typehistory-build = { git = "ssh://git@github.com/sformisano/typehistory.git", rev = "FULL_COMMIT_ID" }
```

Replace `FULL_COMMIT_ID` with one full revision containing the unpublished crates.

Install the CLI from that revision:

```sh
cargo install --git ssh://git@github.com/sformisano/typehistory.git \
  --rev FULL_COMMIT_ID --locked cargo-typehistory
```

Update the lockfile and fetch dependencies before the next lifecycle command.

## Design principles

The receipt example uses a few rules that also apply to longer histories:

- **One history, one identity:** The identity connects stored records to their
  history even when Rust type names change.
- **One Rust type per version:** Earlier types keep the fields that existed in
  that version. Application code can use the current alias or a specific version.
- **One conversion per step:** Reading V1 as V3 first converts V1 to V2, then V2
  to V3. Each new version builds on the existing conversions.
- **Explicit values for old data:** You choose what an added field means for
  older records. Typehistory cannot infer that choice from the field's type.

The ledger is needed when building the application. The running application uses
generated types and conversion functions, so it needs neither the ledger nor Cargo.

## What the macro generates

The struct you write is the history declaration. The macro turns it into the Rust
types your application uses. In the receipt example, those are `ReceiptV1`,
`ReceiptV2`, and the current alias `Receipt`.

| Generated item | What it provides |
| --- | --- |
| `ReceiptV1`, `ReceiptV2`, and later types | One struct with the fields and types for that version |
| `Receipt` | Alias of the latest generated type, including an allowed draft |
| Record implementations | `Clone`, `Debug`, `PartialEq`, Serde serialization and deserialization, and schema support |
| `HasHistory` on the current type | `TYPE` identifies the history, `VERSION` identifies the current version, and `history()` provides its decoder |
| `History<Receipt>` | Lists supported versions and decodes their bytes into the current `Receipt` |

By default, every field type must implement `Clone`, `PartialEq`, and `Debug`,
including types used only in earlier versions. Generated `Debug` calls each
field's own `Debug` implementation. Histories that opt into
[Redactable support](#optional-redactable-integration) use different formatting and error contracts.

The macro owns the generated implementations, so do not add derives to the history
declaration. You can add documentation and `allow`, `warn`, or `deny` attributes.
Put field naming lints on the record, following Rust's usual lint scope.
Other attributes that transform the declaration are unsupported.

A history identity such as `billing.invoice.issued` follows these rules:

- At least three segments, separated by dots.
- Each segment starts with a lowercase letter and contains only lowercase
  letters, digits, or underscores.
- At most 255 ASCII bytes in total.

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

Choose a directory outside both `receipt-history` and this repository's workspace.

```sh
cargo new --lib invoice-history --edition 2024
cd invoice-history
```

1. Copy the [setup manifest](#1-create-the-package).
2. Set `name = "invoice-history"` in its `[package]` section.
3. Create `typehistory.toml` and `build.rs` using the [initialization instructions](#2-initialize-the-schema-file).
4. Empty `src/lib.rs`.

Initialize the package:

```sh
cargo generate-lockfile
cargo fetch --locked
cargo typehistory init --package invoice-history
```

Continue in `invoice-history` for the following steps.

### V1: retain the original fields

The first invoice has a `legacy` label and a count stored as `u32`.

Write this complete `src/lib.rs`:

<!-- example:versions/v1.rs -->
```rust
#[typehistory::history(id = "billing.invoice.issued")]
pub struct Invoice {
    pub legacy: String,
    pub count: u32,
}
```

`Invoice` currently aliases `InvoiceV1`. Freeze this starting point before changing it:

```sh
cargo build --locked
cargo typehistory freeze --package invoice-history
```

### V2: remove, convert, and add fields

V2 makes three changes:

- **Rename `legacy` to `label`:** Mark `legacy` as removed and add `label` in V2.
  Keep the `legacy` declaration so Typehistory can generate `InvoiceV1`.
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

#[derive(Debug)]
pub struct ConvertError(pub u64);

impl Display for ConvertError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        write!(formatter, "count {} cannot be converted", self.0)
    }
}

impl Error for ConvertError {}

#[typehistory::history(id = "billing.invoice.issued")]
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
```

The two conversion functions take different inputs:

- **`widen` takes the old field value:** `cast_from_previous_value` passes it the
  previous `count` by value. It returns the count as `u64`.
- **`label` takes the old record:** `cast_from_previous_payload` passes it
  `&InvoiceV1`. It clones `legacy` to produce the new label.

The rename needs access to the whole previous record because V1 has no field
called `label`. Typehistory calls record conversions before moving any fields,
so `legacy` remains available until `label` has read it.

Conversion functions, also called callbacks, return `Result`. These two always
return `Ok`; V3 introduces a conversion that can fail.

Build and freeze V2:

```sh
cargo build --locked
cargo typehistory freeze --package invoice-history \
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

#[derive(Debug)]
pub struct ConvertError(pub u64);

impl Display for ConvertError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        write!(formatter, "count {} cannot be converted", self.0)
    }
}

impl Error for ConvertError {}

#[typehistory::history(id = "billing.invoice.issued")]
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
    use super::{ConvertError, Invoice, InvoiceV3};
    use std::error::Error;
    use typehistory::{decode, DecodeFailureKind, HasHistory, PayloadVersion, ReadError};

    #[test]
    fn old_bytes_reach_the_current_type() {
        let value = decode::<Invoice>(
            &Invoice::TYPE,
            PayloadVersion::INITIAL,
            br#"{"legacy":"INV-42","count":42}"#,
        )
        .unwrap();
        assert_eq!(value.count, "42");
        assert_eq!(value.label, "INV-42");
        assert_eq!(value.revision, 7);
        assert_eq!(Invoice::VERSION.get(), 3);
        let _: Invoice = InvoiceV3 {
            count: "42".to_owned(),
            label: "current".to_owned(),
            revision: 7,
        };
    }

    #[test]
    fn failure_keeps_the_original_version_and_failing_step() {
        let failure = decode::<Invoice>(
            &Invoice::TYPE,
            PayloadVersion::INITIAL,
            br#"{"legacy":"INV-13","count":13}"#,
        )
        .unwrap_err();
        let ReadError::Decode(error) = &failure else {
            panic!("expected a conversion failure");
        };
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
cargo typehistory freeze --package invoice-history \
  --type billing.invoice.issued --version 3
cargo build --release --locked
```

## Field history reference

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
| `id = "billing.invoice.issued"` | On the struct; identifies the history independently of its Rust name |
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

Defaults are explicit choices:

- An added `Option` still needs `backfill_value = None` or a conversion function.
- `backfill_value = Type::default()` is allowed when that default is right for older records.

### Order and lifetime of a field

A field can be added once, updated several times, and removed once.
Write its history attributes newest first:

- Each attribute uses a different version above the retained floor.
- Updates follow the field's addition and precede its removal.
- A removed field name stays reserved for its earlier versions.

When an added field later changes type, its backfill must produce the type it
had when first added. The next update's `previous_type` records that earlier type.
Each conversion then receives the type from the immediately previous version.

### Rename, merge, or split fields

Use `cast_from_previous_payload` when a new value depends on other fields.
The conversion borrows the complete previous record and returns one destination
field's value. It supports several kinds of change:

- **Rename:** Remove the old field and add the new field in the same version.
  The conversion reads the old name, as `label` reads `legacy` in the invoice example.
- **Merge:** Remove the old fields and add a field whose conversion combines their values.
- **Split:** Remove the old field and add new fields, each with its own conversion.
  Each conversion reads the original value from the previous record.

All record callbacks run before fields move. They read the previous record's
values, including removed fields. They cannot read another callback's output.
Clone a value when the new field needs ownership.

### Changes that keep the same type

A field can change meaning while keeping its Rust type. Record that change with
`updated_at`, the same type as `previous_type`, and a conversion function.
Fields without a change at that version carry their values forward.

Every new version needs a field boundary. There is no separate version header
for adding a version with no field changes.

## Rejected declarations

The compiler checks both the history attributes and the functions they name.
For example, it rejects an addition without a value for older records or a
conversion that accepts the wrong previous type.

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

For example, this field says when `revision` appears but leaves its earlier value
undefined:

```text
#[history(added_at = v2)]
pub revision: u32,
```

The V2 example supplies `backfill_value = 7_u32` to complete the declaration.
Other rejected declarations include unsupported error types, async callbacks,
capturing closures, duplicate attribute keys, and attributes in the wrong version order.
The [evolution matrix](docs/evolution-matrix.md) records the supported and rejected combinations.

## Decode stored records and handle failures

The current `decode` API accepts JSON. Use
`decode::<Invoice>(&identity, version, bytes)` with these inputs:

- **Identity:** Which history the record belongs to. Validate an identity obtained
  at runtime with `TypeIdentity::try_from(String)`.
- **Version:** Which version the input represents. Convert a raw number with
  `PayloadVersion::try_from_raw(u32)`; zero is invalid.
- **Bytes:** The record's JSON payload.

Your application prepares these inputs from its own storage representation.

The decoder checks the identity and version before parsing JSON. It then reads
the matching historical type and applies each conversion up to the current version.
The fields present in the JSON do not determine its version.

`Invoice::history().retained_versions()` lists the accepted versions.
Calling `Invoice::history().decode(...)` directly skips the identity check, so
use it only after selecting the correct history yourself.

### Invalid records

Generated record decoders accept JSON objects. Reading fails for:

- An unsupported version, before attempting to parse its bytes.
- Malformed JSON or a sequence in place of a record object.
- Unknown fields, duplicate fields, or missing required fields.

A missing supported `Option` field may decode as `None`.

### Conversion failures

For ordinary histories, `ReadError` separates an identity mismatch from a
`DecodeError`. A `DecodeError` identifies whether the version was unsupported,
JSON decoding failed, or a conversion failed.

When a conversion fails, its error includes:

- **Source version:** The version originally read from storage.
- **Failed step:** The previous and destination versions of that conversion.
- **Cause:** The error returned by your conversion function, available through `Error::source()`.

For example, reading V1 can fail in the V2-to-V3 conversion. The error still
reports V1 as the source version, as the invoice test demonstrates.

`ReadError` and `DecodeError` implement `Debug`, `Display`, and `std::error::Error`.
The source chain preserves the original JSON or conversion error when one exists.
These error types do not implement `Serialize`, `Clone`, or equality traits.
Histories that opt into Redactable use [separate error types](docs/redactable.md).

Decoding returns a current record or an error. It neither rewrites stored bytes
nor checks a schema fingerprint at runtime. Your application decides what to do
with the result.

## Frozen shapes and business meaning

Freezing protects the structure of earlier records. The build checks the actual
types resolved by the compiler against the saved schema, including nested types.
This catches changes made through type aliases or dependencies as well as direct edits.

### What builds reject

- **Changed historical fields:** A frozen version must keep its serialized field
  names, types, and nested structure.
- **Deleted history:** A frozen history or retained version cannot disappear from
  the declaration during ordinary development.
- **Invalid ledger:** A missing or malformed ledger fails the build. Duplicate
  JSON keys, version gaps, and inconsistent metadata are rejected too.

If a nested record needs a new structure, keep its earlier definition and add a
field update that converts it to the new type.

### What conversion tests must check

The compiler can check that a backfill returns a `String`. It cannot determine
whether `"USD"` is the correct currency for an old receipt. A conversion can also
return the wrong value while preserving the expected type.

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
| `cargo typehistory init --package invoice-history` | Create the ledger before any history declaration exists |
| `cargo typehistory check` | Check configured workspace libraries without writing package files |
| `cargo typehistory check --package invoice-history` | Check one configured library |
| `cargo typehistory freeze --package invoice-history` | Freeze all ordinary drafts after validating the complete package |
| `cargo typehistory freeze --package invoice-history --type billing.invoice.issued --version 3` | Freeze exactly one ordinary draft or reset reservation |
| `cargo typehistory reset --package invoice-history --type billing.invoice.issued --version 3` | Reserve the highest frozen version for experimental replacement |
| `cargo typehistory reset --undo --package invoice-history --type billing.invoice.issued --version 3` | Clear that reservation after source matches the saved frozen schema |
| `cargo typehistory import --package invoice-history --from imported.json --floor billing.invoice.issued=4` | Validate and import a complete ledger with the stated retained floor |

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

While the reservation exists:

- Development builds allow edits to that version. Builds requiring frozen
  versions, including release builds, reject the reservation.
- Earlier versions remain protected.
- You cannot add a newer version or clear the reservation by deleting source.

Finish a reset in one of two ways:

- **Accept the replacement:** Run `freeze` with the exact identity and version.
  Freezing the whole package rejects pending reservations.
- **Undo the reset:** Restore source that matches the saved schema, then run
  `reset --undo` with the exact identity and version. Undo does not edit source;
  a schema mismatch leaves the reservation open.

### Import a retained floor

Use import when bringing an existing history and its ledger into a package.
Some histories start above V1. Their retained floor is the earliest version the
program supports.

Import requires a complete ledger in the current format. It cannot change or
remove entries already frozen in the destination ledger.

Each imported floor above V1 requires a matching `--floor EXACT_ID=N` argument.
The declaration must state the same floor:

```text
#[typehistory::history(id = "billing.invoice.issued", retained_from = v4)]
```

This fragment belongs to a separate history starting at V4. Unannotated fields
start at V4, and Typehistory generates no V1, V2, or V3 types for it.
Do not apply it to the tutorial's existing V1-to-V3 history to discard old versions.

The source and imported ledger must agree:

- The ledger contains every declared identity and supported version.
- The floor annotation matches the imported floor and its `--floor` argument.
- The declaration's floor or field boundaries describe its latest version.

The annotation alone cannot supply missing historical schemas.

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
to be frozen. You can also require frozen versions in a development build with
`TYPEHISTORY_REQUIRE_FROZEN=1`.

| Consumer profile or setting | Ordinary draft or reset reservation |
| --- | --- |
| Development | Allowed with warning |
| `TYPEHISTORY_REQUIRE_FROZEN=1 cargo build` | Rejected |
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
  `typehistory.toml`. Depending on the runtime alone does not enable ledger checks.
- The ledger lives at `typehistory/schemas.json`; its location is fixed.
- No `links` value, manifest metadata, or second declaration file is needed.
- A package cannot configure both standalone and Axis ledgers.

### Variables used by tooling

The build hook sets `TYPEHISTORY_LEDGER_PATH`, `TYPEHISTORY_SCHEMA_ID_PREFIX`, and
`TYPEHISTORY_AUTHORITY_KIND`. Do not set them yourself.

`TYPEHISTORY_SCHEMA_EXPORT` and `AXIS_SCHEMA_EXPORT` are used by export tooling.
They must be unset or `1` and cannot relax release or explicit strict checks.
Lifecycle export uses a captured copy of development sources. Forced Cargo
environment settings remain effective; conflicting forced settings fail.

## Modules, dependency names, and nested values

### Modules and dependency names

Put histories in ordinary Rust modules and import the macro explicitly.
Types, conversion functions, and expressions resolve where the declaration
appears, including supported `self`, `super`, and `crate` paths.

The following rules also apply:

- **Renamed dependencies:** Use `package = "typehistory"` and invoke the macro
  through the dependency's chosen name, such as `#[history_api::history(...)]`.
  If you rename `typehistory-build`, use that name in `build.rs` too.
- **Dependency tables:** Runtime dependencies can be target-specific or inherited
  from the workspace. History declarations themselves must be unconditional.
- **Generated names:** Avoid collisions with names such as `InvoiceV1` and
  `InvoiceV2`. Raw identifiers keep their Rust spelling; `r#type` is stored as `type`.
  Duplicate serialized field names are rejected.
- **Source directories:** A source directory can be named `target`. Lifecycle
  snapshots exclude Cargo's actual build output directory.

### Nested records

A field can contain another named struct. Typehistory needs to know that struct's
schema so a later edit cannot silently change a frozen record's nested fields.

For a supporting struct such as `Address`:

1. Derive `typehistory::Schema` to describe its structure for the build checks.
2. Derive Serde's `Serialize` and `Deserialize` for reading and writing its values.
3. Add `#[serde(deny_unknown_fields)]` so decoding rejects unexpected fields.

`Schema` supplies `ResolvedSchema` and schema export support. The other derives
supply serialization; constructors and application validation remain your code's responsibility.

Add this module to an initialized library:

<!-- example:nested-example.rs -->
```rust
pub mod addresses {
    #[derive(
        Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, typehistory::Schema,
    )]
    #[serde(deny_unknown_fields)]
    pub struct Address {
        pub number: u32,
    }

    #[typehistory::history(id = "billing.address.record")]
    pub struct AddressRecord {
        pub address: Address,
    }

    #[test]
    fn nested_values_keep_their_closed_codec() {
        use typehistory::{HasHistory, PayloadVersion};

        assert!(AddressRecord::history()
            .decode(PayloadVersion::INITIAL, br#"{"address":{"number":7}}"#)
            .is_ok());
        assert!(AddressRecord::history()
            .decode(
                PayloadVersion::INITIAL,
                br#"{"address":{"number":7,"unknown":1}}"#,
            )
            .is_err());
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

1. Enable the `redactable` feature on `typehistory`, `typehistory-build`, and the CLI.
2. Select each history with `#[history(id = "billing.receipt.recorded", redacted)]`.
3. Declare its field policies and use the redacted decoding API from the
   [integration guide](docs/redactable.md).

Enabling the feature alone does not change ordinary histories, even when another
dependency enables it. They keep the tutorial's traits and error types.
Ordinary Serde serialization still writes the original field values, including
for redacted histories. Persist those payloads; use redacted output for display.

The guide covers field policies, nested records, callback errors, and renamed
dependencies, with examples using Redactable 0.12.0.

## Axis integration

Typehistory was extracted from Axis's event history implementation. The
[Axis authoring guide at the extraction revision](https://github.com/sformisano/axis/blob/af7b96c3f0744cdfd170c50bb321d5dfd02be8ea/docs/design/domain-authoring-macros.md#event-field-history)
describes its original use in `aggregate!` declarations.

The [generation interfaces](docs/redactable.md#axis-and-other-macro-frontends)
allow an integrating framework to supply its own errors and traits.
A standalone consumer tests the interfaces intended for Axis. Updating Axis's
own macros to use them is separate integration work.

## Crates and development

Applications normally depend on `typehistory` and `typehistory-build`.
The workspace separates runtime code, code generation, and development tooling:

| Crate | Responsibility |
| --- | --- |
| `typehistory` | Public macros, history traits, and decoding API |
| `typehistory-core` | Runtime decoding, identities, and structural schema contracts |
| `typehistory-macros` | Procedural macro entry points |
| `typehistory-codegen` | Parsing, schema checks, and Rust generation |
| `typehistory-build` | Build integration and checked ledger operations |
| `cargo-typehistory` | Cargo subcommand for lifecycle operations |

For more detail:

- The [evolution matrix](docs/evolution-matrix.md) lists supported and rejected cases.
- The [example guide](docs/guide.md) explains what the receipt and invoice tests check.
- The [tutorial integration tests](crates/cargo-typehistory/tests/standalone/tutorial.rs) check intermediate edits and expected failures.

The [invoice reference application](examples/invoice/README.md) is a runnable
Cargo package with a committed history ledger. Try it with
`cargo run -p invoice-history --locked`.

Install the pinned Rust toolchain with rustup. Git is needed to clone the repository.
Set `TMPDIR` to a short disk path with room for disposable consumer builds.

Run from the repository root:

```sh
cargo fetch --locked
cargo fmt --all --check
cargo clippy --workspace --all-targets --no-default-features --locked --target-dir target/clippy -- -Dwarnings
cargo test --workspace --all-targets --no-default-features --locked
cargo test --workspace --doc --no-default-features --locked
cargo doc --workspace --no-deps --no-default-features --locked
cargo build --workspace --release --no-default-features --locked

cargo clippy --workspace --all-targets --all-features --locked --target-dir target/clippy -- -Dwarnings
cargo test --workspace --all-targets --all-features --locked
cargo test --workspace --doc --all-features --locked
cargo doc --workspace --no-deps --all-features --locked
cargo build --workspace --release --all-features --locked

cargo run -p invoice-history --release --locked
cargo run -p cargo-typehistory --locked -- check --package invoice-history
```

Set `RUSTDOCFLAGS=-Dwarnings` when checking documentation, as CI does.
GitHub Actions runs these Cargo checks in both feature modes. Workspace tests
include the reference application and Rust integration tests that compile the
marked README blocks directly, exercise lifecycle commands, and check expected
failures. Normal builds use `target`; Clippy uses `target/clippy`.

## License

Licensed under either [MIT](LICENSE-MIT) or [Apache 2.0](LICENSE-APACHE).
