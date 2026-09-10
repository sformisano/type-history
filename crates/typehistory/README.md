# Typehistory

Typehistory keeps older JSON records readable as Rust fields change. Declare each
field's history once. It generates historical types, adjacent conversions, and
the current type. Normal Cargo builds protect frozen wire shapes.

The crates live in this repository and remain unpublished. Use a local checkout
or an exact Git revision. Standalone packages need no Axis crates or database.

## Table of contents

- [Start an empty package](#start-an-empty-package)
- [What the macro generates](#what-the-macro-generates)
- [Evolve an invoice through V1, V2, and V3](#evolve-an-invoice-through-v1-v2-and-v3)
- [Field history reference](#field-history-reference)
- [Rejected declarations](#rejected-declarations)
- [Decode stored records and handle failures](#decode-stored-records-and-handle-failures)
- [Frozen shapes and business meaning](#frozen-shapes-and-business-meaning)
- [Lifecycle commands](#lifecycle-commands)
- [Build profiles and environment](#build-profiles-and-environment)
- [Modules, dependency names, and nested values](#modules-dependency-names-and-nested-values)
- [Supported scope](#supported-scope)
- [Axis integration](#axis-integration)

The [executable guide](docs/guide.md) follows these commands in a disposable
consumer. The [evolution matrix](docs/evolution-matrix.md) lists supported and
rejected combinations. Complete version files live in [the invoice example](examples/invoice/README.md).

## Start an empty package

Use the repository's pinned Rust 1.97.1 toolchain. Set `TYPEHISTORY_CHECKOUT` to
an absolute path containing this checkout. Run this setup outside its workspace:

```sh
export RUSTUP_TOOLCHAIN=1.97.1
export TYPEHISTORY_CHECKOUT=/absolute/path/to/typehistory
cargo install --path "$TYPEHISTORY_CHECKOUT/crates/typehistory/cargo-typehistory" --locked
cargo new --lib invoice-history --edition 2024
cd invoice-history
```

Replace `Cargo.toml` with this manifest. Replace both checkout paths with your
actual absolute path; Cargo does not expand environment variables inside TOML.

```toml
[package]
name = "invoice-history"
version = "0.0.0"
edition = "2024"
publish = false
build = "build.rs"

[workspace]

[dependencies]
typehistory = { path = "/absolute/path/to/typehistory/crates/typehistory/typehistory" }
redactable = { version = "=0.12.0", features = ["json"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0.149"

[build-dependencies]
typehistory-build = { path = "/absolute/path/to/typehistory/crates/typehistory/build" }
```

The example uses Redactable for safe conversion errors and Serde JSON for tests.
Generated records address their own supporting libraries through Typehistory.
If authored nested types use Schemars directly, preserve `schemars = "=1.2.2"`.

For Git dependencies, replace both path dependencies with the following entries.
Replace `FULL_COMMIT_ID` with one full revision containing these unpublished crates:

```toml
[dependencies]
typehistory = { git = "ssh://git@github.com/sformisano/typehistory.git", rev = "FULL_COMMIT_ID" }
redactable = { version = "=0.12.0", features = ["json"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0.149"

[build-dependencies]
typehistory-build = { git = "ssh://git@github.com/sformisano/typehistory.git", rev = "FULL_COMMIT_ID" }
```

Install the CLI from that same revision:

```sh
cargo install --git ssh://git@github.com/sformisano/typehistory.git \
  --rev FULL_COMMIT_ID --locked cargo-typehistory
```

Create `typehistory.toml`:

```toml
format = 1
```

Create `build.rs`:

```rust
fn main() {
    typehistory_build::compile();
}
```

The package requires a library target. No `links` value, manifest metadata, or
second declaration file is needed. Configuration accepts only `format = 1`.
The fixed ledger location is `typehistory/schemas.json`.

Empty `src/lib.rs` before initialization. Resolve and fetch dependencies first:

```sh
cargo generate-lockfile
cargo fetch --locked
cargo typehistory init --package invoice-history
```

`init` creates an empty ledger. It refuses existing authority or an already
declared history. Ordinary builds require this file, even with an empty library.
Lifecycle commands use `--locked --offline`; refresh dependencies deliberately
after manifest changes. Add this line to `.gitignore`:

```gitignore
**/typehistory/.schemas.lock
```

Commit the manifest, lockfile, configuration, build hook, source, and ledger.
The lock file is disposable process coordination; the JSON ledger is authority.

## What the macro generates

Put `#[typehistory::history(...)]` above an ordinary named struct. The attribute
replaces that inventory with `InvoiceV1`, later retained versions, and the
`Invoice` alias. Removed fields remain in older structs and disappear from newer ones.

| Generated item | Contract |
| --- | --- |
| `InvoiceV1`, `InvoiceV2`, and later types | One concrete record for each authorized retained version |
| `Invoice` | Alias of the latest generated type, including a permitted draft |
| Record traits | `Clone`, `PartialEq`, strict Serde codecs, redacted `Debug` and `ToRedacted`, and structural schema contracts |
| `HasHistory` on the current type | Stable `TYPE`, positive `VERSION`, and `history()` |
| `History<Invoice>` | Exact retained-version selection followed by adjacent conversions |

Do not add derives to the outer history declaration. Typehistory owns those
implementations. Documentation and `allow`, `warn`, and `deny` attributes are
supported. Lint attributes follow Rust's usual scope; field naming lints belong
on the record. Keep transforming attributes off the declaration.

The identity must have at least three dot-separated segments and at most 255
ASCII bytes. Each segment starts with a lowercase letter. Remaining characters
are lowercase letters, digits, or underscores. `billing.invoice.issued` is valid.
Changing its Rust module or type name does not change that explicit identity.

## Evolve an invoice through V1, V2, and V3

### V1: retain the original fields

Write this complete `src/lib.rs` after initialization:

<!-- example:versions/v1.rs -->
```rust
#[typehistory::history(id = "billing.invoice.issued")]
pub struct Invoice {
    #[sensitive(redactable::Secret)]
    pub legacy: String,
    #[not_sensitive]
    pub count: u32,
}
```

The current alias, `Invoice`, points to `InvoiceV1`. The first development build
warns about its draft. Freeze V1 before introducing V2:

```sh
cargo build --locked
cargo typehistory freeze --package invoice-history
```

### V2: remove, convert, and add fields

Replace `src/lib.rs` with this complete source:

<!-- example:versions/v2.rs -->
```rust
use redactable::{BypassTextRedaction, RedactedValue, ToRedacted};

#[derive(Debug)]
pub struct ConvertError;

impl ToRedacted for ConvertError {
    fn to_redacted(&self) -> RedactedValue {
        BypassTextRedaction("safe conversion failure".to_owned()).to_redacted()
    }
}

#[typehistory::history(id = "billing.invoice.issued")]
pub struct Invoice {
    #[sensitive(redactable::Secret)]
    #[history(removed_at = v2)]
    pub legacy: String,
    #[not_sensitive]
    #[history(updated_at = v2, previous_type = u32, cast_from_previous_value = widen)]
    pub count: u64,
    #[sensitive(redactable::Secret)]
    #[history(added_at = v2, cast_from_previous_payload = label)]
    pub label: String,
    #[not_sensitive]
    #[history(added_at = v2, backfill_value = 7_u32)]
    pub revision: u32,
}

fn widen(value: u32) -> Result<u64, ConvertError> {
    Ok(u64::from(value))
}

fn label(previous: &InvoiceV1) -> Result<String, ConvertError> {
    Ok(previous.legacy.clone())
}
```

`label` borrows the whole V1 record before `widen` consumes its count. The
removed legacy field remains available during that borrow. The explicit `7`
records this example's chosen historical revision; Typehistory does not invent it.

```sh
cargo build --locked
cargo typehistory freeze --package invoice-history \
  --type billing.invoice.issued --version 2
```

### V3: keep both adjacent conversions

Replace `src/lib.rs` with this complete source:

<!-- example:versions/v3.rs -->
```rust
use redactable::{BypassTextRedaction, RedactedValue, ToRedacted};

#[derive(Debug)]
pub struct ConvertError;

impl ToRedacted for ConvertError {
    fn to_redacted(&self) -> RedactedValue {
        BypassTextRedaction("safe conversion failure".to_owned()).to_redacted()
    }
}

#[typehistory::history(id = "billing.invoice.issued")]
pub struct Invoice {
    #[sensitive(redactable::Secret)]
    #[history(removed_at = v2)]
    pub legacy: String,
    #[not_sensitive]
    #[history(updated_at = v3, previous_type = u64, cast_from_previous_value = stringify)]
    #[history(updated_at = v2, previous_type = u32, cast_from_previous_value = widen)]
    pub count: String,
    #[sensitive(redactable::Secret)]
    #[history(added_at = v2, cast_from_previous_payload = label)]
    pub label: String,
    #[not_sensitive]
    #[history(added_at = v2, backfill_value = 7_u32)]
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
        Err(ConvertError)
    } else {
        Ok(value.to_string())
    }
}
```

The newest update appears first. V1 count is `u32`, V2 count is `u64`, and V3
count is `String`. The example rejects 13 to demonstrate a fallible second step.
Add the following complete test module to that file:

<!-- example:runtime-tests.rs -->
```rust
#[cfg(test)]
mod tests {
    use super::{Invoice, InvoiceV3};
    use redactable::ToRedacted;
    use typehistory::{decode, DecodeFailureKind, HasHistory, PayloadVersion, ReadError};

    #[test]
    fn old_bytes_reach_the_current_type() {
        let value = decode::<Invoice>(
            &Invoice::TYPE,
            PayloadVersion::INITIAL,
            br#"{"legacy":"PAYLOAD_SECRET","count":42}"#,
        )
        .unwrap();
        assert_eq!(value.count, "42");
        assert_eq!(value.label, "PAYLOAD_SECRET");
        assert_eq!(value.revision, 7);
        assert_eq!(Invoice::VERSION.get(), 3);
        let _: Invoice = InvoiceV3 {
            count: "42".to_owned(),
            label: "current".to_owned(),
            revision: 7,
        };
        assert!(!format!("{value:?}").contains("PAYLOAD_SECRET"));
        assert!(!value.to_redacted().text().contains("PAYLOAD_SECRET"));
    }

    #[test]
    fn failure_keeps_the_original_version_and_failing_step() {
        let failure = decode::<Invoice>(
            &Invoice::TYPE,
            PayloadVersion::INITIAL,
            br#"{"legacy":"PAYLOAD_SECRET","count":13}"#,
        )
        .unwrap_err();
        let ReadError::Decode(error) = &failure else {
            panic!("expected a conversion failure");
        };
        assert_eq!(error.source_version().get(), 1);
        assert_eq!(error.adjacent_from().unwrap().get(), 2);
        assert_eq!(error.adjacent_to().unwrap().get(), 3);
        assert_eq!(error.kind(), DecodeFailureKind::Upcast);
        assert!(failure.to_string().contains("safe conversion failure"));
        assert!(!serde_json::to_string(&failure).unwrap().contains("PAYLOAD_SECRET"));
    }
}
```

Run the tests while V3 remains a draft, then freeze it:

```sh
cargo test --lib --locked
cargo typehistory freeze --package invoice-history \
  --type billing.invoice.issued --version 3
cargo build --release --locked
```

## Field history reference

An unannotated field exists at the retained floor, normally V1. The current
version is the maximum of that floor and every field boundary. Write versions
as `v1`, `v2`, and so on; `v0` and overflowing values fail.

| Attribute key | Placement and meaning |
| --- | --- |
| `id = "billing.invoice.issued"` | Required outer attribute identity; stable across Rust renames |
| `retained_from = vN` | Optional outer imported floor; requires matching ledger and explicit import authority |
| `added_at = vN` | Field first exists at N; choose one birth assignment |
| `updated_at = vN` | Field changes at N; declare its previous type and one conversion |
| `removed_at = vN` | Field disappears at N; retain the source field for older versions |
| `previous_type = Old` | Required on an update; type immediately before that boundary |
| `backfill_value = expression` | Addition only; expression returns the reconstructed birth type |
| `cast_from_previous_value = path` | Update only; synchronous function consumes the previous field |
| `cast_from_previous_payload = path` | Addition or update; synchronous function borrows the complete previous payload |

| Boundary | Allowed assignments | Required callback signature |
| --- | --- | --- |
| Addition | One backfill value or one payload callback | `fn(&InvoiceVn) -> Result<BirthType, E>` for a callback |
| Update | One value callback or one payload callback, plus `previous_type` | `fn(PreviousType) -> Result<Destination, E>` or `fn(&InvoiceVn) -> Result<Destination, E>` |
| Removal | None | None; predecessor remains readable by payload callbacks |

`E` must implement `Debug + ToRedacted + Send + Sync + 'static` in standalone
code. Return `Ok(value)` for a conversion that cannot fail. An added `Option`
still needs an explicit `backfill_value = None` or callback. An explicit
`Type::default()` is an author choice. Current producers supply all current fields.

Each field has one lifetime: at most one birth, any valid updates, and at most
one removal. Put its records newest first. Boundaries on one field are distinct,
above its floor, after birth, and before removal. A removed name stays reserved.

An addition followed by updates initializes the earliest reconstructed type.
The next update's `previous_type` defines that birth type. Each later callback
receives the exact adjacent predecessor, even when a field spans several versions.

Rename a field by removing its old name and adding a distinct new name at the
same boundary. A payload callback reads the old name. Merge and split fields
through separate destination callbacks borrowing the same intact predecessor.
Callbacks cannot read partially computed output. All payload callbacks finish
before any predecessor fields move. Helpers may clone values explicitly.

Same-type updates are supported with an explicit previous type and callback.
Retained intermediate versions can carry fields unchanged. There is no separate
current-version header for appending a boundary-free version. The last boundary
must still express the retained head.

## Rejected declarations

These are deliberate edits to the complete V2 source above. Each fails an
ordinary build. Diagnostic excerpts come from the shared parser or Rust compiler.
The [executable example](examples/invoice/README.md) exercises these edits.

| Rejected edit | Diagnostic excerpt | Correction |
| --- | --- | --- |
| Remove `backfill_value = 7_u32` | `an added field requires` | Restore a birth expression or payload callback |
| Replace `previous_type = u32` with `from = u32` | `unknown history key` | Use `previous_type` |
| Remove `cast_from_previous_value = widen` | `an updated field requires` | Supply one conversion |
| Add a payload callback beside the backfill | `exactly one assignment rule` | Keep one producer |
| Change `widen(value: u32)` to `widen(value: u16)` | `mismatched types` | Accept the exact previous field type |
| Return `Result<String, ConvertError>` from `widen` | `mismatched types` | Return the V2 destination type, `u64` |
| Change `label` to accept `&InvoiceV2` | `mismatched types` | Borrow the adjacent previous payload, `&InvoiceV1` |
| Set `updated_at = v0` | `history versions are positive` | Use the next authorized positive version |
| Set `updated_at = v4294967295` above frozen V1 | `exceeds the one permitted successor V2` | Append only the authorized successor |

For example, this deliberately rejected field lacks its historical initializer:

```text
#[history(added_at = v2)]
pub revision: u32,
```

Its complete correction is the V2 field with `backfill_value = 7_u32` above.
Wrong output types, unredactable errors, async functions, capturing closures,
duplicate keys, and conflicting chronology also fail. The
[matrix](docs/evolution-matrix.md) covers their full combinations.

## Decode stored records and handle failures

Store the stable identity, positive payload version, and raw JSON bytes together.
Validate dynamic identities with `TypeIdentity::try_from(String)`. Validate raw
versions with `PayloadVersion::try_from_raw(u32)`.

`decode::<Invoice>(&identity, version, bytes)` checks identity first. It selects
the exact historical type before reading JSON, then applies every adjacent step.
It never guesses a version from fields or deserializes old bytes as current first.
`Invoice::history().retained_versions()` lists accepted versions. The descriptor's
`decode` method assumes the caller has already selected the correct identity.

Unknown versions fail before JSON parsing. Generated record codecs reject
sequences, unknown fields, duplicate fields, malformed JSON, and missing required
fields. An absent supported `Option` field may decode as `None`.

`ReadError` distinguishes an identity mismatch from `DecodeError`. Conversion
errors keep the original stored version and the failing adjacent pair. A failure
at V2 to V3 while reading V1 therefore reports source V1 and adjacent V2/V3.

`Debug`, `Display`, Serde serialization, and `ToRedacted` expose only safe context
and the captured redacted cause. Raw decoder diagnostics and author error
formatting are discarded. The author's `ToRedacted` implementation defines the
safe cause; do not certify secret-bearing text through a bypass wrapper.

Field sensitivity uses bare `#[sensitive]` for supported scalar values,
`#[sensitive(redactable::Secret)]` for strings, or another supported policy.
Use `#[not_sensitive]` to declare a public field. Unannotated fields inherit their type's redaction contract.
One field policy must work for every retained type. Raw serialization and direct
field access retain actual values; use the redacted APIs for logging.

Runtime decoding reads no ledger, invokes no Cargo process, and compares no
schema fingerprint. It performs no reducer, state load, database write, or replay.

## Frozen shapes and business meaning

The ledger protects compiler-resolved wire shapes and the complete retained
inventory. Editing a frozen nested shape, deleting a frozen type, or losing a
retained version fails ordinary builds. Missing or malformed authority fails safely.
Duplicate JSON keys, invalid or gapped versions, and inconsistent metadata fail
ledger parsing. Version ranges are authorized before dense type generation.

Frozen checks do not freeze callback bodies, constructor semantics, or
wire-equivalent nominal Rust types. Reordering equivalent fields can pass while
changing callback order. Removing a same-shape semantic step can pass if remaining
boundaries still express every retained version. Test conversion meaning against
representative stored data. Keep conversions deterministic and free of I/O,
clocks, randomness, and externally visible side effects.

Changing a nested value's old wire shape requires preserving its old definition
and adding a new field update. A schema match cannot prove a business rule.
Typehistory supplies no state versioning, downcasting, automatic defaults, or
recursive business validation.

## Lifecycle commands

Run commands from the consumer workspace. Every mutation requires one exact
`--package NAME`. `--type EXACT_ID` and `--version N` must appear together.
Wildcards are rejected. Optional `--features LIST` and `--target TRIPLE` select
compiler inputs; custom target files are unsupported.

| Command | Effect |
| --- | --- |
| `cargo typehistory init --package invoice-history` | Create authority before any history declaration exists |
| `cargo typehistory check` | Check configured workspace libraries without writing package files |
| `cargo typehistory check --package invoice-history` | Check one configured library |
| `cargo typehistory freeze --package invoice-history` | Freeze all ordinary drafts after validating the complete package |
| `cargo typehistory freeze --package invoice-history --type billing.invoice.issued --version 3` | Freeze exactly one ordinary draft or reset reservation |
| `cargo typehistory reset --package invoice-history --type billing.invoice.issued --version 3` | Reserve the highest frozen version for experimental replacement |
| `cargo typehistory reset --undo --package invoice-history --type billing.invoice.issued --version 3` | Clear that reservation after source matches the saved frozen shape |
| `cargo typehistory import --package invoice-history --from imported.json --floor billing.invoice.issued=4` | Validate complete imported authority with the declared retained floor |

### Iterate and discard an ordinary draft

After V2 is frozen, edit V3 repeatedly in development. Only one ordinary latest
draft per history is permitted. Freeze it before appending V4.

To discard the example's unfrozen V3, restore the complete V2 source and its
current callers. Remove draft fields and boundaries, and restore previous types.
The alias becomes `InvoiceV2`. No ledger command is needed or provided. Removing
only the annotation while keeping `count: String` changes frozen V2 and fails.
A remaining draft boundary keeps the draft alive. Entirely draft-only types may
be deleted. Stored development bytes can become unreadable after draft edits.

### Reset and undo

WARNING: Replacing a frozen shape can make stored bytes unreadable. Use reset
only for disposable experimental data; evolve durable formats with a successor.

Reset requires the highest frozen version and no successor draft. Its ledger
entry retains the original shape with an explicit reservation. Earlier versions
stay protected. Development permits edits to the reserved version; strict builds
reject it. You cannot append a successor or erase the reservation by deleting source.

Freeze the exact reserved identity/version to accept its replacement. Package
freeze rejects any pending reservation. To undo, restore the source that produces
the saved shape, then use `reset --undo`. Undo does not edit source. Mismatched
source leaves the reservation pending.

### Import a retained floor

Import requires a complete ledger in the current format. It cannot change or
remove existing frozen authority. Each imported floor above V1 requires the
matching `--floor EXACT_ID=N`. The outer attribute must declare the same floor:

```text
#[typehistory::history(id = "billing.invoice.issued", retained_from = v4)]
```

This is an attribute fragment for a separate history genuinely beginning at V4.
It is not an edit to the retained V1-to-V3 invoice. Unannotated fields then begin
at V4; no V1-to-V3 types are invented. The import must contain every declared
identity and retained version. A floor annotation cannot authorize itself.
An imported boundary-free final version needs an expressible source head;
silently trimming real history is unsupported.

### Failed commands and recovery

Mutations take a package lock, capture compiler inputs, validate a candidate,
check source freshness, and atomically replace the ledger. They retain the lock
through readback. Busy locks, stale inputs, and failures before replacement leave
the authoritative bytes unchanged. Temporary candidates are disposable.

A failure after replacement reports `COMMITTED`. Inspect the live ledger before
retrying that operation. It has already crossed the commit point. No operation
promises a transaction across packages or changes stored record bytes.

## Build profiles and environment

Every normal build reads source, configuration, and committed authority. Changes
to those files, relevant modules, manifests, and strict settings invalidate checks.
Builds never freeze, repair, or rewrite the ledger. Compiler assertions include
nested resolved wire types, so a cached build still checks changed dependencies.

| Consumer profile or setting | Ordinary draft or reset reservation |
| --- | --- |
| Development | Allowed with warning |
| `TYPEHISTORY_REQUIRE_FROZEN=1 cargo build` | Rejected |
| `AXIS_REQUIRE_FROZEN_EVENTS=1 cargo build` | Rejected |
| Release family | Rejected |
| Custom profile inheriting `dev` | Development rules |
| Custom profile inheriting `release` | Release rules |

Both strict variables accept only unset or exactly `1`. Empty strings, `0`, and
`true` fail, even when the other variable equals `1`. Either valid variable
strengthens both frontends. There is no permissive release override. Policy
follows the consuming profile family, independent of `debug_assertions` and optimization.

`TYPEHISTORY_SCHEMA_EXPORT` and `AXIS_SCHEMA_EXPORT` are tooling signals. They
accept only unset or `1` and cannot relax release or explicit strict checks.
Lifecycle export uses captured development sources. Forced Cargo environment
settings remain effective; conflicting forced settings fail instead of being bypassed.

Do not set `TYPEHISTORY_LEDGER_PATH`, `TYPEHISTORY_SCHEMA_ID_PREFIX`, or
`TYPEHISTORY_AUTHORITY_KIND` yourself. The build hook supplies package authority.
A package cannot configure both standalone and Axis ledgers. An Axis package may
depend on Typehistory runtime without enabling standalone authority.

## Modules, dependency names, and nested values

Use ordinary `mod` declarations and explicit macro imports. A renamed dependency
works through `package = "typehistory"`; invoke its chosen name, such as
`#[history_api::history(...)]`. A renamed build dependency likewise changes the
`build.rs` call. Generated paths follow the consumer's dependency names.
Runtime dependencies may use ordinary or target-specific dependency tables,
including renamed and workspace-inherited entries. Histories themselves remain
unconditional. Source directories may be named `target`; lifecycle snapshots
exclude Cargo's resolved output directory.

Types, callback paths, and expressions resolve at the original declaration.
Supported `self`, `super`, and `crate` paths retain that meaning. Raw identifiers
keep their Rust spelling; `r#type` has the wire key `type`. Duplicate canonical
names and collisions with generated `InvoiceVn` names fail.

Nested named records can derive `typehistory::Schema` for structural contracts.
That supporting derive creates `ResolvedSchema` and schema export support.
It supplies no Serde codec, redaction, constructor, or business validation.
Provide those ordinary traits on the nested value. Nested shapes must use the
supported closed wire representation. Pair ordinary Serde derives with
container `#[serde(deny_unknown_fields)]`, which `Schema` accepts. Other Serde
or Schemars shaping attributes are rejected. Public supporting records may keep
nested field types private. Generated history types cannot themselves be nested
fields; use separate supporting records for nested values.

This complete additional module defines a closed nested record and one independent
history. Add it to the initialized library to create a new V1 draft:

<!-- example:nested-example.rs -->
```rust
pub mod addresses {
    #[derive(
        Clone,
        Debug,
        PartialEq,
        serde::Serialize,
        serde::Deserialize,
        redactable::NotSensitive,
        typehistory::Schema,
    )]
    #[serde(deny_unknown_fields)]
    pub struct Address {
        pub number: u32,
    }

    #[typehistory::history(id = "billing.address.record")]
    pub struct AddressRecord {
        pub address: Address,
    }

    #[test]
    fn nested_values_keep_their_closed_codec() {
        use typehistory::{HasHistory, PayloadVersion};

        assert!(AddressRecord::history()
            .decode(PayloadVersion::INITIAL, br#"{"address":{"number":7}}"#)
            .is_ok());
        assert!(AddressRecord::history()
            .decode(
                PayloadVersion::INITIAL,
                br#"{"address":{"number":7,"unknown":1}}"#,
            )
            .is_err());
    }
}
```

The example certifies the address number as public through `NotSensitive`.
Choose a redacting contract for a nested value that contains private data.

## Supported scope

History declarations support concrete named records, including empty records.
The field wire contract includes `bool`, signed and unsigned 8-to-128-bit integers,
`String`, vectors, fixed
arrays, optional values, and nested resolved records. `Vec<u8>` follows the
existing byte representation; array length is part of its shape.

Floats, pointer-sized integers, enums, tuple/unit history structs, generics, recursive histories, maps, sets,
nested options, asynchronous callbacks, and whole-payload ownership callbacks
are outside this authoring API. Conditional inventories and Serde
rename/default/flatten/skip/alias/with attributes are rejected. Supported
feature-selected aliases remain subject to compiler-resolved frozen checks.

Keep historical data only for the versions you intentionally retain. Real floors
need explicit import authority. Generated code grows with the retained range;
an unauthorized huge range is rejected before allocation or expansion.

## Axis integration

Axis `aggregate!` uses this same history parser, generator, schema authority,
build checks, and lifecycle implementation. Keep the existing Axis declaration,
`schema_build::compile()` hook, `axis/event-schemas.json`, and `cargo axis` commands.
Axis derives identities and retains aggregate roles, reducers, state, and delivery.
See the [Axis integration guide](https://github.com/sformisano/axis/blob/af7b96c3f0744cdfd170c50bb321d5dfd02be8ea/docs/design/domain-authoring-macros.md#event-field-history).
