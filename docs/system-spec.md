# System Specification

## Domain Overview

- Type History generates retained Rust record versions from one named-field struct declaration.
- A stable name identifies one history across Rust type and module renames.
- A frozen ledger records each retained version's storage contract.
- Explicit adjacent conversions upgrade retained values to the current record.

## Current Observable Behaviors

- Histories use concrete named-field structs. Supporting records, enums, and nonempty tuple structs can be fields.
- Supported containers include vectors, fixed arrays, options, string-keyed maps, declared-membership sets, and bare tuples with 1–16 items.
- `Box` persists its inner value. `Rc` and `Arc` do the same when the `rc` feature is enabled.
- Optional adapters provide finite floats, UUID text, exact decimal text, and five shared temporal profiles.
- Checked adapters expose `TryFrom`, `as_inner`, and `into_inner`. They expose no mutable native access.
- JSON and named-field MessagePack preserve admitted float bits, decimal scale, temporal nanoseconds, and offsets.
- `derive_debug` and `derive_partial_eq` control those generated traits for every retained version. Both default to `true`.

## Repository Layout

- `crates/core/type-history` is the public facade and macro entry point.
- `crates/core/type-history-core` owns storage contracts, adapters, and versioned decoding.
- `crates/core/type-history-codegen` owns declaration parsing, frozen checks, diagnostics, and generated code.
- `crates/core/type-history-macros` exposes `versioned` and `Schema`.
- `crates/core/cargo-type-history` owns ledger lifecycle commands and standalone consumer coverage.
- `book/src/integration.md` defines supported field contracts and codec limits.
- `book/src/evolution-matrix.md` records supported and rejected evolution cases.

## Constraints and Invariants

- An unchanged version must keep the complete storage contract.
- The contract includes representation, admitted domain, field presence, profile, set membership, tuple order, and tuple arity.
- Existing frozen ledger bytes must remain unchanged.
- Retained payloads must remain readable with their original meaning.
- `HashMap<String, V, S>` and `BTreeMap<String, V>` are equivalent when `V` has the same contract.
- Sets remain distinct from sequences. Custom set elements must declare stable membership.
- Type History trusts membership declarations. It does not prove custom equality or scan collections for duplicates.
- Feature unification must not change adapter schemas, domains, or encodings.
- Incompatible contract changes require a new retained version and explicit conversion.
- Native dependency Serde features must not change adapter encodings.

## External Interfaces

- `#[versioned(stable_name = "...")]` declares a history.
- `derive_debug` and `derive_partial_eq` are optional boolean `versioned` arguments.
- `Schema` declares supporting field structure.
- `SetMembership` and `ConstantMembership::custom` declare custom set membership.
- `type_history::adapters` exposes checked UUID, decimal, Chrono, and Time wrappers behind features.
- `typed-floats` adds direct support for `NonNaNFinite<f32>` and `NonNaNFinite<f64>`.
- `Versioned` reads and writes generated history envelopes.
- `cargo type-history` initializes, checks, freezes, imports, resets, and undoes ledger changes.

## Known Gaps / Deferred

- History roots cannot be enums, tuple structs, unit structs, generic, or recursive.
- Non-string map keys, unit values, empty tuple structs, raw floats, and non-finite floats are unsupported.
- Borrowed fields, recursive graphs, cycles, weak pointers, locks, and atomics are unsupported.
- `Rc` and `Arc` allocation identity and sharing are not preserved.
- Arbitrary-precision decimals, named timezones, epoch-unit profiles, and arbitrary Serde overrides are unsupported.
- Codec guarantees cover JSON and named-field MessagePack. Map byte order is unspecified.
