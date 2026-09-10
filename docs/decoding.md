# Decode stored records and handle failures

The receipt snippets use the [V2 `ReceiptCreated` declaration](../README.md#3-add-a-field-in-v2), before the README adds V3.
Place snippets containing `?` in a function that returns a compatible `Result`.

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

The generated methods need no trait imports or manually assembled metadata.

The application chooses where to save the serialized bytes and how to retrieve them.
Type History does not call a database, filesystem, or transport.

## Choose a Serde format

`Versioned` uses a map with three named fields: `stable_name`, `version`, and `payload`.
The stable name is a string. The version is a positive `u32`, written as a number.
The payload contains the fields from that historical version.

The fields can arrive in any order, including `payload` first.
The decoder temporarily buffers the payload and rejects duplicate fields while reading it.
It then checks the buffered payload against the selected historical type.

Type History enables `serde_json`'s `arbitrary_precision` feature to retain
128-bit integers during buffering. This has a parsing limitation:
an object such as `{"$serde_json::private::Number":"42"}` can also be read as an
integer field. Serde exposes that object and a buffered JSON number through the
same representation.

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

The historical payload must match its declared version. For example, V1 has no
`currency` field, so this input is rejected:

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

A missing supported `Option` field may decode as `None`.
The selected Serde format returns these errors. With JSON, their type is `serde_json::Error`.
The fields present in a payload never determine its version.
All deserialization checks finish before `from_versioned` can run a conversion.

## Conversion failures

`ReceiptCreated::from_versioned` returns a `DecodeError` if an ordinary history's conversion fails.
This separate step preserves the callback's concrete error without reducing it to a Serde error message.

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

`Error::source()` preserves the concrete error returned by the conversion.

`DecodeError` implements `Debug`, `Display`, and `std::error::Error`.
It does not implement `Serialize`, `Clone`, or equality traits.
Histories that opt into Redactable use [separate error types](redactable.md).

## Existing raw JSON payloads

The low-level JSON APIs remain available for data whose metadata is already stored separately.
Pass the stable name, source version, and raw payload together:

<!-- decoding:raw-json.rs -->
```rust
use type_history::{decode, PayloadVersion, StableName};

let stable_name = StableName::try_from("shop.receipt.created".to_owned())?;
let version = PayloadVersion::try_from_raw(1)?;
let bytes = br#"{"amount_cents":1200}"#;
let receipt = decode::<ReceiptCreated>(&stable_name, version, bytes)?;

assert_eq!(receipt.currency, "USD");
```

The two constructors validate metadata supplied at runtime.

This API checks the stable name and version before parsing the exact historical payload.
It then applies adjacent conversions. `ReadError` distinguishes a stable name mismatch from `DecodeError`.
The source chain preserves the original JSON or callback error when one exists.
`Invoice::history().decode(...)` skips the stable name check; `history()` requires the `HasHistory` trait in scope.

Raw JSON has no versioned wrapper, so deserializing it as `Versioned<ReceiptCreated>` fails.
After loading it through `decode`, `into_versioned` prepares the current value for
serialization with its stable name and version.
Any decision to replace stored data remains with the application.
