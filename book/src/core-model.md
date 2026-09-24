# Current types and historical data

After the shop adds `currency`, application code uses `ReceiptCreatedV2` through the `ReceiptCreated` alias. Stored V1 receipts still contain only `amount_cents`.

`Versioned<ReceiptCreated>` connects those two forms. It holds the data from one supported version until you call `ReceiptCreated::from_versioned` to convert it. The `ReceiptCreated` parameter identifies the history; the stored version identifies which type the wrapper actually contains.

## One current type, several possible payloads

Type History generates a struct for each version and connects them to a wrapper that can hold either one. This simplified sketch shows their relationship. `HistoricalReceiptCreated` illustrates the internal enum; application code uses `Versioned<ReceiptCreated>` instead.

```rust,ignore
struct ReceiptCreatedV1 {
    amount_cents: u64,
}

struct ReceiptCreatedV2 {
    amount_cents: u64,
    currency: String,
}

type ReceiptCreated = ReceiptCreatedV2;

enum HistoricalReceiptCreated {
    V1(ReceiptCreatedV1),
    V2(ReceiptCreatedV2),
}
```

These types serve different purposes:

- `ReceiptCreated` is the ordinary current type. It always has `currency` at V2.
- `Versioned<ReceiptCreated>` holds one historical payload, conceptually one
  variant of the enum above. It may contain V1 or V2.
- The stored version tells the deserializer which payload type to read.

The generated history implementation belongs to `ReceiptCreatedV2`. Because
`ReceiptCreated` aliases that struct, the same implementation is available through
the alias. It supplies the supported versions, their deserializers, and the
conversions to the current type.

## Reading and conversion are separate steps

Suppose the stored value contains:

```json
{"stable_name":"shop.receipt.created","version":1,"payload":{"amount_cents":1200}}
```

1. Deserializing `Versioned<ReceiptCreated>` checks the stable name and version.
2. Version `1` selects `ReceiptCreatedV1`, whose fields match the payload.
3. The wrapper retains that V1 value. At this point it has no `currency` field.
4. `ReceiptCreated::from_versioned` consumes the wrapper and runs the V1-to-V2
   conversion. The result has `currency: "USD"` and is an ordinary `ReceiptCreated`.

This order keeps the old receipt's structure intact while decoding. V1 is read using its own fields. The backfill supplies `currency` only when the decoded value is converted to V2.

## What changes when V3 arrives

In the [quick start](quick-start.md#4-change-a-fields-type-in-v3), V3 changes
`amount_cents` from `u64` to `i64`. Type History retains V1 and V2, adds V3 to the
historical payloads, and moves the alias to `ReceiptCreatedV3`.

The same public call now follows the path selected by the stored version:

```text
Stored V1: V1 -> V2 -> V3
Stored V2:       V2 -> V3
Stored V3:             V3
```

- V1 receives the USD backfill, then converts its amount to `i64`.
- V2 keeps its stored currency and converts only its amount.
- V3 is returned without a conversion.

Each step converts one version into the next. A step can fail: an old `u64` amount may exceed `i64::MAX`. In that case, `from_versioned` returns an error with the original stored version and the failing step. Test those limits and handle the error when reading data.

The generated conversions run from older versions toward the current type. A service whose crate contains only V1 cannot read V2. The [distributed example](quick-start.md#using-type-history-in-a-distributed-system) explains how to update services in an order that respects this limit.

## The three names in the example

| Name | Meaning |
| --- | --- |
| `ReceiptCreated` | Rust alias for the latest generated struct |
| `shop.receipt.created` | Frozen stable name shared by all values and versions of this history |
| `version: 1` | Version of this particular stored payload |

Renaming the Rust declaration does not require changing stored data. Changing a
frozen stable name fails the build. The [lifecycle chapter](lifecycle.md) explains
how the ledger protects that name and each frozen schema.
