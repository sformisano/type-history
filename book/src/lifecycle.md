# Freezing and lifecycle commands

A **draft** is the one unfrozen version you can edit during development.
Freezing saves its schema in `type-history/schemas.json`, the **ledger**.
Later builds compare the generated historical types with those saved schemas.
See [package setup](setup.md) for the build hook that enables these checks.

## What freezing protects

Freezing protects the structure of earlier records. The build checks the actual
types resolved by the compiler against the saved schema, including nested types.
This catches changes made through type aliases or dependencies as well as direct edits.

### What builds reject

- **Changed stable name:** Changing a frozen history's `stable_name` fails the
  build, even when its Rust name or module changes.
- **Changed historical fields:** A frozen version must keep its serialized field
  names, types, and nested structure.
- **Deleted history:** A frozen history or retained version cannot disappear from
  the declaration during ordinary development.
- **Invalid ledger:** A missing or malformed ledger fails the build. Duplicate
  JSON keys, version gaps, and inconsistent metadata are rejected too.

If a nested record needs a new structure, keep its earlier definition and add a
field update that converts it to the new type.

### What conversion tests must check

The shop example in the [quick start](quick-start.md) requires older receipts to use US dollars.
Both of these values pass the same type check:

<!-- reference:business-values.rs -->
```rust
let usd: String = "USD".to_owned();
let eur: String = "EUR".to_owned();
assert_ne!(usd, eur);
```

Only `"USD"` matches the shop's earlier receipts. The compiler checks types;
the [receipt assertions](quick-start.md#6-read-and-write-through-the-current-type) check that business rule.
A conversion can return the wrong value while preserving the expected type.

Schemas describe serialized structure, so some source edits leave them unchanged:

- Replacing a Rust type with another that has an equivalent serialized structure.
- Reordering fields, which can change the order in which callbacks run.
- Removing a conversion that preserves the schema, if other field boundaries
  still describe every retained version.

Test conversions with representative historical records. Make each conversion
depend on its input: avoid I/O, clocks, randomness, and externally visible side effects.

The generated conversion chain runs from older versions to the current type.
It does not provide conversions back to older versions. Rust callers also need updating when the current type's fields change.
For example, code accessing a [removed field](field-history.md#rename-merge-or-split-fields) fails compilation.

## Lifecycle commands

The CLI creates and updates the ledger used by Cargo builds. Run its commands
from your application's workspace.

- **Choose a package:** Every command that writes a ledger requires `--package NAME`.
- **Choose a version:** Supply `--type EXACT_NAME` and `--version N` together when
  selecting one version. Wildcards are rejected.
- **Choose build settings:** Use `--features LIST` or `--target TRIPLE` when needed.
  Custom target files are unsupported.

The [setup guide](setup.md#2-initialize-the-schema-file) explains `init`, and the
[quick start](quick-start.md#2-freeze-v1) shows `freeze`.
Use `check` to validate existing histories.
The command examples below use the [invoice package](guide.md).
The remaining commands handle experimental resets or imported histories.

| Command | Effect |
| --- | --- |
| `cargo type-history init --package invoice-history` | Create the ledger before any history declaration exists |
| `cargo type-history check` | Check configured workspace libraries without writing package files |
| `cargo type-history check --package invoice-history` | Check one configured library |
| `cargo type-history freeze --package invoice-history` | Freeze all ordinary drafts after validating the complete package |
| `cargo type-history freeze --package invoice-history --type billing.invoice.issued --version 3` | Freeze exactly one ordinary draft or reset reservation |

### Iterate and discard an ordinary draft

WARNING: Draft edits can make development records unreadable. Freeze a version
before relying on its representation for data you must retain.

After freezing V2, you can edit V3 repeatedly while developing its conversion.
Each history allows one new draft at a time. Freeze V3 before adding V4.

To discard an unfrozen version:

1. Restore the complete source for the last frozen version.
2. Update callers to use that version's fields and types.
3. Build again. No ledger command is needed.

For the invoice, restoring V2 makes `Invoice` an alias of `InvoiceV2` again.
Removing only the V3 attribute while keeping `count: String` would change frozen
V2, so the build rejects it. Any remaining V3 boundary keeps the draft present.

A history with no frozen versions can be deleted entirely.

### Reset and undo

WARNING: Replacing a frozen schema can make stored records unreadable. Use reset
only for disposable experimental data. Add a successor version for durable records.

Reset lets you replace the highest frozen version during experiments. It is
allowed only when there is no newer draft. The ledger keeps the original schema
and marks that version as open for replacement, called a **reset reservation**.

```sh
cargo type-history reset --package invoice-history \
  --type billing.invoice.issued --version 3
```

While the reservation exists:

- Development builds allow edits to that version. Builds requiring frozen
  versions, including release builds, reject the reservation.
- Earlier versions remain protected.
- You cannot add a newer version or clear the reservation by deleting source.

Finish a reset in one of two ways:

**Accept the replacement:** Freeze the exact stable name and version after editing
and testing the declaration. Freezing the whole package rejects pending reservations.

```sh
cargo type-history freeze --package invoice-history \
  --type billing.invoice.issued --version 3
```

**Undo the reset:** Restore source that matches the saved schema, then clear the
reservation. Undo does not edit source; a schema mismatch leaves the reservation open.

```sh
cargo type-history reset --undo --package invoice-history \
  --type billing.invoice.issued --version 3
```

### Import a retained floor

Use import when bringing an existing history and its ledger into a package.
Some histories start above V1. Their retained floor is the earliest version the
program supports.

Import requires a complete ledger in the current format. It cannot change or
remove entries already frozen in the destination ledger.

Each imported floor above V1 requires a matching `--floor EXACT_NAME=N` argument.
The declaration must state the same floor:

```text
use type_history::versioned;

#[versioned(stable_name = "billing.invoice.issued", retained_from = v4)]
```

This fragment belongs to a separate history starting at V4. Unannotated fields
start at V4, and Type History generates no V1, V2, or V3 types for it.
Do not apply it to the invoice guide's existing V1-to-V3 history to discard old versions.

The source and imported ledger must agree:

- The ledger contains every declared stable name and supported version.
- The floor annotation matches the imported floor and its `--floor` argument.
- The declaration's floor or field boundaries describe its latest version.

The annotation alone cannot supply missing historical schemas.

With the declaration and `imported.json` prepared, import them together:

```sh
cargo type-history import --package invoice-history \
  --from imported.json --floor billing.invoice.issued=4
```

### Failed commands and recovery

Ledger writes are validated before the file is replaced. A package lock prevents
concurrent writes, and the command checks that its source inputs have not changed.
The lock stays held until the command reads back the result.

Recovery depends on whether the replacement happened:

- **Before replacement:** A busy lock, changed input, or validation failure leaves
  the ledger unchanged. Temporary candidate files can be discarded.
- **After replacement:** The error includes `COMMITTED`. Inspect the live ledger
  before retrying because the operation has already changed it.

Each package is updated separately. These commands do not modify stored record bytes.

## Build profiles and environment

Development builds let you work on a draft. Release builds require every version
to be frozen. To apply the same requirement during development:

```sh
TYPE_HISTORY_REQUIRE_FROZEN=1 cargo build --locked
```

| Consumer profile or setting | Ordinary draft or reset reservation |
| --- | --- |
| Development | Allowed with warning |
| `TYPE_HISTORY_REQUIRE_FROZEN=1 cargo build` | Rejected |
| `AXIS_REQUIRE_FROZEN_EVENTS=1 cargo build` | Rejected |
| Release family | Rejected |
| Custom profile inheriting `dev` | Development rules |
| Custom profile inheriting `release` | Release rules |

`AXIS_REQUIRE_FROZEN_EVENTS` is also accepted for compatibility with Axis tooling.
Both strict variables must be unset or exactly `1`. Empty strings, `0`, and `true`
fail, even when the other variable equals `1`. Neither can relax release checks.
Custom profiles follow their inherited family, regardless of optimization or `debug_assertions`.

### When checks run

Cargo builds check source and configuration against the committed ledger.
Changes to the ledger, relevant modules, manifests, or strict settings rerun the
checks. Compiler assertions also check changed nested dependencies when build
output is cached. Builds do not freeze, repair, or rewrite the ledger.

### Package configuration

- A consumer needs a library target, the `build.rs` hook, and `format = 1` in
  `type-history.toml`. Depending on the runtime alone does not enable ledger checks.
- The ledger lives at `type-history/schemas.json`; its location is fixed.
- No `links` value, manifest metadata, or second declaration file is needed.
- A package cannot configure both standalone and Axis ledgers.

### Variables used by tooling

The build hook sets `TYPE_HISTORY_LEDGER_PATH`, `TYPE_HISTORY_SCHEMA_ID_PREFIX`, and
`TYPE_HISTORY_AUTHORITY_KIND`. Do not set them yourself.

`TYPE_HISTORY_SCHEMA_EXPORT` and `AXIS_SCHEMA_EXPORT` are used by export tooling.
They must be unset or `1` and cannot relax release or explicit strict checks.
Lifecycle export uses a captured copy of development sources. Forced Cargo
environment settings remain effective; conflicting forced settings fail.
