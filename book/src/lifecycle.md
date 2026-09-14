# Freezing and lifecycle commands

The shop freezes V1 before adding `currency` in V2. That saves V1's schema in `type-history/schemas.json`, the **ledger**. Later builds check that the declaration still produces the same V1 structure.

V2 remains a **draft** until its schema is frozen. You can edit that latest draft during development while earlier versions stay protected. Each history allows only one draft at a time. See [package setup](setup.md) for the build hook that enables these checks.

## What freezing protects

Freezing protects the serialized structure of earlier records. For example, a frozen receipt must keep its original field names and amount type. The check uses the types selected by the compiler, including nested types. Changing a type alias or updating a dependency therefore cannot bypass the check by leaving the field's spelling unchanged.

### What builds reject

- **Changed stable name:** Changing a frozen history's `stable_name` fails the
  build, even when its Rust name or module changes.
- **Changed historical fields:** A frozen version must keep its serialized field
  names, types, and nested structure.
- **Deleted history:** A frozen history or retained version cannot disappear from
  the declaration during ordinary development.
- **Invalid ledger:** A missing or malformed ledger fails the build. Duplicate
  JSON keys, version gaps, and inconsistent metadata are rejected too.

If a nested record or enum needs a new structure, keep its earlier definition and add a
field update that converts it to the new type.

### What conversion tests must check

In the [quick start's shop example](quick-start.md), V1 `ReceiptCreated` events
need `"USD"` because the shop accepted only US dollars then.
Both of these values pass the same type check:

<!-- reference:business-values.rs -->
```rust
let usd: String = "USD".to_owned();
let eur: String = "EUR".to_owned();
assert_ne!(usd, eur);
```

Only `"USD"` matches the currency used by those V1 events. The compiler checks types;
the [conversion assertions](quick-start.md#6-read-and-write-through-the-current-type) check that business rule.
A conversion can return the wrong value while preserving the expected type.

Schemas describe serialized structure, so some source edits leave them unchanged:

- Replacing a Rust type with another that has an equivalent serialized structure.
- Reordering fields, which can change the order in which callbacks run.
- Removing a conversion that preserves the schema, if other field boundaries
  still describe every retained version.

Test conversions with representative historical records. Make each conversion
depend on its input: avoid I/O, clocks, randomness, and externally visible side effects.

The [payment fixtures](https://github.com/sformisano/type-history/blob/main/crates/examples/invoice-history/tests/payment.rs)
read fixed historical JSON and MessagePack and assert the exact current amount
and currency. Returning EUR for an old USD payment fails those tests even though
the schemas still match.

The generated conversions let newer code read older records. They do not let an unchanged old service read a new version. Follow the [distributed rollout example](quick-start.md#using-type-history-in-a-distributed-system) when services upgrade separately.

Application code also needs to match the current type. For example, code accessing a [removed field](field-history.md#rename-merge-or-split-fields) fails compilation.

## Lifecycle commands

The CLI creates and updates the ledger used by Cargo builds. Run its commands
from your application's workspace.

- **Choose a package:** Every command that writes a ledger requires `--package NAME`.
- **Choose a version:** Supply `--type EXACT_NAME` and `--version N` together when
  selecting one version. Wildcards are rejected.
- **Choose build settings:** Use `--features LIST`, `--no-default-features`, or `--target TRIPLE` when needed.
  Custom target files are unsupported.

`--features LIST` adds features to the package defaults. Use `--no-default-features`
to disable those defaults. Both selections apply to Cargo metadata, schema export,
and ordinary compilation during each lifecycle command.
For a package with an alternative `backend-alt` feature:

```sh
cargo type-history check --package my-shop-demo-project --no-default-features --features backend-alt
```

Selected features must still preserve every frozen schema. These options select
consumer features for metadata, schema export, and compilation.

If you call the build library directly, `LifecycleOptions::no_default_features` controls the same choice. Set it to `false` to keep package defaults or `true` to disable them. `workspace::metadata(features)` keeps package defaults; `options::parse` reads the command-line selection.

The [setup guide](setup.md#2-initialize-the-schema-file) explains `init`, and the
[quick start](quick-start.md#2-generate-and-freeze-v1s-schema) shows `freeze`.
Use `check` to validate existing histories.
The command examples below use the [invoice package](guide.md).
The remaining commands handle experimental resets or imported histories.

| Command | Effect |
| --- | --- |
| `cargo type-history init --package invoice-history` | Create the ledger before any history declaration exists |
| `cargo type-history check` | Check initialized workspace libraries without writing package files |
| `cargo type-history check --package invoice-history` | Check one selected library |
| `cargo type-history check --package invoice-history --released-baseline released-schemas.json` | Also preserve every history frozen in a separately supplied release ledger |
| `cargo type-history check --format json` | Report workspace checks as one JSON document |
| `cargo type-history freeze --package invoice-history` | Freeze all ordinary drafts after validating the complete package |
| `cargo type-history freeze --package invoice-history --type billing.invoice.issued --version 3` | Freeze exactly one ordinary draft or reset reservation |

### Compare with a released ledger

Ordinary builds compare source against the working ledger. To protect released
history in CI, obtain `type-history/schemas.json` from the release you trust and
save it separately. Supply that file when checking its package:

```sh
cargo type-history check --package invoice-history --released-baseline released-schemas.json
```

The command requires one exact package and a file distinct from its current
ledger. It rejects missing released histories or versions, changed metadata or
schemas, and reset reservations for released versions. The released file itself
must contain only frozen versions. New histories and later versions are allowed.

The check takes a copy of both ledgers and the source, then compares the release ledger before compiling. Editing the source and working ledger together therefore cannot hide a change to a released schema. It does not rewrite either ledger or the source, on success or failure.

Workspace checks select initialized packages from one shared source copy. The
command verifies Cargo's package graph against that copy before validation and
checks the original inputs again after all selected packages have been checked.
A changed package graph or input fails the command. Source files remain included
even inside directories named `node_modules`, `.next`, `.git`, or `.schemas.lock`;
only identified Cargo caches, Git metadata, and package lock paths are excluded.

These checks detect changes by comparing inputs at specific points. They do not
lock source files against editors, and an edit after the final comparison cannot
be detected. Avoid editing inputs during a command; use an immutable source
checkout when another process could change them.

Your CI job chooses the trusted release file. Type History does not fetch or
authenticate it. Ordinary builds and running applications need no release
registry or baseline file.

### Structured diagnostics

Use `--format json` when another tool needs to read the results. The command writes one JSON document to stdout. A successful check exits with status 0; a failed check exits with status 2. Both `--format` and `--released-baseline` apply only to `check`.

If a nested address field changes, the report identifies the history, version, and path to that field. Each difference includes a stable code, expected and actual values, a correction hint, and a source location when available. Paths distinguish fields, enum variants, tuple positions, and container elements. Missing values use JSON null. The report includes independent differences in a consistent order.

The command still runs compiler checks. When a frozen schema check fails, the report compares the structures observed by that compiler run. A location may point to the containing declaration when the nested type's location is unavailable. Other compiler or input errors remain failures; the report does not invent a schema for code that could not be resolved.

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

### Import a complete history

Use import when bringing an existing history and its ledger into a package.
Every imported history must start at V1 and contain every version through its
current version. The source declarations must reconstruct those same versions.

Import requires a complete ledger in the current format. It cannot change or
remove entries already frozen in the destination ledger, and it cannot contain
reset reservations.

The source and imported ledger must agree:

- The ledger contains every declared stable name and version, starting at V1.
- The field boundaries describe the latest version, or V1 when there are none.
- Every generated schema matches its imported entry.

With the declarations and `imported.json` prepared, import them together:

```sh
cargo type-history import --package invoice-history \
  --from imported.json
```

### Failed commands and recovery

Before `freeze`, `reset`, or `import` replaces a ledger, it validates the proposed contents against a captured copy of the package. A package lock prevents another command from writing the same ledger at the same time. The command also checks that its inputs have not changed, then reads back the result before releasing the lock.

The lock must be a regular file; a symlink is rejected without opening its target.
The captured ledger must match the ledger read under that lock. Commands that
leave the ledger unchanged also verify that this original authority is still
current before reporting success.

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
| Release family | Rejected |
| Custom profile inheriting `dev` | Development rules |
| Custom profile inheriting `release` | Release rules |

`TYPE_HISTORY_REQUIRE_FROZEN` must be unset or exactly `1`.
Empty strings, `0`, and `true` fail. This variable cannot relax release checks.
Custom profiles follow their inherited family, regardless of optimization or `debug_assertions`.

### When checks run

Cargo builds check source and configuration against the committed ledger.
Changes to the ledger, relevant modules, manifests, or strict settings rerun the
checks. Compiler assertions also check changed nested dependencies when build
output is cached. Builds do not freeze, repair, or rewrite the ledger.

### Package configuration

- A consumer needs a library target, the `build.rs` hook, and an initialized ledger.
  Depending on the runtime alone does not enable ledger checks.
- The ledger lives at `type-history/schemas.json`; its location is fixed.
- No `links` value, manifest metadata, or second declaration file is needed.
- `check` without `--package` selects workspace libraries containing the ledger.
  Use `check --package NAME` to check a specific library, including a missing-ledger failure.

### Variables used by tooling

The build hook sets `TYPE_HISTORY_LEDGER_PATH`, `TYPE_HISTORY_SCHEMA_ID_PREFIX`, and
`TYPE_HISTORY_AUTHORITY_KIND`. Do not set them yourself.

`TYPE_HISTORY_SCHEMA_EXPORT` is used by export tooling.
It must be unset or `1` and cannot relax release or explicit strict checks.
Lifecycle export uses a captured copy of development sources. Forced Cargo
environment settings remain effective; conflicting forced settings fail.
