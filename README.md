# Type History

Type History is a Rust library that lets you evolve structs while preserving
support for data written using earlier versions. It uses simple field attributes
to describe changes and compile-time checks to catch missing or incorrect
attributes before they cause runtime failures in production.

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

A shop initially accepts only US dollars. Its `ReceiptCreated` event starts with
an amount in cents. Later, the shop adds other currencies and supports refunds.

To run this example, first [set up a `receipt-history` library](docs/setup.md).

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
- `stable_name` names that history. Every receipt shares this name across all versions.
- Fields without `#[history(...)]` attributes belong to V1.

Type History generates `ReceiptCreatedV1` and makes the original name an alias:

```rust
pub type ReceiptCreated = ReceiptCreatedV1;
```

Application code uses `ReceiptCreated`. The alias always points to the latest
version, so callers do not need to choose a numbered type.

### 2. Freeze V1

V1 starts as a **draft**. You can revise its fields while developing and testing;
ordinary development builds allow drafts with a warning.

When V1 is ready, freeze it:

```sh
cargo type-history freeze --package receipt-history
```

Freezing records two things in `type-history/schemas.json`, a file called the **ledger**:

- The history's `stable_name`.
- V1's **schema**: its serialized field names, types, and nested structure.

Commit the ledger with your code. Later builds use it to check that earlier
versions remain intact. Changing the frozen `stable_name` also fails the build,
even if you rename the Rust type or move it to another module.

### 3. Add a field in V2

The shop now accepts other currencies. Add `currency` and describe how earlier
receipts should supply its value:

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

Type History retains V1 and generates V2. With trait implementations omitted,
the generated structs have these fields:

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

V2 is the new draft. Test its conversion, then freeze it before introducing V3:

```sh
cargo type-history freeze --package receipt-history
```

### 4. Change a field's type in V3

Refunds need negative amounts. Change `amount_cents` from `u64` to `i64`, keeping
its previous type and supplying a conversion:

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

- `updated_at = v3` records when the field changed type.
- `previous_type = u64` keeps the earlier versions' field type intact.
- `cast_from_previous_value` calls `signed_amount` when converting V2 to V3.
  An amount outside the `i64` range returns a conversion error.

Type History now generates `ReceiptCreatedV1`, `ReceiptCreatedV2`, and
`ReceiptCreatedV3`. The original name follows the latest version:

```rust
pub type ReceiptCreated = ReceiptCreatedV3;
```

### 5. Catch mistakes at compile time

The saved schemas and Rust's type checks protect different parts of this history:

- **Missing history attributes:** Adding `currency` without its `#[history(...)]`
  attribute would change frozen V1's fields. The build rejects it.
- **Missing conversion rules:** An added field needs a backfill or conversion;
  a type change needs its previous type and a conversion.
- **Incorrect types:** Here, the backfill must produce a `String`, and
  `signed_amount` must accept `u64` and return `Result<i64, _>`. Rust checks these
  types while the version is a draft, too.
- **Unfinished versions:** Release builds reject drafts.

Freeze V3 once its conversion has been tested:

```sh
cargo type-history freeze --package receipt-history
cargo build --release --locked
```

The compiler cannot tell whether `"USD"` is the right currency for old receipts.
Test conversions with representative earlier data; the next example checks that value.

### 6. Read and write through the current type

Application code uses two types:

- `ReceiptCreated` is the current struct, now V3.
- `Versioned<ReceiptCreated>` can hold any supported version. It keeps the
  `stable_name`, version number, and matching payload together.

First, deserialize an earlier receipt into the wrapper:

<!-- overview:deserialize.rs -->
```rust
use serde_json::from_slice;
use type_history::Versioned;

let stored: Versioned<ReceiptCreated> = from_slice(
    br#"{"stable_name":"shop.receipt.created","version":1,"payload":{"amount_cents":1200}}"#,
)?;
```

Deserializing the wrapper:

- Checks that the stored `stable_name` matches this history.
- Reads the payload as `ReceiptCreatedV1`, because the stored version is `1`.
- Keeps that V1 value without running any conversions.

Next, convert the wrapper into the current type:

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
- The wrapper keeps that metadata with the matching fields, so you do not assemble it yourself.
- The application chooses how to save and retrieve the bytes.

These examples use JSON. The same API works with other
[supported Serde formats](docs/decoding.md#choose-a-serde-format).

## Documentation

| Topic | Where to go next |
| --- | --- |
| Installation and build hook | [Set up your project](docs/setup.md) |
| Renames, removals, and other field changes | [Field history reference](docs/field-history.md) |
| Conversions that read several earlier fields | [Invoice walkthrough](docs/guide.md) |
| Formats, validation, and conversion errors | [Reading and writing versioned values](docs/decoding.md) |
| Freezing, checking, resetting, and importing histories | [History lifecycle](docs/lifecycle.md) |
| Modules, dependency aliases, and supported types | [Integration reference](docs/integration.md) |
| Optional redaction | [Redactable integration](docs/redactable.md) |

The [invoice example](examples/invoice/README.md) is a complete runnable package.
The [documentation index](docs/README.md) describes the crates and further references.

Licensed under [MIT](LICENSE-MIT) or [Apache 2.0](LICENSE-APACHE), at your option.
