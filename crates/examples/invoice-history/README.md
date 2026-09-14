# Invoice reference application

The shop example in the README follows a type one change at a time. This package
shows the result in a runnable application: read a stored V1 invoice as the
current V3 type, then serialize a new invoice with its V3 metadata. It also
converts a V1 payment's enum field into its current V2 form.

The [declarations](src/lib.rs), [build hook](build.rs), and
[frozen schema ledger](type-history/schemas.json) are included. You can run the
application without repeating the edits and freezes from the walkthrough.

Run from the repository root:

```sh
cargo run -p invoice-history --locked
cargo test -p invoice-history --locked
cargo run -p invoice-history --release --locked
```

The application uses `Invoice` throughout. It wraps current values with
`into_versioned`, serializes them, then calls `Invoice::from_versioned` after
deserialization. It prints the resulting invoices and payment.

The [versioned value tests](tests/versioned.rs) cover JSON and MessagePack,
historical conversion, metadata validation, and preserved callback errors.
The [raw JSON tests](tests/history.rs) cover the low-level API for
applications that already keep metadata separately.

The [payment declaration](src/payment.rs) preserves `PaymentStatusV1` and updates
the containing `Payment` struct to use `PaymentStatus`. Its backfill reads the
old status and currency code from the complete previous struct. A paid amount
of 4200 in USD becomes `Paid { amount_minor: 4200, currency: Currency::Usd }`.

The [payment tests](tests/payment.rs) check fixed historical JSON and MessagePack,
both enum forms, current serialization, and conversion errors. They assert literal
expected amounts and currencies, so a conversion returning EUR for USD fails even
when the frozen schemas still match.

The [invoice walkthrough](../../../book/src/guide.md)
explains how to author and freeze each version. The
[tutorial tests](../../core/cargo-type-history/tests/standalone/tutorial.rs)
repeat those edits, check the expected compiler failures, and run the lifecycle
commands:

```sh
cargo test -p cargo-type-history --test standalone tutorial --locked
```

These tests compile the exact marked Rust blocks from the documentation in
temporary consumer packages. They run through Cargo, like the other integration
tests.
