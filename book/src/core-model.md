# Current types and historical data

If `ReceiptCreated` is only an alias of the newest generated struct, how can
`ReceiptCreated::from_versioned` read V1 data?

The latest struct has a generated implementation that knows the whole supported
history. `Versioned<ReceiptCreated>` uses that implementation to hold an earlier
payload. Its generic parameter selects the history, not the payload's version.

## One current type, several possible payloads

Consider the receipt after V2 introduces `currency`. The following is a
**conceptual generated-code sketch**. The enum name illustrates the design;
it is not a public API to import or match on.

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

- `ReceiptCreated` is an ordinary current value. It always has `currency` at V2.
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

You do not deserialize the old payload directly into the latest struct. The
wrapper first reads its matching historical type, then the generated conversions
make it current.

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

An adjacent conversion can fail. For example, an old `u64` amount may exceed
`i64::MAX`. The method returns an error identifying the original stored version
and the failing step. Compile-time checks verify the conversion's types; tests
and runtime error handling address its values.

## The three names in the example

| Name | Meaning |
| --- | --- |
| `ReceiptCreated` | Rust alias for the latest generated struct |
| `shop.receipt.created` | Frozen stable name shared by all values and versions of this history |
| `version: 1` | Version of this particular stored payload |

Renaming the Rust declaration does not require changing stored data. Changing a
frozen stable name fails the build. The [lifecycle chapter](lifecycle.md) explains
how the ledger protects that name and each frozen schema.
