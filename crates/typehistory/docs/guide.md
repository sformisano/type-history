# Build a standalone invoice history

Start with the [Typehistory README](../README.md#start-an-empty-package). It owns
the package setup, field grammar, lifecycle reference, and build rules.
This guide maps that setup to the complete [invoice example](../examples/invoice/README.md).

The consumer starts outside the Axis workspace with an empty library. It has a
Typehistory build hook and `typehistory.toml`, but no ledger or histories yet.
Initialize it before adding the first declaration.

| Step | Source or command | Expected result |
| --- | --- | --- |
| Initialize | README manifest, `build.rs`, configuration, and `cargo typehistory init` | Empty `typehistory/schemas.json` |
| Author V1 | [versions/v1.rs](../examples/invoice/versions/v1.rs) | `Invoice` aliases `InvoiceV1`; development warns about its draft |
| Freeze V1 | Package freeze | Frozen V1 protects `legacy: String` and `count: u32` |
| Try rejected edits | README rejection table, checked by `verify.sh` | Ordinary Cargo fails; ledger bytes stay unchanged |
| Author V2 | [versions/v2.rs](../examples/invoice/versions/v2.rs) | Borrow old legacy text, widen count, and explicitly initialize revision |
| Try strict profiles | Strict development, release, and custom release profile | Unfrozen V2 fails; custom development permits it |
| Freeze V2 | Exact identity/version freeze | Both V1 and V2 become retained authority |
| Discard V3 | Install V3, then restore the full V2 file | Release builds pass with the original ledger |
| Author V3 | [versions/v3.rs](../examples/invoice/versions/v3.rs) | Count changes from `u64` to `String` through the second adjacent callback |
| Decode old bytes | [runtime-tests.rs](../examples/invoice/runtime-tests.rs) | Stored V1 reaches V3; failure keeps source V1 and step V2/V3 |
| Freeze V3 | Exact identity/version freeze | Release accepts the complete retained chain |
| Reset and undo | Exact V3 reset, strict failure, source-matching undo | Original frozen bytes are restored as ordinary authority |
| Refreeze a reservation | Exact V3 reset and exact V3 freeze | Reservation is resolved explicitly |
| Import | Complete current ledger into a second package with matching source | Imported source builds under release rules |

Run `verify.sh` using the example's documented environment. Each expected failure
requires both a failed command and its diagnostic. A successful final line means
that this invocation completed the sequence; retain its output for your tree.

For a real history starting above V1, follow [retained-floor import](../README.md#import-a-retained-floor).
Do not rewrite this invoice's V1 floor. The standalone suite exercises a separate
V4 import with an exact `--floor billing.invoice.issued=4` declaration.

The example's count rejection and revision value are explicit sample business
rules. Replace them with rules justified by your own stored data. Frozen schemas
protect shape and inventory; conversion tests protect the intended meaning.
