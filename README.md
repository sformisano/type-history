# Type History

Type History is a Rust library that lets you evolve structs while preserving
support for data written using earlier versions. It uses field attributes
to describe changes, and compile-time checks to catch missing or incorrect
attributes before they cause runtime failures in production.

## Use Cases

Persisted data often outlives the code that wrote it. As a production
application evolves, it may accumulate years of records written using different
versions of its Rust structs. Type History converts those records into the
current types, so application code can use them without handling each
historical version separately.

Common examples include:

- **Stored events:** Read events written before a field was added, changed, or removed.
- **Saved snapshots:** Restore a record written by an earlier application version.
- **Versioned documents:** Read records with different stored versions through
  one current Rust type.

## Installation

Add the runtime and build hook to your package, then install the CLI:

```sh
cargo add type-history@0.1
cargo add --build type-history-build@0.1
cargo install cargo-type-history --version 0.1 --locked
```

Type History requires Rust 1.97 or later. Follow the
[package setup](https://github.com/sformisano/type-history/blob/main/book/src/setup.md)
to connect the build hook and initialize the schema ledger.

## How to Use

A shop initially accepts only US dollars. Its `ReceiptCreated` event starts with
an amount in cents.

Later, the shop adds other currencies and supports refunds.

To run this example, first [set up a `receipt-history` library](https://github.com/sformisano/type-history/blob/main/book/src/setup.md).

<!-- ANCHOR: journey -->
### 1. Declare V1

Describe the first version with `#[versioned(...)]`:

<!-- quick-example:v1.rs -->
```rust
use type_history::versioned;

#[versioned(stable_name = "shop.receipt.created")]
pub struct ReceiptCreated {
    pub amount_cents: u64,
}
```

- `#[versioned(...)]` creates the type's history.
- `stable_name` names that history. Every `ReceiptCreated` version will share this name.
- Fields without `#[history(...)]` attributes belong to V1.

Type History generates `ReceiptCreatedV1` and makes the original name an alias:

```rust
pub type ReceiptCreated = ReceiptCreatedV1;
```

Application code uses `ReceiptCreated`. The alias always points to the latest
version, so callers do not need to choose a numbered type.

### 2. Freeze V1

V1 starts as a **draft**. You can add, change, or remove fields while developing
and testing without introducing V2.

The important thing to keep in mind is:

- Development builds allow drafts with a warning.
- Release builds **reject** drafts and require every version to be frozen.

Changes to a draft can make even previously written types unreadable.

So we need to freeze V1 before storing receipts that the application will need to read later:

```sh
cargo type-history freeze --package receipt-history
```

The command checks the package and saves V1's **schema** in
`type-history/schemas.json`, called the **ledger**. A schema describes the
serialized field names, types, and nested structure.

Setup created an empty ledger, `{}`. Freezing V1 adds this entry, written in
JSON Schema format:

<!-- journey:ledger-v1.json -->
```json
{
  "shop.receipt.created": {
    "1": {
      "metadata": {},
      "schema": {
        "$id": "urn:typehistory:schema:shop.receipt.created",
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "additionalProperties": false,
        "properties": {
          "amount_cents": {
            "format": "uint64",
            "minimum": 0,
            "type": "integer"
          }
        },
        "required": ["amount_cents"],
        "type": "object"
      }
    }
  }
}
```

- `shop.receipt.created` is the history's `stable_name`.
- `1` is the version being frozen. Freezing preserves V1 rather than creating V2.
- `schema` records that V1 requires an `amount_cents` field containing a `u64`.

Commit the ledger with your code. Later builds compare V1 against its saved schema
and reject changes to its serialized structure. Further changes must be declared
in V2.

The frozen `stable_name` must also stay unchanged. You can still rename the Rust
type or move it to another module.

### 3. Add a field in V2

The shop now accepts other currencies. Add a `currency` field and supply `"USD"`
for receipts written before the field existed:

<!-- quick-example:v2.rs -->
```rust
use type_history::versioned;

#[versioned(stable_name = "shop.receipt.created")]
pub struct ReceiptCreated {
    pub amount_cents: u64,

    #[history(added_in = v2, backfill_value = "USD".to_owned())]
    pub currency: String,
}
```

- `#[history(...)]` describes the change to this field.
- `added_in = v2` introduces `currency` in V2.
- `backfill_value` supplies `"USD"` when converting V1 receipts, because the shop
  accepted only US dollars then. New receipts supply their currency explicitly.

Type History preserves V1 and generates V2 with the new field. With trait
implementations omitted, the generated structs look like this:

```rust
pub struct ReceiptCreatedV1 {
    pub amount_cents: u64,
}

pub struct ReceiptCreatedV2 {
    pub amount_cents: u64,
    pub currency: String,
}
```

It also generates the V1-to-V2 conversion (this is a simplified example):

```rust
fn convert_v1_to_v2(previous: ReceiptCreatedV1) -> ReceiptCreatedV2 {
    ReceiptCreatedV2 {
        amount_cents: previous.amount_cents,
        currency: "USD".to_owned(),
    }
}
```

The conversion preserves `amount_cents` and fills `currency` with the declared
backfill value. The alias now points to V2:

```rust
pub type ReceiptCreated = ReceiptCreatedV2;
```

V2 starts as a draft. You can revise it while V1's saved schema continues to
protect earlier receipts. Test the conversion before freezing V2:

```sh
cargo type-history freeze --package receipt-history
```

Freezing adds a `2` entry to `type-history/schemas.json` and leaves the `1` entry
unchanged. The `properties` in V2's schema describe both fields:

<!-- journey:v2-properties.json -->
```json
{
  "amount_cents": { "format": "uint64", "minimum": 0, "type": "integer" },
  "currency": { "type": "string" }
}
```

Both fields are listed in V2's `required` array. Commit the updated ledger with
the V2 declaration before introducing V3.

### 4. Change a field's type in V3

Refunds need negative amounts, so V3 changes `amount_cents` from `u64` to `i64`.
Keep the earlier type in the field's history and provide a conversion:

<!-- overview:v3.rs -->
```rust
use std::num::TryFromIntError;
use type_history::versioned;

#[versioned(stable_name = "shop.receipt.created")]
pub struct ReceiptCreated {
    #[history(updated_at = v3, previous_type = u64, cast_from_previous_value = signed_amount)]
    pub amount_cents: i64,

    #[history(added_in = v2, backfill_value = "USD".to_owned())]
    pub currency: String,
}

fn signed_amount(previous: u64) -> Result<i64, TryFromIntError> {
    i64::try_from(previous)
}
```

- `updated_at = v3` records which version changes the field's type.
- `previous_type = u64` preserves the field's type in V1 and V2.
- `cast_from_previous_value` uses `signed_amount` to convert the field from V2 to V3.
- `signed_amount` returns an error if the earlier amount exceeds the `i64` range.

Type History keeps V1 and V2 and generates `ReceiptCreatedV3`. This simplified
sketch shows the new struct and the V2-to-V3 conversion:

```rust
pub struct ReceiptCreatedV3 {
    pub amount_cents: i64,
    pub currency: String,
}

fn convert_v2_to_v3(previous: ReceiptCreatedV2) -> Result<ReceiptCreatedV3, TryFromIntError> {
    Ok(ReceiptCreatedV3 {
        amount_cents: signed_amount(previous.amount_cents)?,
        currency: previous.currency,
    })
}
```

The conversion changes the amount's type and carries over the currency. The
original name now points to V3:

```rust
pub type ReceiptCreated = ReceiptCreatedV3;
```

V3 is the new draft. The ledger still contains only the frozen V1 and V2 schemas.

### 5. Catch mistakes at compile time

Builds check the declaration against the ledger and type-check its conversion rules:

- **Missing history attributes:** Adding `currency` without its `#[history(...)]`
  attribute would change frozen V1's fields. The build rejects it.
- **Missing conversion rules:** An added field needs a backfill or conversion;
  a type change needs its previous type and a conversion.
- **Incorrect types:** Here, the backfill must produce a `String`, and
  `signed_amount` must accept `u64` and return `Result<i64, _>`. Rust checks these
  types while the version is a draft, too.
- **Unfinished versions:** Release builds reject drafts.

For example, changing the `currency` backfill to an integer fails compilation:

<!-- journey:wrong-backfill.rs -->
```rust
#[history(added_in = v2, backfill_value = 0_u32)]
pub currency: String,
```

Rust reports the type mismatch:

<!-- journey:backfill-error.txt -->
```text
expected `String`, found `u32`
```

The compiler cannot tell whether `"USD"` is the right currency for old receipts.
Test conversions with representative earlier data; step 6 checks that value.
Also check the conversion's limits. Here, `signed_amount` converts `1200`
successfully and rejects `u64::MAX`:

<!-- journey:conversion-tests.rs -->
```rust
assert_eq!(signed_amount(1200)?, 1200);
assert!(signed_amount(u64::MAX).is_err());
```

Once those tests pass, freeze V3 and build for release:

```sh
cargo type-history freeze --package receipt-history
cargo build --release --locked
```

The ledger now contains three frozen versions. V1 and V2 stay unchanged. The
`properties` in V3's schema record the signed amount and existing currency:

<!-- journey:v3-properties.json -->
```json
{
  "amount_cents": { "format": "int64", "type": "integer" },
  "currency": { "type": "string" }
}
```

Commit the updated ledger with the V3 declaration.

### 6. Read and write through the current type

Application code uses two types:

- `ReceiptCreated` is the current struct, now V3.
- `Versioned<ReceiptCreated>` can hold any supported version. It keeps the
  `stable_name`, version number, and matching payload together.

Suppose the application retrieves a receipt written as V1. Deserialize its bytes
into `Versioned<ReceiptCreated>`:

<!-- overview:deserialize.rs -->
```rust
use serde_json::from_slice;
use type_history::Versioned;

let bytes = br#"{
    "stable_name": "shop.receipt.created",
    "version": 1,
    "payload": { "amount_cents": 1200 }
}"#;
let stored: Versioned<ReceiptCreated> = from_slice(bytes)?;

assert_eq!(stored.source_version().get(), 1);
```

Deserializing the wrapper:

- Checks that the stored `stable_name` matches this history.
- Reads the payload as `ReceiptCreatedV1`, because the stored version is `1`.
- Keeps that V1 value without running any conversions.

The wrapper still holds V1. Pass it to `from_versioned` to obtain the current
`ReceiptCreated`:

<!-- overview:read-old.rs -->
```rust
let receipt: ReceiptCreated = ReceiptCreated::from_versioned(stored)?;

assert_eq!(receipt.amount_cents, 1200);
assert_eq!(receipt.currency, "USD");
```

`from_versioned` applies the generated conversions in order:

- **V1 to V2:** Preserve the amount and supply `"USD"` for `currency`.
- **V2 to V3:** Convert the amount from `u64` to `i64`.
- **Result:** Return the current `ReceiptCreated` for application code to use.

Reading V2 runs only the V2-to-V3 conversion. Reading V3 needs no conversion.
If a conversion fails, `from_versioned` returns an error with the original version
and the failing step.

To write a new receipt, construct the current type and wrap it before serialization:

<!-- overview:write-current.rs -->
```rust
use serde_json::to_vec;

let receipt = ReceiptCreated {
    amount_cents: -400,
    currency: "EUR".to_owned(),
};
let bytes = to_vec(&receipt.into_versioned())?;
```

- `into_versioned` supplies the stable name and current version automatically.
- `to_vec` serializes the wrapper and its payload together.

The resulting JSON contains V3's version number and the refund's fields
(formatted here for readability):

<!-- overview:written.json -->
```json
{
  "stable_name": "shop.receipt.created",
  "version": 3,
  "payload": {
    "amount_cents": -400,
    "currency": "EUR"
  }
}
```

The application chooses how to save and retrieve these bytes. This example uses
JSON; the same API works with other
[supported Serde formats][formats].
<!-- ANCHOR_END: journey -->

## Documentation

- [The book](https://github.com/sformisano/type-history/blob/main/book/README.md)
  covers the generated types, field changes, conversions, freezing, and integrations.
- [API reference](https://docs.rs/type-history/latest/type_history/)
  documents public types, methods, and traits.
- [The invoice example](https://github.com/sformisano/type-history/blob/main/examples/invoice/README.md)
  is a complete runnable package.

[formats]: https://github.com/sformisano/type-history/blob/main/book/src/decoding.md#choose-a-serde-format

Licensed under [MIT](https://github.com/sformisano/type-history/blob/main/LICENSE-MIT)
or [Apache 2.0](https://github.com/sformisano/type-history/blob/main/LICENSE-APACHE), at your option.
