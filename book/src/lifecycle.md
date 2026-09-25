# Freeze and manage histories

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

If a nested record or enum needs a new structure, keep its earlier definition. Add a
field update that converts the old value to the new type.

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
to disable those defaults. Both selections apply to Cargo metadata and to every build
of the lifecycle snapshot.
For a package with an alternative `backend-alt` feature:

```sh
cargo type-history check --package my-shop-demo-project --no-default-features --features backend-alt
```

Selected features must still preserve every frozen schema.

If you call the build library directly, `LifecycleOptions::no_default_features` controls the same choice. Set it to `false` to keep package defaults or `true` to disable them. `workspace::metadata(features)` keeps package defaults; `options::parse` reads the command-line selection.

The [setup guide](setup.md#3-initialize-the-schema-file) explains `init`, and the
[quick start](quick-start.md#2-generate-and-freeze-v1s-schema) shows `freeze`.
Use `check` to validate existing histories.
The command examples below use the [invoice package](guide.md).
The remaining commands handle experimental resets or imported histories.

| Command | Effect |
| --- | --- |
| `cargo type-history init --package invoice-history` | Create the ledger before any history declaration exists |
| `cargo type-history check` | Check initialized workspace libraries without writing package files |
| `cargo type-history check --package invoice-history` | Check one selected library |
| `cargo type-history check --package invoice-history --released-baseline released-schemas.json` | Also preserve every history frozen in a separately supplied [released ledger](#compare-with-a-released-ledger) |
| `cargo type-history check --format json` | Report workspace checks as one JSON document |
| `cargo type-history freeze --package invoice-history` | Freeze all [ordinary drafts](#iterate-and-discard-an-ordinary-draft) after validating the complete package |
| `cargo type-history freeze --package invoice-history --type billing.invoice.issued --version 3` | Freeze exactly one ordinary draft or [reset reservation](#reset-and-undo) |

### Iterate and discard an ordinary draft

WARNING: Editing a draft can make records written with it unreadable. Freeze a
version before storing data you must keep.

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
and adds `"reset_draft": true` to that version. This marker is a **reset reservation**.
Build messages call the version a reset draft.

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

### Compare with a released ledger

Ordinary builds compare source against the working ledger. A reset followed by a
freeze can change a released version's schema, and those builds still pass. To
protect released history in CI, obtain `type-history/schemas.json` from the release
you trust and save it as a separate file. Supply that file when checking its package:

```sh
cargo type-history check --package invoice-history --released-baseline released-schemas.json
```

The command requires one exact package and a file distinct from its working
ledger. It rejects missing released histories or versions, changed metadata or
schemas, and reset reservations for released versions. The released ledger itself
must contain only frozen versions. New histories and later versions are allowed.

The check copies both ledgers and the source. It compares the released ledger with the working ledger, then the working ledger with the compiled source. A change to a released schema therefore fails the first comparison, even when the source and working ledger are edited together. The check does not rewrite either ledger or the source, on success or failure.

Your CI job chooses the trusted released ledger. Type History does not fetch or
authenticate it. Ordinary builds and running applications need no released ledger.

### Structured diagnostics

Use `--format json` when another tool needs to read the results. The command writes one JSON document to stdout. A successful check exits with status 0; a failed check exits with status 2. Both `--format` and `--released-baseline` apply only to `check`.

Each diagnostic has a stable `code` and an `origin`. It names the affected `stable_name`, `version`, and `path`, so a change inside a nested record points to that field. It also gives the `expected` and `actual` values, a correction `hint`, and a source `location` when available. The top level has `schema_version`, `command`, `ok`, `packages`, and `errors`. Each `packages` entry has `package` and `diagnostics`. Paths distinguish fields, enum variants, tuple positions, and container elements. Missing values use JSON null. The report includes independent differences in a consistent order.

The command runs a dedicated test build, the **schema export build**, that records source shapes. It then compares them with the ledger. The schema export build permits frozen-shape drift so the report can describe each difference. Other compiler or input errors remain failures.

Observed source differences use the `current_ledger` origin, which means the working ledger. A location points to the authored containing field when discovery identifies it, or to the history declaration otherwise. Captured paths map back to the original source. [Custom frontends](crates.md) may omit locations.

Ordinary builds still reject frozen-shape drift. When the comparison finds no differences, `check` also runs `cargo check` on the lifecycle snapshot. Every command that writes the ledger runs that `cargo check` first. Release and explicit strict builds reject the schema export build.

### Copied inputs

Commands copy and fingerprint every file under each local package root and
declared input, including files in `node_modules` and `.next`. They skip only:

- The workspace's Cargo target directory.
- A `target` directory created by Cargo at the top of a package or declared input.
- Git metadata in a `.git` entry at the top of a package or declared input.
- Each package's `type-history/.schemas.lock`.

Other `target` and `.git` directories are copied.

Workspace checks select initialized packages from one shared lifecycle snapshot.
The command verifies Cargo's package graph against that snapshot before validation
and checks the original inputs again after all selected packages have been checked.
A changed package graph or input fails the command.

Commands detect changes by comparing fingerprints at specific points. They do not
lock source files against editors, and an edit after the final comparison cannot
be detected. Do not edit inputs while a command runs. If another process could
change them, use an immutable source checkout.

### Failed commands and recovery

Each `init`, `freeze`, `reset`, or `import` holds a package lock while it runs. Another of these commands for the same package fails at once with `is busy`. `check` does not take the lock. Before `freeze`, `reset`, or `import` replaces a ledger, it validates the proposed contents against its lifecycle snapshot. It also checks that its inputs have not changed, then reads back the result before releasing the lock.

The lock must be a regular file. A symlink is rejected without opening its target.
The captured ledger must match the ledger read under that lock. Commands that leave
the ledger unchanged also check, before reporting success, that the ledger still
matches what they read under the lock.

Recovery depends on whether the replacement happened:

- **Before replacement:** A busy package lock, changed input, or validation failure leaves
  the ledger unchanged. Resolve the reported cause, then run the command again.
- **After replacement:** The error includes `COMMITTED`. The command has already replaced
  the ledger. Review the ledger's diff before running another command. If it holds the
  intended change, rerunning `freeze` or `import` reports `no-op`.

An interrupted command prints no `COMMITTED` error, even when it has already replaced
the ledger. Review the ledger's diff before running another command. An interrupted
command can also leave a `type-history/.history-candidate-*` file, or a
`type-history-schema-*` directory in the system temporary directory. When no command
is running, delete them. The next command that uses `type-history-snapshot` removes
copies left there.

Each command updates one package's ledger. These commands do not modify stored record bytes.

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

Cargo builds check source and configuration against the ledger.
Changes to the ledger, relevant modules, manifests, or strict settings rerun the
checks. Compiler assertions also check changed nested dependencies when build
output is cached. Builds do not freeze, repair, or rewrite the ledger.

### Package configuration

- A consumer needs a library target, the `build.rs` hook, and an initialized ledger.
  Without the hook, history declarations fail to compile.
- The ledger lives at `type-history/schemas.json`; its location is fixed.
- No `links` value or second declaration file is needed; manifest metadata is needed only for `snapshot-inputs`.
- `check` without `--package` selects workspace libraries containing the ledger.
  Use `check --package NAME` to check a specific library, including a missing-ledger failure.

### Variables used by tooling

The build hook sets `TYPE_HISTORY_LEDGER_PATH`, `TYPE_HISTORY_SCHEMA_ID_PREFIX`,
`TYPE_HISTORY_AUTHORITY_KIND`, and `TYPE_HISTORY_ADMISSION`. Do not set them yourself.

`TYPE_HISTORY_SCHEMA_EXPORT` selects the schema export build.
It must be unset or `1`. Only a non-strict development test build can observe
changed frozen shapes. Non-test builds retain their shape assertions. Release
and explicit strict builds reject the schema export build, including when every
version is frozen.

Lifecycle commands build the lifecycle snapshot with the `dev` profile. Cargo
`[env]` entries still apply. An entry such as
`TYPE_HISTORY_REQUIRE_FROZEN = { value = "1", force = true }` makes the schema
export build fail, so `check`, `freeze`, `reset`, and `import` fail.

`TYPE_HISTORY_PRIVATE_SNAPSHOT` must be unset or `1`. With `1`, each lifecycle
command builds in a private temporary snapshot instead of reusing
`type-history-snapshot` in Cargo's target directory; see
[package setup](setup.md#3-initialize-the-schema-file). Other values fail the command.
