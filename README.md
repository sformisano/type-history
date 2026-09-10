# Typehistory

Typehistory describes how a Rust record changes over time. You keep its field
additions, changes, removals, and conversion rules in one declaration. Typehistory
generates the earlier record types and the conversions between consecutive versions.

This keeps earlier versions available while application code uses the latest
type. Once you freeze a version, Typehistory saves its schema in a committed
file. Later Cargo builds reject changes to that frozen schema or deletion of
history that the file still records.

Typehistory also reads earlier records into the current type. Its current decoder
uses JSON and an explicit version number to choose the starting type. Your
application handles storage and delivery.

## Table of contents

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
- [Origin in Axis](#axis-integration)
- [Crates and development](#crates-and-development)

## Quick example

Suppose a shop originally accepted only US dollars. Its `Receipt` record needed
an amount in cents, because every receipt had the same currency. The shop now
accepts other currencies, so the next record version needs a `currency` field.

Earlier receipts have no such field. Reading one as the new type requires a
choice: which currency should it receive? In this example, the shop's history
justifies assigning `"USD"`. Typehistory records that choice alongside the new
field and generates both versions of `Receipt`.

The following tutorial starts from an empty package. It introduces V1, freezes
it, then adds and tests V2 in the same package.

## Start an empty package

### 1. Create the package

The crates are pre-release and are not published on crates.io. This setup uses
an authorized checkout of the private repository. A
[Git dependency option](#use-a-git-revision) follows the complete tutorial.

Three packages have different jobs:

| Package | Why the example needs it |
| --- | --- |
| `typehistory` | Provides the history declaration, generated types, and decoder |
| `typehistory-build` | Checks historical schemas whenever Cargo builds the package |
| `cargo-typehistory` | Initializes the schema file and freezes new versions |

Use the repository's pinned Rust 1.97.1 toolchain. Choose a directory outside
this repository's workspace for the new package.

Set `TYPEHISTORY_CHECKOUT` to the absolute path of your checkout.

```sh
export RUSTUP_TOOLCHAIN=1.97.1
export TYPEHISTORY_CHECKOUT=/absolute/path/to/typehistory
cargo install --path "$TYPEHISTORY_CHECKOUT/crates/typehistory/cargo-typehistory" --locked
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
typehistory = { path = "/absolute/path/to/typehistory/crates/typehistory/typehistory" }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0.149"

[build-dependencies]
typehistory-build = { path = "/absolute/path/to/typehistory/crates/typehistory/build" }
```

Replace both checkout paths with your actual absolute path. Cargo does not expand
environment variables inside TOML. The tests use `serde_json` to write a current
receipt. Serde is also available for the supporting types shown later.

### 2. Initialize the schema file

A schema describes a version's serialized field names, field types, and supported
nested structure.

Typehistory uses `build.rs` to check each build against previously frozen schemas.
The package configuration enables those checks, and a schema file records what
they must preserve. This file is called the **ledger**.

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

`init` creates the empty ledger at `typehistory/schemas.json`. It requires a
package with no history declarations and refuses to replace an existing ledger.
Ordinary builds require this file, even before the first history exists.

Lifecycle commands run with `--locked --offline`. Fetching dependencies first
allows their compiler checks to run without network access. After changing the
manifest, update the lockfile and fetch the required dependencies before using
those commands again.

Add this line to `.gitignore`:

```gitignore
**/typehistory/.schemas.lock
```

Commit the manifest, Cargo lockfile, configuration, build hook, source, and ledger
with your project. The `.schemas.lock` file only coordinates concurrent commands;
it is disposable.

### 3. Declare V1

Write this complete `src/lib.rs`:

<!-- quick-example:v1.rs -->
```rust
#[typehistory::history(id = "shop.receipt.created")]
pub struct Receipt {
    pub amount_cents: u64,
}
```

The attribute gives this history the stable identity `shop.receipt.created`.
That identity stays the same if you later rename the Rust type or move its module.

Typehistory generates a concrete `ReceiptV1` struct containing `amount_cents`.
It also makes `Receipt` an alias of `ReceiptV1`, so application code can use the
original name. A field with no history annotation exists from V1 by default.

### 4. Freeze V1

The first version starts as a **draft**: you can still change it during development.
A development build accepts the draft and prints a warning. Freezing records
its schema in the ledger, which makes later builds check that V1 remains intact.

Build the declaration, then freeze V1:

```sh
cargo build --locked
cargo typehistory freeze --package receipt-history
```

For example, changing the frozen V1 field from `u64` to `String` now fails a
build. That edit would change the representation of an existing version.
Adding V2 describes the change while preserving V1.

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

`added_at = v2` says that `currency` first exists in V2.
`backfill_value` supplies its value when converting a receipt from before that
addition. The expression assigns `"USD"`, because the example's V1 receipts all
used US dollars.

One declaration now generates both record versions:

| Type | Fields |
| --- | --- |
| `ReceiptV1` | `amount_cents: u64` |
| `ReceiptV2` | `amount_cents: u64`, `currency: String` |
| `Receipt` | Alias of `ReceiptV2` |

The conversion preserves the amount and fills in the currency. V1's schema stays
unchanged. V2 is the new editable draft, and application code using `Receipt`
now sees its two fields.

### 6. Read V1 and write V2

A V1 receipt can be represented as this JSON:

```json
{"amount_cents":1200}
```

`decode::<Receipt>` selects the Receipt history. The decoder checks that the stored
identity matches `Receipt::TYPE`. The stored version selects its starting record type.
Reading these bytes as V1 creates a `ReceiptV1`, then converts it into the current
`ReceiptV2` using the declared backfill.

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

The first test checks that the earlier receipt becomes an amount of 1200 with
currency `"USD"`. The second constructs a current receipt with `"EUR"`, writes
it through Serde, and reads it back as V2.

New receipts supply their currency explicitly. The backfill applies when converting
older records; it does not supply fields for current constructors or rewrite
stored bytes. An application that saves these records must keep identity,
version, and bytes together in its storage format.

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

A history describes the versions your program still supports. Its stable identity
connects them even when Rust names change. Each version has a concrete generated
type, so a conversion function receives the fields that actually existed at that
point in the history.

Conversions run between consecutive versions. Reading V1 as V3 first converts V1
to V2, then V2 to V3. Each step expresses one change. Added fields need an explicit
value or conversion function because the correct interpretation of missing data
depends on the application.

The build hook checks declarations against the ledger. Runtime decoding uses the
generated types and conversion functions, so it needs no ledger or Cargo process.
Freezing protects historical schemas; conversion tests check whether the chosen
values still mean the right thing.

## What the macro generates

`#[typehistory::history(...)]` replaces a named struct declaration with its
versioned structs and a current alias. For the Receipt example, it generates
`ReceiptV1`, `ReceiptV2`, and the alias `Receipt`. A removed field remains present
in the older generated structs where it belonged.

| Generated item | What it provides |
| --- | --- |
| `ReceiptV1`, `ReceiptV2`, and later types | One concrete record for each supported version recorded by the history |
| `Receipt` | Alias of the latest generated type, including an allowed draft |
| Record implementations | `Clone`, `Debug`, `PartialEq`, strict Serde serialization and deserialization, and structural schema support |
| `HasHistory` on the current type | Stable `TYPE`, positive `VERSION`, and the `history()` descriptor |
| `History<Receipt>` | Selection of an earlier version followed by conversions into `Receipt` |

By default, every field type used in a generated version must implement `Clone`, `PartialEq`,
and `Debug`. This includes fields present only in earlier versions. `Debug` formats
each field through that field type's `Debug` implementation.

The macro supplies these implementations. Do not add derives to the outer history
declaration. Documentation attributes and `allow`, `warn`, and `deny` are supported.
Lint attributes follow Rust's usual scope; field naming lints belong on the record.
Other attributes that transform the declaration are unsupported.

The identity has at least three dot-separated segments and at most 255 ASCII bytes.
Each segment starts with a lowercase letter. Its remaining characters are lowercase
letters, digits, or underscores. `billing.invoice.issued` is a valid identity.

## Evolve an invoice through V1, V2, and V3

The Receipt tutorial introduced a field with a constant backfill. This example
adds conversions that consume an earlier field or borrow the entire earlier record.
It also shows how a failed conversion identifies its original version and failing step.

### Prepare a separate invoice package

This example uses a fresh package called `invoice-history`. Its V1 declaration
must exist before V2, just as in the Receipt tutorial.

Choose a directory outside both `receipt-history` and this repository's workspace.

```sh
cargo new --lib invoice-history --edition 2024
cd invoice-history
```

Use the [setup manifest](#1-create-the-package), with `name = "invoice-history"` in its `[package]` section.
Create the same `typehistory.toml` and `build.rs` from [initialization](#2-initialize-the-schema-file).
Empty `src/lib.rs` before running these commands:

```sh
cargo generate-lockfile
cargo fetch --locked
cargo typehistory init --package invoice-history
```

The package now has its own empty ledger. The following steps operate only on
this invoice package.

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

V2 stores the old label under `label`, makes room for larger counts with `u64`,
and records a revision number. This example assigns revision 7 to earlier invoices;
an application must choose a value justified by its own history.

Replacing a field name requires two declarations: removing the old field and
adding the new one. The old `legacy` field remains in `InvoiceV1`, so a conversion
can read it while constructing V2's `label`.

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

`widen` consumes the previous `count` and returns its `u64` value.
`label` borrows `InvoiceV1` because it needs a different field from that record.
Typehistory runs callbacks that borrow the whole record before moving any fields
out of it. The removed `legacy` value is therefore still available to `label`.

Both callbacks return `Result`, even though these two conversions always succeed.
The shared error type becomes useful in V3, where a conversion can fail.

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

The newest update appears first on the field. These annotations describe
`u32` in V1, `u64` in V2, and `String` in V3.
The example rejects 13 solely to demonstrate a conversion failure.

Reading a V1 invoice with count 42 runs `widen`, then `stringify`, producing
the current count `"42"`. A count of 13 passes the first conversion and fails
the second. The error preserves source version V1 and identifies the V2-to-V3 step.

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

A **boundary** is a version where a field is added, changed, or removed.
The **retained floor** is the earliest version kept in a history, normally V1.
An unannotated field exists from that floor. The latest version is the highest
field boundary, or the floor when no field has changed.

Write versions as `v1`, `v2`, and so on. Version zero and overflowing values
are rejected.

| Attribute key | Placement and meaning |
| --- | --- |
| `id = "billing.invoice.issued"` | Required outer attribute identity; stable across Rust renames |
| `retained_from = vN` | Optional outer attribute for an imported retained floor; requires a matching ledger and explicit import |
| `added_at = vN` | Field first exists in N; provide one backfill or conversion function |
| `updated_at = vN` | Field changes in N; provide its previous type and one conversion function |
| `removed_at = vN` | Field disappears in N; keep its source declaration for earlier versions |
| `previous_type = Old` | Required on an update; the field's type in the immediately previous version |
| `backfill_value = expression` | Addition only; produces the field's value in the version where it was added |
| `cast_from_previous_value = path` | Update only; a synchronous function consumes the previous field value |
| `cast_from_previous_payload = path` | Addition or update; a synchronous function borrows the complete previous record |

| Change | Allowed assignments | Callback signature |
| --- | --- | --- |
| Addition | One backfill value or one record callback | `fn(&InvoiceVn) -> Result<AddedType, E>` for a callback |
| Update | One field callback or one record callback, plus `previous_type` | `fn(PreviousType) -> Result<Destination, E>` or `fn(&InvoiceVn) -> Result<Destination, E>` |
| Removal | None | None; record callbacks can still read the removed field from the previous version |

`E` must implement `std::error::Error + Send + Sync + 'static`.
Return `Ok(value)` for a conversion that cannot fail. An added `Option` still
needs an explicit `backfill_value = None` or callback. `Type::default()` is allowed
when the author explicitly chooses it.

### Order and lifetime of a field

A field can be added once, changed several times, and removed once. Its history
attributes appear newest first. Their versions must be distinct, above the retained
floor, after the field's addition, and before its removal where applicable.
A removed field name stays reserved.

When an added field later changes type, its backfill must produce the type it
had when first added. The next update's `previous_type` records that earlier type.
Each conversion then receives the type from the immediately previous version.

### Rename, merge, or split fields

Rename a field by removing its old name and adding a distinct new name at the
same boundary. A record callback reads the old field to produce the new one.
The invoice's `legacy`-to-`label` change follows this pattern.

Merging or splitting fields uses separate destination callbacks that borrow the
same previous record. Each callback sees the intact previous record, never partially
computed output. All record callbacks finish before any fields move. A callback
can clone a value when it needs ownership.

### Changes that keep the same type

An update may preserve a field's Rust type while changing its meaning. It still
requires `previous_type` and a conversion callback. Intermediate versions can
also carry a field unchanged.

The latest field boundary determines the current version. There is no separate
version header for appending a version with no field boundary.

## Rejected declarations

The following edits apply to the complete V2 invoice source. Each fails an
ordinary build. The diagnostic excerpts identify the rule the edit violates.

| Rejected edit | Diagnostic excerpt | Correction |
| --- | --- | --- |
| Remove `backfill_value = 7_u32` | `an added field requires` | Supply the value for earlier invoices through a backfill or record callback |
| Replace `previous_type = u32` with `from = u32` | `unknown history key` | Use `previous_type` |
| Remove `cast_from_previous_value = widen` | `an updated field requires` | Supply one conversion |
| Add a record callback beside the backfill | `exactly one assignment rule` | Keep one assignment |
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
Invalid output types, unsupported error types, async functions, capturing closures,
duplicate keys, and conflicting version order also fail.
The [evolution matrix](crates/typehistory/docs/evolution-matrix.md) records the supported and rejected combinations.

## Decode stored records and handle failures

The application supplies `decode::<Invoice>(&identity, version, bytes)` with an
identity, a positive version, and JSON bytes. Validate a dynamic identity with
`TypeIdentity::try_from(String)` and a raw version with `PayloadVersion::try_from_raw(u32)`.

The decoder checks the identity first. It then selects the exact historical type,
reads the JSON into that type, and converts through each subsequent version.
It never guesses a version from the fields present in the JSON.
`Invoice::history().retained_versions()` lists the accepted versions.
The descriptor's `decode` method assumes the caller already selected the correct identity.

Unknown versions fail before JSON parsing. Generated record deserializers reject
sequences, unknown fields, duplicate fields, malformed JSON, and missing required
fields. An absent supported `Option` field may decode as `None`.

`ReadError` distinguishes an identity mismatch from `DecodeError`. A conversion
failure records the original version and the two versions joined by the failed
step. Reading V1 and failing at V2-to-V3 therefore reports source V1 and adjacent
versions V2 and V3.

Both error types implement ordinary `Debug`, `Display`, and `std::error::Error`.
Their `Error::source()` chain retains the original decoder or conversion error.
Applications can inspect that cause along with Typehistory's version metadata.
The wrappers do not provide `Serialize`, `Clone`, or equality guarantees.

Decoding returns a current record or an error. It reads no ledger and compares
no schema fingerprint at runtime. It does not load application state, run a
reducer, write to a database, or replay events.

## Frozen shapes and business meaning

A schema describes a version's serialized field names, field types, and supported
nested structure. Freezing saves that schema in the ledger. The build checks
compiler-resolved types against it, including nested types, so changing an alias
or dependency cannot silently replace an earlier representation.

Changing a frozen nested structure, deleting a frozen history, or losing a retained
version fails an ordinary build. A missing or malformed ledger also fails.
Ledger parsing rejects duplicate JSON keys, invalid or gapped versions, and
inconsistent metadata. Version ranges are checked before generating their types.

These checks cannot determine whether a conversion returns the right business
value. For example, they cannot establish whether `"USD"` is correct for an old
receipt. A callback can change its answer without changing any schema.

Some source changes also preserve schemas. Equivalent nominal Rust types can
have the same serialized representation. Reordering equivalent fields can pass
while changing callback order. Removing a same-shape semantic step can pass if
the remaining boundaries still describe every retained version.

Test conversion rules against representative historical records. Keep callbacks
deterministic and free of I/O, clocks, randomness, and externally visible side effects.

If a nested value needs a new serialized structure, preserve its earlier definition.
Add a field update that converts the earlier nested type into the new one.
Typehistory does not supply application state versioning, conversions to older
versions, inferred defaults, or recursive business validation.

## Lifecycle commands

Run these commands from the consumer workspace. Every command that writes a
ledger requires one exact `--package NAME`. When selecting a version,
`--type EXACT_ID` and `--version N` must appear together. Wildcards are rejected.

Optional `--features LIST` and `--target TRIPLE` select compiler inputs.
Custom target files are unsupported.

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

After freezing V2, you can revise V3 repeatedly in development. A history allows
one ordinary latest draft at a time. Freeze V3 before appending V4.

To discard the invoice's unfrozen V3, restore the complete V2 source and its callers.
That restores the earlier field types and removes V3's fields and boundaries.
`Invoice` becomes an alias of `InvoiceV2` again. No ledger command is needed.

Removing only the annotation while keeping `count: String` changes frozen V2,
so the build rejects it. Any remaining V3 boundary keeps the draft present.
A history containing only a draft can be deleted entirely.

### Reset and undo

WARNING: Replacing a frozen schema can make stored records unreadable. Use reset
only for disposable experimental data. Add a successor version for durable records.

Reset opens the highest frozen version for experimental edits. It requires no
successor draft. The ledger keeps the original schema and records a **reservation**
for its replacement. Earlier versions remain protected.

Development builds permit edits to the reserved version, but strict builds reject
the reservation. You cannot append a successor while it exists. Deleting source
does not remove it.

Freeze the exact reserved identity and version to accept its replacement.
A package-wide freeze rejects pending reservations, so that decision must be explicit.

To undo a reset, restore source that produces the saved schema. Then run
`reset --undo` for the exact identity and version. Undo does not edit source;
a mismatch leaves the reservation pending.

### Import a retained floor

Some imported histories begin above V1. Their **retained floor** identifies the
earliest version the program still keeps. Import requires a complete ledger in
the current format and cannot change or remove existing frozen entries.

Each imported floor above V1 requires a matching `--floor EXACT_ID=N` argument.
The declaration must state the same floor:

```text
#[typehistory::history(id = "billing.invoice.issued", retained_from = v4)]
```

This fragment describes a separate history beginning at V4. In that history,
unannotated fields start at V4 and no V1-to-V3 types are generated. It is not
an edit to the walkthrough's V1-to-V3 invoice.

The imported ledger must contain every declared identity and retained version.
A floor annotation alone cannot establish those earlier schemas.
The source must also express the imported latest version through its floor or
field boundaries; import does not silently trim history that cannot be expressed.

### Failed commands and recovery

Commands that change a ledger take a package lock and capture the compiler inputs.
They validate a candidate ledger and check that source has not changed before
atomically replacing the file. The lock remains held until the result is read back.

A busy lock, stale input, or failure before replacement leaves the ledger unchanged.
Temporary candidate files are disposable.

A failure after replacement reports `COMMITTED`. Inspect the live ledger before
retrying, because the operation has already changed it. Commands do not promise
a transaction across packages or modify stored record bytes.

## Build profiles and environment

Every normal build checks source and configuration against the committed ledger.
Changes to the ledger, relevant modules, manifests, or strict settings invalidate
the checks. Builds never freeze, repair, or rewrite the ledger.
Compiler assertions also check changed nested dependencies when build output is cached.

| Consumer profile or setting | Ordinary draft or reset reservation |
| --- | --- |
| Development | Allowed with warning |
| `TYPEHISTORY_REQUIRE_FROZEN=1 cargo build` | Rejected |
| `AXIS_REQUIRE_FROZEN_EVENTS=1 cargo build` | Rejected |
| Release family | Rejected |
| Custom profile inheriting `dev` | Development rules |
| Custom profile inheriting `release` | Release rules |

Both strict variables accept only unset or exactly `1`. Empty strings, `0`, and
`true` fail, even when the other variable equals `1`. Neither variable can relax
release checks. The consuming profile's family determines these rules, independently
of `debug_assertions` and optimization.

`TYPEHISTORY_SCHEMA_EXPORT` and `AXIS_SCHEMA_EXPORT` are tooling signals.
They accept only unset or `1` and cannot relax release or explicit strict checks.
Lifecycle export uses captured development sources. Forced Cargo environment
settings remain effective; conflicting forced settings fail.

The build hook supplies `TYPEHISTORY_LEDGER_PATH`, `TYPEHISTORY_SCHEMA_ID_PREFIX`,
and `TYPEHISTORY_AUTHORITY_KIND`. Do not set them yourself. The package requires
a library target and accepts only `format = 1` in `typehistory.toml`.
No `links` value, manifest metadata, or second declaration file is needed.
The ledger location is fixed at `typehistory/schemas.json`.

A package cannot configure both standalone and Axis ledgers. Depending on the
Typehistory runtime does not itself enable standalone ledger checks.

## Modules, dependency names, and nested values

### Modules and dependency names

Histories use ordinary `mod` declarations and explicit macro imports. A renamed
dependency works through `package = "typehistory"`. Invoke its chosen name,
such as `#[history_api::history(...)]`. Renaming the build dependency likewise
changes the call in `build.rs`. Generated paths follow those dependency names.

Runtime dependencies can use ordinary or target-specific dependency tables,
including renamed and workspace-inherited entries. History declarations themselves
must remain unconditional. Source directories may be named `target`; lifecycle
snapshots exclude Cargo's resolved output directory.

Types, callback paths, and expressions resolve where the original declaration
appears. Supported `self`, `super`, and `crate` paths keep that meaning.
Raw identifiers keep their Rust spelling; `r#type` has the serialized key `type`.
Duplicate canonical names and collisions with generated `InvoiceVn` names fail.

### Nested records

A history field can contain a separate named record. Deriving `typehistory::Schema`
on that supporting record describes its nested structure for frozen checks.
The derive creates `ResolvedSchema` and schema export support. It does not supply
Serde implementations, constructors, or business validation.

Nested records must use the supported closed representation, which rejects
unknown fields. Pair ordinary Serde derives with `#[serde(deny_unknown_fields)]`.
Other Serde or Schemars attributes that change the representation are rejected.
If supporting types use Schemars directly, keep `schemars = "=1.2.2"`.

Public supporting records can contain private nested field types. Generated history
versions and their current aliases can also be nested fields, including through
`Option`. Their schema describes the selected version's payload structure.

The following module adds an independent `AddressRecord` history to an initialized
library. Its `Address` value has a schema and ordinary Serde implementations.

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
Adding this module creates a new V1 draft, which must be frozen before a release build.

## Supported scope

History declarations support concrete named records, including empty records.
Supported field types include `bool`, signed and unsigned 8-to-128-bit integers,
`String`, vectors, fixed arrays, optional values, and nested resolved records.
`Vec<u8>` uses the existing byte representation; array length is part of its schema.

The authoring API does not support floats, pointer-sized integers, enums,
tuple or unit history structs, generics, recursive histories, maps, sets, or nested
options. Asynchronous callbacks and callbacks that consume a whole record are
also unsupported.

Conditional history inventories and Serde `rename`, `default`, `flatten`, `skip`,
`alias`, and `with` attributes are rejected. Supported feature-selected aliases
remain subject to compiler-resolved frozen checks.

Generated code grows with the retained version range. Version ranges are checked
against the ledger before allocation or expansion, so an unauthorized huge range
is rejected. A history starting above V1 needs an explicit retained-floor import.

## Optional Redactable integration

Enable the default-disabled `redactable` feature on the runtime, build hook, and
CLI. Then select `#[history(id = "billing.receipt.recorded", redacted)]` for each
history that needs redacted records and errors. Ordinary histories keep the
tutorial's traits and error contracts, including when another dependency enables
the feature.

The [optional integration guide](crates/typehistory/docs/redactable.md) covers
field policies, nested records, safe errors, renamed dependencies, and compiled examples.
It uses the published Redactable 0.12.0 contracts. Payload serialization remains unchanged.

## Axis integration

Typehistory was extracted from Axis's event field-history implementation.
The [generation interfaces](crates/typehistory/docs/redactable.md#axis-and-other-macro-frontends)
let Axis retain its own structured errors and event traits. A standalone fixture
checks those interfaces; the existing Axis extraction checkout requires separate integration work.
The [Axis authoring guide at the extraction revision](https://github.com/sformisano/axis/blob/af7b96c3f0744cdfd170c50bb321d5dfd02be8ea/docs/design/domain-authoring-macros.md#event-field-history)
records that origin and its `aggregate!` syntax.

At that revision, Axis used `schema_build::compile()`, `axis/event-schemas.json`,
and `cargo axis` commands for its event histories. Axis also owned aggregate roles,
reducers, state, and delivery. The pinned guide describes that revision; current
standalone Typehistory behavior is defined by this repository.

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

The [evolution matrix](crates/typehistory/docs/evolution-matrix.md) lists supported
and rejected cases. The [guide](crates/typehistory/docs/guide.md) explains what
the receipt and invoice checks establish.

Install the pinned Rust toolchain with rustup, plus Bash, Git, and ripgrep.
Set `TMPDIR` to a short disk path with room for disposable consumer builds.

Run from the repository root:

```sh
cargo fetch --locked
bash scripts/verify.sh
```

The script runs both feature modes for Clippy, workspace tests, doctests, and public documentation,
and the executable receipt and invoice examples. GitHub Actions runs the same script.
The example verifier compares the marked README blocks with their source files
before running them. Normal builds use `target`; Clippy uses `target/clippy`.

## License

Licensed under either [MIT](LICENSE-MIT) or [Apache 2.0](LICENSE-APACHE).
