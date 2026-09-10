# Follow a record through its history

Typehistory keeps earlier versions of a Rust record available as its fields change.
The [README tutorial](../README.md#start-an-empty-package) shows the complete
setup and declarations. This guide explains what its examples establish and how
the executable checks exercise the same changes.

## Start with the Receipt example

Receipt V1 contains an amount in cents. V2 adds currency, because the example shop
starts accepting currencies besides US dollars. An earlier receipt has no currency
field, so its conversion needs an explicit value. The declaration supplies `"USD"`
based on the shop's earlier use of dollars.

The tutorial initializes an empty `receipt-history` package before declaring V1.
Initialization creates `typehistory/schemas.json`, the ledger that records frozen
schemas. V1 initially builds as a draft. Freezing it establishes the schema that
later builds must preserve.

After V2 is added, one declaration generates `ReceiptV1` and `ReceiptV2`.
`Receipt` becomes an alias of `ReceiptV2`. The conversion fills V2's new currency
field while leaving V1's generated type unchanged.

The [receipt tests](../README.md#6-read-v1-and-write-v2) deserialize a V1 `Versioned<Receipt>` and pass it to `Receipt::from_versioned`.
They check the current amount and currency. They also construct a new `Receipt`
with `"EUR"` and serialize `receipt.into_versioned()`.
That second case shows that the backfill does not override a currency supplied by current application code.

Application code uses `Receipt` throughout. `Versioned` carries the stable identity,
source version, and matching historical data without separate metadata arguments.
The application chooses storage and a Serde format supporting maps and named payload fields.
JSON and MessagePack's `to_vec_named` support that representation.

Declare the identity with `stable_id` and keep its exact value after freezing.
Renaming a Rust type or moving its module does not change that identity.

## Add conversions that use earlier fields

The [invoice walkthrough](../README.md#evolve-an-invoice-through-v1-v2-and-v3)
starts in a separate `invoice-history` package. Keeping it separate lets its V1
begin with an empty ledger and preserves the Receipt tutorial's history.

Invoice [V1](../README.md#v1-retain-the-original-fields) contains `legacy: String` and
`count: u32`. After it is frozen, [V2](../README.md#v2-remove-convert-and-add-fields) replaces
`legacy` with `label`, widens `count` to `u64`, and adds `revision`.

The two conversion functions need different inputs. `widen` consumes the earlier
count because that value alone determines the new count. `label` borrows the
entire `InvoiceV1` because its input comes from another field. Typehistory calls
record-borrowing functions before moving any previous fields, so `legacy` remains
available until `label` has read it.

The revision backfill supplies 7 explicitly. This number belongs to the example;
Typehistory cannot infer which revision an application should assign its earlier data.

## Keep every conversion step

Invoice [V3](../examples/invoice/src/lib.rs) changes the count to a `String`.
It keeps the V1-to-V2 conversion alongside the new V2-to-V3 conversion.
Reading V1 with count 42 therefore produces `42_u64` first, then `"42"`.
The current alias becomes `InvoiceV3`, while both earlier generated types remain available.

The example rejects count 13 to exercise a failure in the second conversion.
The [invoice tests](../examples/invoice/tests/history.rs) verify that the resulting
error records source V1 and the failing V2-to-V3 step. They also recover the
original `ConvertError` through `Error::source()` and inspect its count.

That error follows ordinary Rust error handling. The callback's error type implements
`std::error::Error + Send + Sync + 'static`, and Typehistory preserves it as the cause.
Version metadata tells the caller where conversion stopped; the cause explains
why the callback rejected that value.

Deserialization errors come from the selected Serde format. Conversion errors come
from `Invoice::from_versioned`, after the complete stored record has been decoded.
Reading a `Versioned` does not rewrite stored bytes or automatically run conversions.

## Check schemas and conversion results

A frozen schema protects historical field names, serialized types, and supported
nested structure. It also prevents deleting a retained version or history.
It cannot prove that a callback returns the correct business value.
The Receipt backfill would still produce a `String` if its currency were wrong.

The Rust integration tests check both properties. Ordinary builds reject incompatible
declarations, and runtime assertions check specific conversion results. The README's
[rejection table](../README.md#rejected-declarations) explains the invalid edits
used by those compiler checks.

## Exercise development and release rules

After V2 is frozen, V3 can remain a draft while its conversion is tested.
Development builds allow that draft with a warning. Release builds and explicit
strict checks reject it until it is frozen. Custom profiles follow the development
or release profile from which they inherit.

The tutorial test also discards an unfrozen V3 by restoring the complete V2 source.
This restores the field types and callers as well as removing V3 annotations.
The original ledger then accepts the source again without a lifecycle command.

Reset is a separate operation for replacing a frozen experimental version.
The tutorial test checks that a reset reservation prevents strict builds, that undo
requires the original schema, and that an exact freeze accepts a replacement.
The README's [reset instructions](../README.md#reset-and-undo) explain the
data-loss risk before those commands.

Import checks use another empty package with a complete ledger and matching source.
A history beginning above V1 requires an explicit
[retained-floor import](../README.md#import-a-retained-floor).
The standalone suite uses a separate V4 history for that case; it does not trim
V1 through V3 from the invoice walkthrough.

## Run the executable checks

Run `cargo test` in each tutorial package to check its conversion examples.
The repository's [contributor guide](../CONTRIBUTING.md) covers the full suite,
including tests that compile the README examples and check rejected declarations.

The [reference application](../examples/invoice/README.md) includes the completed
invoice history, its frozen ledger, and conversion tests.
Conversion tests for a real application still need representative records and
the application's own business rules.
