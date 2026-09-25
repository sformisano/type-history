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
- The UUID, decimal, and temporal adapters expose `TryFrom`, `as_inner`, and `into_inner`. They expose no mutable native access.
- JSON and named-field MessagePack preserve admitted float bits, decimal scale, temporal nanoseconds, and offsets.
- `derive_debug` and `derive_partial_eq` control those generated traits for every retained version. Both default to `true`.
- Lifecycle commands capture local package roots, workspace manifests, lockfiles, effective Cargo configuration, and declared snapshot inputs.
- Each lifecycle command copies every local input again. Under an exclusive lock, it builds in `type-history-snapshot` inside Cargo's existing target directory. It first removes Cargo's state for local packages, so they always rebuild from the new copy. Only dependency output is reused. Contention, a missing target directory, or an unusable `type-history-snapshot` directory selects a private temporary snapshot. `TYPE_HISTORY_PRIVATE_SNAPSHOT=1` also selects one. Any other value fails the command.
- Frozen schema documents derive from compiler-resolved structural shapes. Descriptive schema exports retain container, field, and variant documentation.
- Lifecycle commands observe source shapes in the schema export build, a dedicated development test build. They compare those shapes with the ledger. With `--released-baseline`, `check` first compares the released ledger with the package's ledger.
- JSON reports identify observed source differences as `current_ledger` and include discovered source locations when available.

## Repository Layout

- `crates/core/type-history` is the public facade and macro entry point.
- `crates/core/type-history-core` owns storage contracts, adapters, and versioned decoding.
- `crates/core/type-history-codegen` owns declaration parsing, frozen checks, diagnostics, and generated code.
- `crates/core/type-history-macros` exposes `versioned` and `Schema`.
- `crates/core/type-history-build` owns the build hook, lifecycle operations, snapshot capture and reuse, and JSON check reports.
- `crates/core/cargo-type-history` provides the `cargo type-history` command and standalone consumer coverage.
- `book/src/integration.md` defines supported field contracts and codec limits.
- `book/src/evolution-matrix.md` records supported and rejected evolution cases.

## Constraints and Invariants

- An unchanged version must keep the complete storage contract.
- The contract includes representation, admitted domain, field presence, profile, set membership, tuple order, and tuple arity.
- Only an exact `reset`, and the `freeze` or `reset --undo` that closes it, may change a frozen ledger entry. Other commands and new Type History releases must leave existing entries unchanged. In a ledger written by Type History, they stay byte-for-byte identical.
- Ordinary builds enforce frozen shapes. Release and explicit strict builds reject the schema export build.
- After a clean comparison, `check` also runs `cargo check` on the lifecycle snapshot. Every command that writes the ledger runs that `cargo check` before its atomic write.
- Retained payloads must remain readable. Frozen checks protect their structure. Conversion tests must protect their meaning.
- `HashMap<String, V, S>` and `BTreeMap<String, V>` are equivalent when `V` has the same contract.
- Sets remain distinct from sequences. Custom set elements must declare stable membership.
- Type History trusts membership declarations. It does not prove custom equality or scan collections for duplicates.
- Feature unification must not change adapter schemas, domains, or encodings.
- Incompatible contract changes require a new retained version and explicit conversion.
- Native dependency Serde features must not change adapter encodings.
- Generated files outside local package roots are captured only when a package declares them in `package.metadata.type-history.snapshot-inputs`.

## External Interfaces

- `#[versioned(stable_name = "...")]` declares a history.
- `derive_debug` and `derive_partial_eq` are optional boolean `versioned` arguments.
- `Schema` declares supporting field structure.
- `SetMembership` and `ConstantMembership::custom` declare custom set membership.
- `type_history::adapters` exposes checked UUID, decimal, Chrono, and Time adapters behind features.
- `typed-floats` adds direct support for `NonNaNFinite<f32>` and `NonNaNFinite<f64>`.
- `Versioned` wraps each stored payload with its stable name and version.
- `cargo type-history` initializes, checks, freezes, imports, resets, and undoes ledger changes.
- Frameworks can supply payload traits and optional source locations through the shared generator and build interfaces.
- `package.metadata.type-history.snapshot-inputs` declares relative files or directories that a package needs from outside its root.
- Declared input snapshots bind every path-resolution component and the resolved contents; adding, removing, or retargeting a symlink invalidates the snapshot even when the resolved bytes are unchanged.
- A relative source symlink is rejected when its unchanged target would escape the lifecycle snapshot.

## Known Gaps / Deferred

- History roots cannot be enums, tuple structs, unit structs, generic, or recursive.
- Non-string map keys, unit values, empty tuple structs, raw floats, and non-finite floats are unsupported.
- Borrowed fields, recursive graphs, cycles, weak pointers, locks, and atomics are unsupported.
- `Rc` and `Arc` allocation identity and sharing are not preserved.
- Arbitrary-precision decimals, named timezones, epoch-unit profiles, and arbitrary Serde overrides are unsupported.
- Codec guarantees cover JSON and named-field MessagePack. Map byte order is unspecified.
