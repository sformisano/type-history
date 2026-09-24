# Decode stored records and handle failures

The shop needs to write new receipts and read receipts already in storage. Both operations use `ReceiptCreated`, the alias for the current type. `Versioned<ReceiptCreated>` keeps each receipt's data together with the information needed to decode it later.

These examples use the shop's [V2 declaration](quick-start.md#3-add-a-field-in-v2), before the walkthrough adds V3. Place snippets containing `?` in a function that returns a compatible `Result`.

Start with a new receipt for 3000 cents in EUR:

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

`into_versioned` supplies `shop.receipt.created` and version 2. Serde writes those values with the receipt's fields. After the application retrieves the bytes, `from_versioned` returns the current `ReceiptCreated`. Here the stored receipt is already V2, so no field conversion runs.

The generated methods need no trait imports or manually assembled metadata. Your application chooses where to save and retrieve the bytes. Type History does not call a database, filesystem, or transport.

## Choose a Serde format

`Versioned` uses a map with three named fields: `stable_name`, `version`, and `payload`.
The stable name is a string. The version is a positive `u32`, written as a number.
The payload contains the fields from that historical version.

For example, a V1 receipt's payload has `amount_cents` but no `currency`. The version number tells Type History to read that payload as V1. The fields can arrive in any order, including `payload` first. The decoder temporarily holds the payload until it can select the historical type, while retaining enough information to reject duplicate fields.

JSON supports this representation. MessagePack supports it with
[`to_vec_named`](https://docs.rs/rmp-serde/1.3.1/rmp_serde/encode/fn.to_vec_named.html),
which serializes structs with their field names:

<!-- decoding:messagepack.rs -->
```rust
use rmp_serde::{from_slice, to_vec_named};

let bytes = to_vec_named(&receipt.into_versioned())?;
let receipt = ReceiptCreated::from_versioned(from_slice(&bytes)?)?;
```

This fragment assumes a `ReceiptCreated` event named `receipt`, an `rmp-serde`
dependency, and a function that propagates errors.
Use `serde_json::to_vec` and `serde_json::from_slice` for the equivalent JSON path.

Use `to_vec_named` for MessagePack because the decoder needs field names. MessagePack's default struct-array encoding omits them and is rejected. Any other Serde format must support both named record maps and the field types in the history.

Type History enables `serde_json`'s `arbitrary_precision` feature so buffering retains 128-bit integers. That feature has a parsing limitation: `{"$serde_json::private::Number":"42"}` can also be read as an integer field. Serde exposes that object and a buffered JSON number through the same representation.

## Inspect the stored version before converting

Deserializing selects the historical type but does not run conversions.
`from_versioned` applies the declared adjacent conversions and returns the current alias.
You can inspect the source version before choosing to convert:

<!-- decoding:source-version.rs -->
```rust
use serde_json::from_slice;
use type_history::Versioned;

let stored: Versioned<ReceiptCreated> = from_slice(
    br#"{"stable_name":"shop.receipt.created","version":1,"payload":{"amount_cents":1200}}"#,
)?;
assert_eq!(stored.stable_name().as_str(), "shop.receipt.created");
assert_eq!(stored.source_version().get(), 1);

let receipt = ReceiptCreated::from_versioned(stored)?;
let current = receipt.into_versioned();
assert_eq!(current.source_version().get(), 2);
```

Serializing `stored` before conversion would preserve V1. Serializing `current`
writes V2. Neither operation changes bytes already held in storage.

## Invalid records

The stored version selects the decoder; the payload must match that version. Adding `currency` to a payload still labeled V1 does not make it V2. This input is rejected:

<!-- decoding:invalid.rs -->
```rust
use serde_json::from_slice;
use type_history::Versioned;

let invalid = br#"{"stable_name":"shop.receipt.created","version":1,"payload":{"amount_cents":1200,"currency":"USD"}}"#;
assert!(from_slice::<Versioned<ReceiptCreated>>(invalid).is_err());
```

Deserializing a `Versioned` fails for:

- A different stable name or an unsupported version.
- A missing, duplicate, or unknown wrapper field.
- A version that is zero, negative, larger than `u32::MAX`, or written as a string or fraction.
- Malformed input or a sequence in place of a record map.
- Unknown fields, duplicate fields, or missing required fields.

A missing supported `Option` field may decode as `None`. A required field cannot be omitted. The selected Serde format returns decoding errors; with JSON, their type is `serde_json::Error`.

All these checks finish before a conversion can run. A newer payload is also rejected when its version is absent from the consumer's crate. See [distributed service upgrades](quick-start.md#using-type-history-in-a-distributed-system) for the resulting deployment order.

## Conversion failures

Decoding can succeed while conversion fails. The bytes may be a valid V1 record, but a later backfill may reject its value. `from_versioned` returns a `DecodeError` containing the original conversion error.

The [V3 invoice](guide.md#v3-keep-both-adjacent-conversions) rejects a count of 13 during its V2-to-V3 conversion.
Using that declaration and its `ConvertError`, read an invoice from V1 to inspect the failure:

<!-- decoding:conversion-error.rs -->
```rust
use serde_json::from_slice;
use std::error::Error;

let stored = from_slice(
    br#"{"stable_name":"billing.invoice.issued","version":1,"payload":{"legacy":"INV-13","count":13}}"#,
)?;
let error = Invoice::from_versioned(stored).unwrap_err();

assert_eq!(error.source_version().get(), 1);
assert_eq!(error.adjacent_from().unwrap().get(), 2);
assert_eq!(error.adjacent_to().unwrap().get(), 3);
let cause = error.source().unwrap().downcast_ref::<ConvertError>().unwrap();
assert_eq!(cause.0, 13);
```

The invoice was stored as V1, but it failed while moving from V2 to V3. The error preserves both facts. `Error::source()` also lets the application inspect the original `ConvertError(13)` returned by the callback.

`DecodeError` implements `Debug`, `Display`, and `std::error::Error`.
It does not implement `Serialize`, `Clone`, or equality traits.

## Existing raw JSON payloads

An existing database may store the stable name and version in separate columns, leaving only the receipt's fields in its JSON column. Use `decode` for that layout. Pass the metadata with the raw payload:

<!-- decoding:raw-json.rs -->
```rust
use type_history::{decode, PayloadVersion, StableName};

let stable_name = StableName::try_from("shop.receipt.created".to_owned())?;
let version = PayloadVersion::try_from_raw(1)?;
let bytes = br#"{"amount_cents":1200}"#;
let receipt = decode::<ReceiptCreated>(&stable_name, version, bytes)?;

assert_eq!(receipt.currency, "USD");
```

`StableName::try_from` checks the name's syntax. `PayloadVersion::try_from_raw` rejects zero; the decoder then checks whether the requested history supports that version.

This API checks the stable name and version before parsing the exact historical payload.
It then applies adjacent conversions. `ReadError` distinguishes a stable name mismatch from `DecodeError`.
The source chain preserves the original JSON or callback error when one exists.
`ReceiptCreated::history().decode(...)` skips the stable name check; `history()` requires the `HasHistory` trait in scope.

Both decoding APIs buffer payload values and require named fields for nested
records, too. JSON syntax errors retain their input position. A field-type error
found when reading the buffered values may have no line or column.

Raw JSON has no versioned wrapper, so deserializing it as `Versioned<ReceiptCreated>` fails.
After loading it through `decode`, `into_versioned` prepares the current value for
serialization with its stable name and version.
Any decision to replace stored data remains with the application.
