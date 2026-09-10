# Evolve an invoice through V1, V2, and V3

This example extends the [README walkthrough](../README.md#how-it-works) with a field rename and a conversion that can fail.
It follows an invoice through three versions in a separate package.

This example covers:

- **Type changes:** Convert a count from `u32` to `u64`, then to `String`.
- **Renames:** Read `legacy` from the previous record to populate the new `label` field.
- **Failures:** Find the original stored version and the conversion step that failed.

## Prepare a separate invoice package

Create a fresh package called `invoice-history`. It needs its own ledger so you
can declare and freeze V1 before adding V2.

Create it alongside `receipt-history`, in a separate directory.

```sh
cargo new --lib invoice-history --edition 2024
cd invoice-history
```

1. Copy the [setup manifest](setup.md#1-create-the-package).
2. Set `name = "invoice-history"` in its `[package]` section.
3. Create `type-history.toml` and `build.rs` using the [initialization instructions](setup.md#2-initialize-the-schema-file).
4. Empty `src/lib.rs`.

Initialize the package:

```sh
cargo generate-lockfile
cargo fetch --locked
cargo type-history init --package invoice-history
```

Continue in `invoice-history` for the following steps.

## V1: retain the original fields

The first invoice has a `legacy` label and a count stored as `u32`.

Write this complete `src/lib.rs`:

<!-- example:versions/v1.rs -->
```rust
use type_history::versioned;

#[versioned(stable_name = "billing.invoice.issued")]
pub struct Invoice {
    pub legacy: String,
    pub count: u32,
}
```

`Invoice` currently aliases `InvoiceV1`. Freeze this starting point before changing it:

```sh
cargo build --locked
cargo type-history freeze --package invoice-history
```

## V2: remove, convert, and add fields

V2 makes three changes:

- **Rename `legacy` to `label`:** Mark `legacy` as removed and add `label` in V2.
  Keep the `legacy` declaration so Type History can generate `InvoiceV1`.
- **Widen `count` to `u64`:** Record its previous type, `u32`, and provide a conversion.
- **Add `revision`:** Assign 7 to earlier invoices. This value is a choice made
  for the example; your application needs a value that fits its own history.

Replace `src/lib.rs` with this complete source:

<!-- example:versions/v2.rs -->
```rust
use std::{
    error::Error,
    fmt::{Display, Formatter, Result as FmtResult},
};
use type_history::versioned;

#[versioned(stable_name = "billing.invoice.issued")]
pub struct Invoice {
    #[history(removed_at = v2)]
    pub legacy: String,
    #[history(updated_at = v2, previous_type = u32, cast_from_previous_value = widen)]
    pub count: u64,
    #[history(added_in = v2, cast_from_previous_payload = label)]
    pub label: String,
    #[history(added_in = v2, backfill_value = 7_u32)]
    pub revision: u32,
}

fn widen(value: u32) -> Result<u64, ConvertError> {
    Ok(u64::from(value))
}

fn label(previous: &InvoiceV1) -> Result<String, ConvertError> {
    Ok(previous.legacy.clone())
}

#[derive(Debug)]
pub struct ConvertError(pub u64);

impl Display for ConvertError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        write!(formatter, "count {} cannot be converted", self.0)
    }
}

impl Error for ConvertError {}
```

The two conversion functions take different inputs:

- **`widen` takes the old field value:** `cast_from_previous_value` passes it the
  previous `count` by value. It returns the count as `u64`.
- **`label` takes the old record:** `cast_from_previous_payload` passes it
  `&InvoiceV1`. It clones `legacy` to produce the new label.

The rename needs access to the whole previous record because V1 has no field
called `label`. Type History calls record conversions before moving any fields,
so `legacy` remains available until `label` has read it.

Conversion functions, also called callbacks, return `Result`. These two always
return `Ok`; V3 introduces a conversion that can fail.

Build and freeze V2:

```sh
cargo build --locked
cargo type-history freeze --package invoice-history \
  --type billing.invoice.issued --version 2
```

## V3: keep both adjacent conversions

V3 changes `count` from `u64` to `String`. The declaration keeps the earlier
`u32`-to-`u64` conversion because V1 records still need that step.

Replace `src/lib.rs` with this complete source:

<!-- example:versions/v3.rs -->
```rust
use std::{
    error::Error,
    fmt::{Display, Formatter, Result as FmtResult},
};
use type_history::versioned;

#[versioned(stable_name = "billing.invoice.issued")]
pub struct Invoice {
    #[history(removed_at = v2)]
    pub legacy: String,
    #[history(updated_at = v3, previous_type = u64, cast_from_previous_value = stringify)]
    #[history(updated_at = v2, previous_type = u32, cast_from_previous_value = widen)]
    pub count: String,
    #[history(added_in = v2, cast_from_previous_payload = label)]
    pub label: String,
    #[history(added_in = v2, backfill_value = 7_u32)]
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

#[derive(Debug)]
pub struct ConvertError(pub u64);

impl Display for ConvertError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        write!(formatter, "count {} cannot be converted", self.0)
    }
}

impl Error for ConvertError {}
```

The newest update appears first on `count`. Its history now describes `u32` in
V1, `u64` in V2, and `String` in V3.

Reading V1 runs both conversions in order:

| Stored count | V1 to V2: `widen` | V2 to V3: `stringify` |
| --- | --- | --- |
| 42 | Produces `42_u64` | Produces `"42"` |
| 13 | Produces `13_u64` | Returns `ConvertError(13)` |

The rejection of 13 exists only to demonstrate failure handling. The resulting
error records V1 as the source version and V2-to-V3 as the failed step.

Append this test module to `src/lib.rs`:

<!-- example:runtime-tests.rs -->
```rust
#[cfg(test)]
mod tests {
    use super::{ConvertError, Invoice};
    use serde_json::from_slice;
    use std::error::Error;
    use type_history::DecodeFailureKind;

    #[test]
    fn old_bytes_reach_the_current_type() {
        let stored = from_slice(
            br#"{"stable_name":"billing.invoice.issued","version":1,"payload":{"legacy":"INV-42","count":42}}"#,
        ).unwrap();
        let value = Invoice::from_versioned(stored).unwrap();
        assert_eq!(value.count, "42");
        assert_eq!(value.label, "INV-42");
        assert_eq!(value.revision, 7);
        let _: Invoice = Invoice {
            count: "42".to_owned(),
            label: "current".to_owned(),
            revision: 7,
        };
    }

    #[test]
    fn failure_keeps_the_original_version_and_failing_step() {
        let stored = from_slice(
            br#"{"stable_name":"billing.invoice.issued","version":1,"payload":{"legacy":"INV-13","count":13}}"#,
        ).unwrap();
        let error = Invoice::from_versioned(stored).unwrap_err();
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
cargo type-history freeze --package invoice-history \
  --type billing.invoice.issued --version 3
cargo build --release --locked
```

The [invoice reference application](../examples/invoice/README.md) contains the completed history, ledger, and tests.
