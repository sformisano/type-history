# Field history reference

This reference covers the `#[versioned(...)]` struct arguments and `#[history(...)]` field attributes.
The [README walkthrough](../README.md#how-it-works) introduces them through a complete history.

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
| `stable_name = "billing.invoice.issued"` | On the struct; names the type and its history independently of its Rust name |
| `retained_from = vN` | On the struct; starts an imported history at N; requires a matching ledger and explicit import |
| `added_in = vN` | Field first exists in N; provide one backfill or conversion function |
| `updated_at = vN` | Field changes in N; provide its previous type and one conversion function |
| `removed_at = vN` | Field disappears in N; keep its source declaration for earlier versions |
| `previous_type = Old` | Required on an update; the field's type in the immediately previous version |
| `backfill_value = expression` | Addition only; produces the field's value in the version where it was added |
| `cast_from_previous_value = path` | Update only; a synchronous function consumes the previous field value |
| `cast_from_previous_payload = path` | Addition or update; a synchronous function borrows the complete previous record |

## Values and conversion functions

An addition or update must explain how to construct the destination field.
Choose one rule for each change:

| Change | Required rule | Conversion signature |
| --- | --- | --- |
| Addition | One backfill value or one record callback | `fn(&InvoiceVn) -> Result<AddedType, E>` for a callback |
| Update | One field callback or one record callback, plus `previous_type` | `fn(PreviousType) -> Result<Destination, E>` or `fn(&InvoiceVn) -> Result<Destination, E>` |
| Removal | None | None; record callbacks can still read the removed field from the previous version |

For ordinary histories, `E` must implement `std::error::Error + Send + Sync + 'static`.
For redacted histories, it must implement `ToRedacted`; see the
[callback error contract](redactable.md#callback-errors-and-causes).
Return `Ok(value)` when a conversion cannot fail.

The receipt fragments on this page are separate changes to the [V2 declaration](../README.md#3-add-a-field-in-v2).
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

## Order and lifetime of a field

A field can be added once, updated several times, and removed once.
Write its history attributes newest first, as on the invoice's `count` field:

<!-- reference:ordered-field.rs -->
```rust
#[history(updated_at = v3, previous_type = u64, cast_from_previous_value = stringify)]
#[history(updated_at = v2, previous_type = u32, cast_from_previous_value = widen)]
pub count: String,
```

The field uses `String` in V3. The attributes preserve its earlier types: `u64` in
V2 and `u32` in V1.

- Each attribute uses a different version above the retained floor.
- Updates follow the field's addition and precede its removal.
- A removed field name stays reserved for its earlier versions.

When an added field later changes type, its backfill must produce the type it
had when first added. The next update's `previous_type` records that earlier type.
Each conversion then receives the type from the immediately previous version.

## Rename, merge, or split fields

Use `cast_from_previous_payload` when a new value depends on other fields.
The conversion borrows the complete previous record and returns one destination
field's value. For the invoice's rename, the field declarations are:

<!-- reference:rename-fields.rs -->
```rust
#[history(removed_at = v2)]
pub legacy: String,

#[history(added_in = v2, cast_from_previous_payload = label)]
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

All record callbacks run before fields move. They read the previous record's
values, including removed fields. They cannot read another callback's output.
Clone a value when the new field needs ownership.

## Changes that keep the same type

A field can change meaning while keeping its Rust type. Record that change with
`updated_at`, the same type as `previous_type`, and a conversion function. For
example, V3 could require uppercase currency codes in the receipt:

<!-- reference:same-type-field.rs -->
```rust
#[history(updated_at = v3, previous_type = String, cast_from_previous_value = uppercase)]
#[history(added_in = v2, backfill_value = "USD".to_owned())]
pub currency: String,
```

The conversion is an ordinary function. `Infallible` is useful when it always succeeds:

<!-- reference:same-type-callback.rs -->
```rust
use std::convert::Infallible;

fn uppercase(value: String) -> Result<String, Infallible> {
    Ok(value.to_ascii_uppercase())
}
```

A V2 receipt with `"eur"` now loads as a current receipt with `"EUR"`.
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
| Remove `backfill_value = 7_u32` | `an added field requires` | Supply the value for earlier invoices through a backfill or record callback |
| Replace `previous_type = u32` with `from = u32` | `unknown history key` | Use `previous_type` |
| Remove `cast_from_previous_value = widen` | `an updated field requires` | Supply one conversion |
| Add a record callback beside the backfill | `exactly one assignment rule` | Choose either the backfill or the callback |
| Change `widen(value: u32)` to `widen(value: u16)` | `mismatched types` | Accept the previous field's exact type, `u32` |
| Return `Result<String, ConvertError>` from `widen` | `mismatched types` | Return the V2 field type, `u64` |
| Change `label` to accept `&InvoiceV2` | `mismatched types` | Borrow the previous version, `&InvoiceV1` |
| Set `updated_at = v0` | `history versions are positive` | Use the next permitted positive version |
| Set `updated_at = v4294967295` above frozen V1 | `exceeds the one permitted successor V2` | Introduce V2 before a later version |

Other rejected declarations include unsupported error types, async callbacks,
capturing closures, duplicate attribute keys, and attributes in the wrong version order.
The [evolution matrix](evolution-matrix.md) records the supported and rejected combinations.
