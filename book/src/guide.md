# Evolve an invoice through V1, V2, and V3

The shop's receipts showed how to add a currency and change an amount's type. This example follows an invoice that renames a field and changes its count twice. It also shows how a failure in the second conversion keeps the original stored version in its error.

The invoice changes in three ways:

- **Type changes:** Convert a count from `u32` to `u64`, then to `String`.
- **Renames:** Read `legacy` from the previous record to populate the new `label` field.
- **Failures:** Find the original stored version and the conversion step that failed.

## Prepare a separate invoice package

Create a fresh package called `invoice-history`. It needs its own ledger so you
can declare and freeze V1 before adding V2.

Create it alongside `my-shop-demo-project`, in a separate directory.

```sh
cargo new --lib invoice-history --edition 2024
cd invoice-history
```

1. Copy the [setup manifest](setup.md#1-create-the-package).
2. Set `name = "invoice-history"` in its `[package]` section.
3. Create `build.rs` with the same build hook from the [initialization instructions](setup.md#2-initialize-the-schema-file).
4. Empty `src/lib.rs`.

Initialize this package using its own name, `invoice-history`:

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

The application now calls the invoice's label `label`, needs a wider count, and records a revision. V2 makes those changes together:

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
    #[history(removed_in = v2)]
    pub legacy: String,
    #[history(updated_in = v2, previous_type = u32, backfill_fn = widen)]
    pub count: u64,
    #[history(added_in = v2, backfill_fn = label)]
    pub label: String,
    #[history(added_in = v2, backfill_value = 7_u32)]
    pub revision: u32,
}

fn widen(previous: &InvoiceV1) -> Result<u64, ConvertError> {
    Ok(u64::from(previous.count))
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

Both backfill functions borrow `&InvoiceV1`:

- **`widen` reads `previous.count`** and returns the count as `u64`.
- **`label` clones `previous.legacy`** to produce the new label.

Type History runs the backfills before moving unchanged fields into V2. Both functions can read the complete V1 record, including `legacy`. Removing that field from V2 does not remove it from the source passed to `label`.

Backfills run in field declaration order. They read the previous record, so one backfill cannot read another backfill's result.

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
    #[history(removed_in = v2)]
    pub legacy: String,
    #[history(updated_in = v3, previous_type = u64, backfill_fn = stringify)]
    #[history(updated_in = v2, previous_type = u32, backfill_fn = widen)]
    pub count: String,
    #[history(added_in = v2, backfill_fn = label)]
    pub label: String,
    #[history(added_in = v2, backfill_value = 7_u32)]
    pub revision: u32,
}

fn widen(previous: &InvoiceV1) -> Result<u64, ConvertError> {
    Ok(u64::from(previous.count))
}

fn label(previous: &InvoiceV1) -> Result<String, ConvertError> {
    Ok(previous.legacy.clone())
}

fn stringify(previous: &InvoiceV2) -> Result<String, ConvertError> {
    let value = previous.count;
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

The rejection of 13 is an artificial rule used to demonstrate failure handling. A count of 42 reaches V3 as `"42"`. A count of 13 passes through V2, then fails. The error still identifies V1 as the stored version and V2-to-V3 as the failing step.

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

The [invoice reference application](https://github.com/sformisano/type-history/blob/main/crates/examples/invoice-history/README.md) contains the completed history, ledger, and tests.
