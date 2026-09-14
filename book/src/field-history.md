# Field history reference

The shop added `currency` with a backfill of `"USD"`, then changed `amount_cents` from `u64` to `i64`. Both changes belong in `#[history(...)]` attributes because older receipts still need their original fields and types.

This reference explains the attributes used in the [receipt walkthrough](quick-start.md), then applies them to renames and other field changes:

- **Additions:** An attribute records which version introduced a field and what
  backfill value to use when reading older records.
- **Type changes:** An attribute records which version changed a field’s type and
  specifies a conversion from the old type to the new type.
- **Renames:** You mark the old field as removed and add the new field in the same
  version. A conversion reads the whole previous record and returns the value for
  the new field.
- **Removals:** The field stays in the history declaration, with an attribute
  recording which version removed it. Earlier versions retain it so older data
  remains readable. The current type omits it, so consumer code that still accesses
  it fails to compile.

Field attributes answer two questions: what did this version look like, and how does an older value become this version? `added_in`, `updated_in`, `removed_in`, and `previous_type` describe the historical structure. `backfill_value` and `backfill_fn` supply values during conversion.

Write versions as `v1`, `v2`, and so on. Version zero and values that overflow the version number are rejected.

Every history starts at V1. A field without history attributes exists from V1.
A **boundary** is a version where a field is added, changed, or removed.
The latest boundary determines the current version; without boundaries, it is V1.

| Attribute key | Where it goes and what it declares |
| --- | --- |
| `stable_name = "billing.invoice.issued"` | On the struct; names the type and its history independently of its Rust name |
| `added_in = vN` | Field first exists in N; provide `backfill_value` or `backfill_fn` |
| `updated_in = vN` | Field changes in N; provide `previous_type` and one backfill rule |
| `removed_in = vN` | Field disappears in N; keep its source declaration for earlier versions |
| `previous_type = Old` | Required on an update; the field's type in the immediately previous version |
| `backfill_value = expression` | Addition or update; directly supplies the destination field's value |
| `backfill_fn = path` | Addition or update; a synchronous function borrows the complete previous record |

## Backfill values and functions

V1 receipts have no `currency`, so reading them as V2 requires a value for that new field. The shop supplies `"USD"` through `backfill_value`. A `backfill_fn` is useful when the value depends on the previous record or the conversion can fail.

Every addition or update requires exactly one of these two rules:

| Change | Required rule | Conversion signature |
| --- | --- | --- |
| Addition | One backfill value or one record callback | `fn(&InvoiceVn) -> Result<AddedType, E>` for a callback |
| Update | One backfill value or one record callback, plus `previous_type` | `fn(&InvoiceVn) -> Result<Destination, E>` for a callback |
| Removal | None | None; record callbacks can still read the removed field from the previous version |

`E` must implement `std::error::Error + Send + Sync + 'static`.
Return `Ok(value)` when a conversion cannot fail.

The `ReceiptCreated` event examples on this page are separate changes to the [V2 declaration](quick-start.md#3-add-a-field-in-v2).
Each demonstrates a possible V3. The invoice fragments use the [invoice guide](guide.md).

An optional field still needs an explicit value for older data. Adding a reference in V3 could use `None`:

<!-- reference:optional-field.rs -->
```rust
#[history(added_in = v3, backfill_value = None)]
pub reference: Option<String>,
```

This is a field fragment for the history declaration. New `ReceiptCreated` constructors
must supply `reference`, even when its value is `None`.
`backfill_value = Type::default()` is also allowed when that default is right for older records.

An update can replace the previous value with an expression:

<!-- reference:constant-update.rs -->
```rust
#[history(updated_in = v3, previous_type = String, backfill_value = "USD".to_owned())]
#[history(added_in = v2, backfill_value = "USD".to_owned())]
pub currency: String,
```

This assigns `"USD"` when upgrading V2 to V3, including receipts whose V2 currency was EUR. It demonstrates a replacement expression; it would not preserve the meaning of those EUR amounts. The type author must decide whether a replacement fits the stored data.

Both backfill forms run during historical conversion. New records must supply their own field values; a backfill is not a constructor default.

## Order and lifetime of a field

A field can be added once, updated several times, and removed once.
Write its history attributes newest first, as on the invoice's `count` field:

<!-- reference:ordered-field.rs -->
```rust
#[history(updated_in = v3, previous_type = u64, backfill_fn = stringify)]
#[history(updated_in = v2, previous_type = u32, backfill_fn = widen)]
pub count: String,
```

Read the declaration from top to bottom. The field has type `String` in V3. The first attribute records `u64` as its V2 type; the second records `u32` as its V1 type. Type History reconstructs those types before generating the forward conversions.

- Each attribute uses a different version above V1.
- Updates follow the field's addition and precede its removal.
- A removed field name stays reserved for its earlier versions.

When an added field later changes type, its backfill must produce the type it
had when first added. The next update's `previous_type` records that earlier type.
Each backfill function borrows the complete struct from the immediately previous version.

## Rename, merge, or split fields

Use `backfill_fn` when a new value depends on other fields.
The conversion borrows the complete previous record and returns one destination
field's value. For the invoice's rename, the field declarations are:

<!-- reference:rename-fields.rs -->
```rust
#[history(removed_in = v2)]
pub legacy: String,

#[history(added_in = v2, backfill_fn = label)]
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

For the rename, `label` clones the string because its input is borrowed and the new field needs to own its value. Other callbacks can still read the same V1 string. Type History runs all backfills in field declaration order before moving unchanged fields. Each callback sees the previous record's values and cannot read another callback's result.

## Changes that keep the same type

A field can change meaning while keeping its Rust type. Record that change with
`updated_in`, the same type as `previous_type`, and a conversion function. For
example, V3 of `ReceiptCreated` could require uppercase currency codes:

<!-- reference:same-type-field.rs -->
```rust
#[history(updated_in = v3, previous_type = String, backfill_fn = uppercase)]
#[history(added_in = v2, backfill_value = "USD".to_owned())]
pub currency: String,
```

The conversion is an ordinary function. `Infallible` is useful when it always succeeds:

<!-- reference:same-type-callback.rs -->
```rust
use std::convert::Infallible;

fn uppercase(previous: &ReceiptCreatedV2) -> Result<String, Infallible> {
    Ok(previous.currency.to_ascii_uppercase())
}
```

A V2 event with `"eur"` now loads as a current `ReceiptCreated` event with `"EUR"`.
New values must already follow the application's currency rules; historical
conversions do not validate current constructors. Fields without a change at V3
carry their values forward.

Every new version needs a field boundary. There is no separate version header
for adding a version with no field changes.

## Rejected declarations

The compiler checks both the history attributes and the functions they name.
For example, it rejects an addition without a value for older records or a
conversion that accepts the wrong previous type.

This field says when `revision` appears but leaves its earlier value undefined:

```text
#[history(added_in = v2)]
pub revision: u32,
```

The V2 example supplies `backfill_value = 7_u32` to complete the declaration.

Each edit below applies to the [complete V2 invoice source](guide.md#v2-remove-convert-and-add-fields) and fails a Cargo build:

| Rejected edit | Diagnostic excerpt | Correction |
| --- | --- | --- |
| Remove `backfill_value = 7_u32` | `a history record requires` | Supply the value for earlier invoices through a backfill or record callback |
| Replace `previous_type = u32` with `from = u32` | `unknown history key` | Use `previous_type` |
| Remove `backfill_fn = widen` | `a history record requires` | Supply one conversion |
| Add a record callback beside the backfill | `exactly one of` | Choose either the backfill or the callback |
| Change `widen(previous: &InvoiceV1)` to `widen(previous: &InvoiceV2)` | `mismatched types` | Borrow the previous struct, `&InvoiceV1` |
| Return `Result<String, ConvertError>` from `widen` | `mismatched types` | Return the V2 field type, `u64` |
| Change `label` to accept `&InvoiceV2` | `mismatched types` | Borrow the previous version, `&InvoiceV1` |
| Set `updated_in = v0` | `history versions are positive` | Use the next permitted positive version |
| Set `updated_in = v4294967295` above frozen V1 | `exceeds the one permitted successor V2` | Introduce V2 before a later version |

Other rejected declarations include unsupported error types, async callbacks,
capturing closures, duplicate attribute keys, and attributes in the wrong version order.
The [evolution matrix](evolution-matrix.md) records the supported and rejected combinations.
