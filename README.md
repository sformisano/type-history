# Type History

Type History is a Rust library that lets you evolve types while preserving support for data written using earlier versions. It uses field attributes to describe changes, and compile-time checks to catch missing or incorrect attributes before they cause runtime failures in production.

## Use cases

Persisted data often outlives the code that wrote it. As a production application evolves, it may accumulate years of records written using different versions of its Rust types. Type History converts those records into the current types, so application code can use them without handling each historical version separately.

Common examples include:

- **Stored events:** Read events written before fields were added, changed, or removed.
- **Saved snapshots:** Restore records written by an earlier application version.
- **Versioned documents:** Read records with different stored versions.

## Set up your project

Type History works through three components:

- `type-history` is the library your application uses to describe how types change. It generates the code needed to read previous versions.
- `cargo-type-history` is a Cargo command. It creates the package's schema file, `type-history/schemas.json`, and saves each new version's schema there.
- `type-history-build` runs during Cargo builds. It checks that each version in the schema file still matches its Rust definition.

### Install the components

Type History requires Rust 1.97 or later. It supports Linux and macOS. Windows is not supported.

Install the 0.3.1 CLI from crates.io:

```sh
cargo install cargo-type-history --version '=0.3.1' --locked
```

Add the library and build hook to your package:

```toml
[dependencies]
type-history = "=0.3.1"

[build-dependencies]
type-history-build = "=0.3.1"
```

Keep the library, build hook, and CLI on the same release.
The [package setup guide](https://github.com/sformisano/type-history/blob/v0.3.1/book/src/setup.md)
shows the complete configuration and a source checkout alternative.

### Connect Type History to Cargo

Create `build.rs` next to your package's `Cargo.toml`. If the package already has one, add the `type_history_build::compile()` call to its `main` function:

```rust
fn main() {
    type_history_build::compile();
}
```

Cargo runs `build.rs` as part of building the package. Declare your versioned types in the package's library target, `src/lib.rs` by default.

### Initialize Type History

The `cargo type-history` commands run Cargo with `--locked --offline`, so fetch the new dependencies first. Then run these commands inside your project:

```sh
cargo fetch
cargo type-history init --package my-shop-demo-project
```

Replace `my-shop-demo-project`, here and in later commands, with the `name` in your `[package]` table. The command creates `type-history/schemas.json`, which starts as an empty JSON object (`{}`).

## How to use

This example follows a shop's `ReceiptCreated` event type as its fields change. The shop initially accepts only US dollars, so the event starts with an amount in cents. Later, the shop adds other currencies and supports refunds.

The example uses a monolith: the application and its types are built and deployed together. The [distributed systems section](#using-type-history-in-a-distributed-system) covers services that upgrade separately. Step 6 uses `serde_json`. Add it to your dependencies to run those snippets.

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

- `#[versioned(...)]` creates the type's history: its numbered versions, starting with V1.
- `stable_name` names that history. Every `ReceiptCreated` version shares this name.
- Fields without `#[history(...)]` exist from V1.

Type History generates `ReceiptCreatedV1` and makes the original name an alias:

```rust
pub type ReceiptCreated = ReceiptCreatedV1;
```

**Note:** Application code uses the alias (`ReceiptCreated` here) for current values. It always refers to the latest version. If a field change breaks application code, the compiler identifies the places that need updating.

### 2. Generate and freeze V1's schema

When Cargo builds your package, it runs `build.rs`. The `type_history_build::compile()` call finds the `ReceiptCreated` declaration, and Type History generates V1's **schema**: a description of its field names, field types, and nested fields. For V1, it describes the `amount_cents` field and its `u64` type.

Type History then compares the schemas generated from the versioned types with any schemas saved in `type-history/schemas.json`. This file is the **ledger**. A version whose schema is saved there is **frozen**. Future builds check that a frozen version's schema has not changed.

Our `schemas.json` ledger is still empty, so our `ReceiptCreatedV1` is a **draft**: a version declared in Rust whose schema has not yet been saved. A type that is not yet in `schemas.json` must start at V1. It cannot start as `ReceiptCreatedV2` or later.

You *can* change a draft while developing and testing. Development builds allow drafts with a warning. Release builds require every version to be frozen.

WARNING: Editing a draft can make records written with it unreadable. Freeze a version before storing data you must keep.

When V1 is ready, freeze it:

```sh
cargo type-history freeze --package my-shop-demo-project
```

The `freeze` command:

- Checks that type changes have the required history attributes and that the conversion code compiles.
- Checks frozen versions against their saved schemas, rejecting changes to their fields or field types.
- Saves the schemas for the package's drafts to `type-history/schemas.json` only after those checks pass.

It does *not* create versions or modify Rust code.

In our example, `ReceiptCreatedV1` is the only draft. Freezing it adds the following entry to the ledger, with its schema in JSON Schema format:

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

Reading the entry from the outside in:

- `shop.receipt.created` is the type's stable name.
- `1` identifies V1.
- `properties` describes `amount_cents` as an integer with the `uint64` format and a minimum of zero.
- `required` lists `amount_cents` as a required field.

`ReceiptCreatedV1` is now frozen. If a later build generates a different schema for V1, the build fails. Changing a frozen version does not make it a draft.

The stable name must also stay the same, or the build fails. You may rename `ReceiptCreated` or move it to another module, but keep `shop.receipt.created` so Type History can find its saved history.

Commit the ledger with the Rust code.

### 3. Add a field in V2

The shop now accepts other currencies, so `ReceiptCreated` needs a `currency` field.

This field did not exist in V1. Use `#[history(...)]` to declare when it was added and what value to supply when reading V1 data:

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

- `added_in = v2` adds `currency` starting with V2.
- `backfill_value` supplies `"USD"` when converting V1 data to V2. The shop accepted only US dollars when that data was written.
- New V2 values supply their own currency.

The declaration now describes two versions. Type History keeps V1 unchanged and generates V2 with the new field. The generated structs look like this, without their trait implementations:

```rust
pub struct ReceiptCreatedV1 {
    pub amount_cents: u64,
}

pub struct ReceiptCreatedV2 {
    pub amount_cents: u64,
    pub currency: String,
}
```

It also generates the V1-to-V2 conversion. This simplified version shows how the fields are converted conceptually:

```rust
fn convert_v1_to_v2(previous: ReceiptCreatedV1) -> ReceiptCreatedV2 {
    ReceiptCreatedV2 {
        amount_cents: previous.amount_cents,
        currency: "USD".to_owned(),
    }
}
```

The conversion keeps `amount_cents` and supplies the declared value for `currency`. The alias now points to V2:

```rust
pub type ReceiptCreated = ReceiptCreatedV2;
```

Application code using `ReceiptCreated` now uses the definition with both fields. Existing struct initializers must supply `currency`. The compiler reports each one that needs updating.

V1 remains frozen, and its generated schema still matches its ledger entry. V2 has no saved schema, so it is the new draft. After a version is frozen, only its direct successor can be a draft.

Test the conversion, then freeze V2:

```sh
cargo type-history freeze --package my-shop-demo-project
```

This time, `ReceiptCreatedV2` is the package's only draft. Freezing adds its `2` entry to the ledger. The `1` entry stays unchanged.

The ledger now contains both versions. V2's schema includes `currency` and requires both fields:

<!-- journey:ledger-v2.json -->
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
    },
    "2": {
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
          },
          "currency": {
            "type": "string"
          }
        },
        "required": ["amount_cents", "currency"],
        "type": "object"
      }
    }
  }
}
```

Commit the updated ledger with the V2 declaration.

The history has now passed through these states:

| Latest version declared in Rust | Schemas saved in the ledger | Result |
| --- | --- | --- |
| V1 | None | V1 is a draft. |
| V1 | Matching V1 | V1 is frozen. |
| V2 | Matching V1 | V1 is frozen. V2 is a draft. |
| V2 | Matching V1 and V2 | V1 and V2 are frozen. |

### 4. Change a field's type in V3

V2 stores `amount_cents` as a `u64`, which cannot hold a negative value. The shop now supports refunds, so V3 needs an `i64`.

Use `#[history(...)]` to record when the type changed, preserve its earlier type, and provide the conversion:

<!-- overview:v3.rs -->
```rust
use std::num::TryFromIntError;
use type_history::versioned;

#[versioned(stable_name = "shop.receipt.created")]
pub struct ReceiptCreated {
    #[history(updated_in = v3, previous_type = u64, backfill_fn = signed_amount)]
    pub amount_cents: i64,

    #[history(added_in = v2, backfill_value = "USD".to_owned())]
    pub currency: String,
}

fn signed_amount(previous: &ReceiptCreatedV2) -> Result<i64, TryFromIntError> {
    i64::try_from(previous.amount_cents)
}
```

- `updated_in = v3` changes the field's type starting with V3.
- `previous_type = u64` keeps `amount_cents` as a `u64` in V1 and V2.
- `backfill_fn` uses `signed_amount` to calculate the field's value when converting V2 to V3.
- `signed_amount` returns an error when the old amount is too large for `i64`.
- `signed_amount` names the previous version, `ReceiptCreatedV2`, because `ReceiptCreated` now refers to V3.

Type History keeps V1 and V2 unchanged and generates `ReceiptCreatedV3`. This simplified version shows the new struct and the V2-to-V3 conversion:

```rust
pub struct ReceiptCreatedV3 {
    pub amount_cents: i64,
    pub currency: String,
}

fn convert_v2_to_v3(previous: ReceiptCreatedV2) -> Result<ReceiptCreatedV3, TryFromIntError> {
    Ok(ReceiptCreatedV3 {
        amount_cents: signed_amount(&previous)?,
        currency: previous.currency,
    })
}
```

The conversion changes the amount's type and keeps the currency. The alias now points to V3:

```rust
pub type ReceiptCreated = ReceiptCreatedV3;
```

V3 is the new draft. The ledger still contains only frozen V1 and V2 entries.

### 5. Check and freeze V3

Before freezing V3, build and test it. The build checks that frozen schemas remain unchanged and that the conversion rules have the required attributes and types. Your tests must check that the conversions produce the intended values.

The build catches mistakes such as:

- **Missing history attributes:** Adding `currency` without its `#[history(...)]` attribute would change frozen V1's fields. The build rejects that change.
- **Missing backfill rules:** Additions and updates require exactly one of `backfill_value` or `backfill_fn`. Updates also require `previous_type`.
- **Incorrect types:** The `currency` backfill must produce a `String`. `signed_amount` must accept `&ReceiptCreatedV2` and return `Result<i64, _>`. Rust checks these types for drafts too.

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

The compiler can verify that the backfill is a `String`. It cannot decide whether `"USD"` is the right currency for V1 data. Test conversions with data that represents what your application has stored.

Also test conversion limits. Here, `signed_amount` converts `1200` and rejects `u64::MAX`:

<!-- journey:conversion-tests.rs -->
```rust
let mut previous = ReceiptCreatedV2 {
    amount_cents: 1200,
    currency: "USD".to_owned(),
};

assert_eq!(signed_amount(&previous)?, 1200);

previous.amount_cents = u64::MAX;
assert!(signed_amount(&previous).is_err());
```

After the tests pass, freeze V3 and build for release:

```sh
cargo type-history freeze --package my-shop-demo-project
cargo build --release --locked
```

`ReceiptCreatedV3` was the only draft. Freezing adds its `3` entry to the ledger. The V1 and V2 entries stay unchanged.

The ledger now contains all three versions. V3 uses `int64` for `amount_cents`, allowing negative amounts:

<!-- journey:ledger-v3.json -->
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
    },
    "2": {
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
          },
          "currency": {
            "type": "string"
          }
        },
        "required": ["amount_cents", "currency"],
        "type": "object"
      }
    },
    "3": {
      "metadata": {},
      "schema": {
        "$id": "urn:typehistory:schema:shop.receipt.created",
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "additionalProperties": false,
        "properties": {
          "amount_cents": {
            "format": "int64",
            "type": "integer"
          },
          "currency": {
            "type": "string"
          }
        },
        "required": ["amount_cents", "currency"],
        "type": "object"
      }
    }
  }
}
```

Commit the updated ledger with the V3 declaration.

### 6. Read and write through the current type

Use `Versioned<ReceiptCreated>` to read stored data. This wrapper can hold any supported version of `ReceiptCreated`, keeping the value together with its stable name and version number.

For example, a stored V1 value looks like this in JSON:

<!-- overview:stored-v1.json -->
```json
{
  "stable_name": "shop.receipt.created",
  "version": 1,
  "payload": {
    "amount_cents": 1200
  }
}
```

- `stable_name` identifies the type's history.
- `version` identifies which version was stored.
- `payload` contains that version's fields.

Deserialize the stored bytes into `Versioned<ReceiptCreated>`:

<!-- overview:deserialize.rs -->
```rust
use serde_json::from_slice;
use type_history::Versioned;

let bytes = br#"{"stable_name":"shop.receipt.created","version":1,"payload":{"amount_cents":1200}}"#;
let stored: Versioned<ReceiptCreated> = from_slice(bytes)?;

assert_eq!(stored.source_version().get(), 1);
```

Type History checks that `stable_name` matches `shop.receipt.created` and uses `version` to select the type for the payload. Here, it reads the payload as `ReceiptCreatedV1`.

Deserialization does not run conversions. The wrapper still holds V1 until you convert it into the current type:

<!-- overview:read-old.rs -->
```rust
let receipt: ReceiptCreated = ReceiptCreated::from_versioned(stored)?;

assert_eq!(receipt.amount_cents, 1200);
assert_eq!(receipt.currency, "USD");
```

`from_versioned` applies the generated conversions in order:

- **V1 to V2:** Keeps `amount_cents` and supplies `"USD"` for `currency`.
- **V2 to V3:** Converts `amount_cents` from `u64` to `i64`.

The result is a `ReceiptCreated` value using the current definition, V3. Reading V2 runs only the V2-to-V3 conversion. Reading V3 needs no conversion.

If a conversion fails, `from_versioned` returns an error identifying the original stored version and the conversion step that failed.

To write new data, construct `ReceiptCreated`, wrap it with `into_versioned`, and serialize it:

<!-- overview:write-current.rs -->
```rust
use serde_json::to_vec;

let receipt = ReceiptCreated {
    amount_cents: -400,
    currency: "EUR".to_owned(),
};

let bytes = to_vec(&receipt.into_versioned())?;
```

`into_versioned` supplies the stable name and current version number. `to_vec` serializes the wrapper, producing these JSON bytes, formatted here for readability:

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

Your application chooses how to save and retrieve the bytes. This example uses JSON; the same API works with other [supported Serde formats][formats].

## Using Type History in a distributed system

Until now, our shop has been a monolith: the application and its types are built and deployed together. Application code creating or accepting `ReceiptCreated` uses the same current definition. If a field change breaks that code, the compiler identifies the places that need updating. Only stored data can still use an older version, which Type History converts when the application reads it.

Now suppose the shop has separate checkout and reporting services. Checkout produces `ReceiptCreated` events, and reporting consumes them. Both depend on a shared crate defining `ReceiptCreated`, but each service is built and deployed separately. A new crate release reaches each service only when you update its dependency, rebuild, and deploy it.

Return to the currency change in V2. If checkout starts sending V2 receipts while reporting still uses V1, reporting rejects those messages. Its crate has no definition for V2, even if reporting does not need the `currency` field. Type History cannot convert V2 receipts back to V1. The V1-to-V2 backfill lets an updated reporting service read older receipts.

### Upgrade consumers before producers

To deploy that change, update the services in this order:

1. **Prepare the shared crate.** Add V2 as shown above. Test the backfill, then freeze V2 before releasing the crate.
2. **Update reporting first.** Rebuild and deploy it with the new crate release. Keep checkout on the old release, producing V1 receipts. Reporting now reads those receipts as V2, with `currency` set to `"USD"`.
3. **Update checkout.** Once every service receiving these receipts supports V2, update checkout to produce V2. Reporting can read both new V2 receipts and V1 receipts still in storage or queues.

Before checkout sends amounts in other currencies, test that reporting handles those receipts correctly. Rust checks the field types; your tests must check how reporting uses their values. Backfills can also return errors, as the V3 amount conversion showed. Each service must handle those errors when reading old messages.

If reporting cannot upgrade yet, checkout must keep producing V1 for it. The teams running these services must agree when checkout can switch to V2.

A service may both read and create receipts. With V2 as its current type, `into_versioned` writes V2 too. Upgrading its crate switches its reads and writes together. Coordinate that switch with the other services, or use separate events.

After checkout has written V2, rolling it back to V1 leaves any V2 messages already in storage or queues. Services reading those messages still need V2 support.

### Use a separate event for refunds

V3 changes the type of `amount_cents` for every receipt. Once checkout writes V3, every service reading receipts needs V3 support, including services that ignore refunds. For refunds, the shop could instead keep its existing `ReceiptCreated` definition and add a separate `RefundCreated` type. Give the new type its own `stable_name` and send it through a separate topic or queue.

Services using the old crate continue receiving the same receipt events as before. Services handling refunds update their shared crate dependency and subscribe to the new topic when they are ready. The application configures those subscriptions; Type History does not route messages.

If reporting calculates sales totals after subtracting refunds, it must support refunds before the shop enables them. Agree on that timing with the teams running the affected services.

<!-- ANCHOR_END: journey -->

[formats]: https://github.com/sformisano/type-history/blob/v0.3.1/book/src/decoding.md#choose-a-serde-format

## Persisted field contracts

Histories remain named-field structs. Their fields can use supporting records,
enums, nonempty tuple structs, and these field families:

- string-keyed `HashMap` and `BTreeMap`
- declared-membership `HashSet` and `BTreeSet`
- bare tuples with 1 through 16 elements
- `Box`, plus `Rc` and `Arc` with the `rc` feature
- checked adapters behind `typed-floats`, `uuid`, `rust-decimal`, `chrono`, and `time`

Type History compares the complete storage contract. Float width, logical
profile, set membership, field presence, tuple order, and tuple arity all
participate. Change any of them in a new version with an explicit conversion.

The checked adapters own their JSON and named-field MessagePack encodings.
Enabling native dependency Serde features does not change those encodings.
See [field integration](https://github.com/sformisano/type-history/blob/v0.3.1/book/src/integration.md#supported-field-contracts)
for exact domains, feature flags, set declarations, and codec limits.

## Prepare a source checkout for tests

The standalone tests compile an AArch64 consumer without linking or running it.
Install that target and fetch the locked dependencies before running the tests:

```sh
rustup target add --toolchain 1.97.1 aarch64-unknown-linux-gnu
cargo fetch --locked
```

The standalone tests build temporary consumer packages.
While their fixtures overlap, tests share one copy of the Type History crates, one CLI build, and one frozen V1 setup.
Each test keeps its own files and CLI copy.
A test that reuses the frozen V1 setup still runs its own `cargo build`.
All tests build in the repository's `target` directory.
The last test still using the shared resources removes them, so single-threaded and filtered runs can reuse less.
The `cargo type-history` commands in these tests reuse compiled dependencies, as [package setup](https://github.com/sformisano/type-history/blob/v0.3.1/book/src/setup.md#3-initialize-the-schema-file) describes.

## Documentation

- [The book](https://github.com/sformisano/type-history/blob/v0.3.1/book/README.md) covers the generated types, field changes, conversions, freezing, and integrations.
- [The 0.3.1 API reference](https://docs.rs/type-history/0.3.1/type_history/) includes all optional field integrations. Build it locally with `cargo doc --workspace --no-deps --all-features --locked`, then open `target/doc/type_history/index.html`.
- [The invoice example](https://github.com/sformisano/type-history/blob/v0.3.1/crates/examples/invoice-history/README.md) is a complete runnable package.

Licensed under [MIT](https://github.com/sformisano/type-history/blob/v0.3.1/LICENSE-MIT) or [Apache 2.0](https://github.com/sformisano/type-history/blob/v0.3.1/LICENSE-APACHE), at your option.
