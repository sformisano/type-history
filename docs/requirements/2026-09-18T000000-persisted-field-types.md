---
type: Requirement
status: Active
priority: P1
priority_rationale: Broader persisted field support must detect substitutions that lose stored values.
date: 2026-09-18
project_slug: type-history
---
# Support common Rust field types through stable storage contracts

## Problem

Type History currently rejects common persisted field types. Identical serialized shapes do not prove that two Rust types preserve the same values: changing a vector to a set can discard repetitions, and changing set equality can collapse distinct members. Native dependency serializers can also change encoding when Cargo features unify.

## Goal

Common owned field types work within named-record histories. Unchanged history versions require exact declared storage-contract equivalence in JSON and named-field MessagePack. Incompatible substitutions require explicit versioned changes and migration.

## Authority

The user's discussion accepts named-record histories, finite-only floats, distinct sequence/set contracts, equivalent map/set implementations only when data compatible, and breaking library changes. The user requested research, then explicitly instructed use of rust-implementation-workflow and subagents to implement that direction. The accepted research is type-support-direction.md from the parent task. This note freezes that direction; names and crate packaging are delegated implementation choices. No publishing or commit authority is included.

## Scope

### In Scope

Maps, sets and membership contracts, tuples and supporting tuple structs, owned transparent wrappers, finite numeric values, checked UUID/decimal/calendar adapters, both supported codecs, schema freezing/comparison/diagnostics, optional additive Cargo integrations, consumer proof and current documentation.

### Out of Scope

The separate unsupported-field diagnostic fix (already on the base); non-named history roots; non-string map keys; non-finite floats; unit types and empty tuple structs; arbitrary-precision decimals; named timezones and epoch-unit profiles; recursive graphs, borrowed fields, weak pointers, locks and atomics; database scans or business validation; a general widening/narrowing solver; publishing.

## Non-Negotiable Requirements

### NNG-01: Existing stored contracts remain readable

Existing frozen schemas, fingerprints, historical JSON and named-field MessagePack fixtures remain unchanged and readable. Any necessary format transition must be explicit, documented, and retain old readers; silently reinterpreting a frozen document fails acceptance.

### NNG-02: Unsafe equivalence is rejected

An unchanged history version rejects a representation, admitted-domain, or declared membership change, including Vec-to-set, finite32-to-finite64, and logical-profile-to-String substitutions. Uncertain custom behavior is never automatically certified by matching field shapes.

### NNG-03: Decoding preserves admitted values

Supported codecs preserve finite float bits, decimal coefficient and scale, and temporal nanoseconds/offsets. Non-finite or out-of-profile values produce checked errors rather than null, rounding, truncation, or clamping.

### NNG-04: Features do not change encodings

Enabling another additive feature or a dependency's serde feature must not change an existing adapter's schema, accepted domain, or bytes.

## Requirements

### REQ-001: Named records own histories

Only structs with named fields own version histories. Supporting enums, tuples and tuple structs are field types. Existing enum forms remain supported.

### REQ-002: String-keyed maps share contracts

HashMap<String,V,S> and BTreeMap<String,V> work recursively and are schema-equivalent for equal value contracts. Hasher and iteration order do not enter storage equivalence.

### REQ-003: Sets retain membership semantics

HashSet<T,S> and BTreeSet<T> share a set contract when their element encoding and membership contracts match. Set differs from sequence. Supported built-ins have known membership contracts. Custom elements require an explicit stable membership declaration; a schema derive alone does not certify Eq/Hash/Ord. A changed membership declaration requires a versioned migration. Type History does not scan persisted collections for duplicate values.

### REQ-004: Tuples preserve order and arity

Bare tuples of arities 1 through 16 and supporting tuple structs of at least one field work recursively. A single-field tuple struct retains Serde's transparent newtype encoding; it is not equivalent to a one-element bare tuple. Empty tuple structs and unit remain unsupported.

### REQ-005: Owned wrappers preserve values

Box<T> and optional Rc<T>/Arc<T> retain the inner storage contract by value. Pointer identity, shared allocation, and cycles are not preserved or supported.

### REQ-006: Finite floats preserve width and bits

An optional typed_floats integration supports NonNaNFinite<f32> and NonNaNFinite<f64> as numeric profiles with distinct widths. Non-finite values are rejected. Supported direct and buffered historical JSON and named-field MessagePack decoding preserves all admitted finite values, including signed zero and subnormals.

### REQ-007: UUID text has a stable domain

An optional uuid 1.x adapter writes lowercase hyphenated UUID text in both codecs, accepts all UUID bit patterns including nil and max, and validates the declared grammar. UUID text is distinct from unrestricted String and any future compact encoding.

### REQ-008: Decimal text preserves scale exactly

An optional rust_decimal 1.x adapter stores signed exact decimal text with coefficient magnitude within 96 bits and scale 0..28, preserving trailing zeros. Checked decoding rejects exponent/underscore conveniences, excess precision, and out-of-range input without rounding. Native dependency serde feature choices cannot affect the profile.

### REQ-009: Temporal profiles are shared and checked

Optional chrono 0.4 and time 0.3 adapters share five distinct profiles: date YYYY-MM-DD (proleptic Gregorian years0000..9999), local time HH:MM:SS[.fraction] (seconds00..59, up to9fraction digits), local datetime joined by T, UTC instant ending Z, and offset datetime retaining a numeric minute-aligned offset. Offset values require both local and UTC dates in range. Fractions use shortest exact nanosecond spelling. Reject leap seconds, excessive precision, second offsets and out-of-range values during checked construction or decoding. No timezone inference, clamping or hidden conversion.

### REQ-010: Schema information survives the whole pipeline

Map, set/membership, tuple, finite-width and logical-profile information survives constant and runtime shapes, JSON Schema export/normalization/parsing, frozen ledgers, comparison, fingerprints and diagnostics. Unknown profiles fail clearly. Contract identities remain independent of Rust names, module paths, dependency aliases and release numbers. Only exact contract equivalence avoids a version change.

### REQ-011: Integrations compose and remain optional

Integrations own serialization, deserialization, schema and checked conversion together. They work recursively inside supported records, enum variants, tuples, maps, sets and options. Optional additive features expose integrations without changing previously exposed contracts. Arbitrary serde(with) authoring overrides remain outside this scope.

### REQ-012: Consumers can learn and verify the guarantees

Current public documentation describes feature flags, public adapters, checked conversions, set membership trust, required migrations, field/history-root distinction and codec limits. Public consumer tests and retained fixtures demonstrate supported behavior and rejected incompatible changes.

## Acceptance Criteria

1. When maps use equal value contracts, freezing with HashMap and checking/reading with BTreeMap succeeds through both codecs.
2. When equal-contract sets change collection implementation, old values survive; Vec-to-set or changed membership fails unchanged-version validation and succeeds only through an explicit migration.
3. When tuples/newtypes/owned wrappers contain supported adapters, both codecs decode historical records and preserve tuple arity/order/newtype distinction.
4. When finite float bit patterns include both zeros, subnormals, boundaries and generated samples, direct and buffered historical decoding returns identical bits; non-finite input fails.
5. When UUID, decimal or calendar adapters round-trip under baseline and unified dependency features, bytes and schemas remain stable, including decimal trailing zeros and temporal nanoseconds/offsets.
6. When profiles receive malformed or out-of-domain values (including leap seconds and excessive decimal/fraction precision), construction or decoding fails without lossy conversion.
7. When only float width, membership, logical profile, range, or unit changes, frozen-contract checks identify incompatibility; unknown profiles fail.
8. When baseline frozen artifacts and retained historical fixtures are read at head, schemas/fingerprints and decoded values retain their baseline meaning.
9. When default, individual and combined feature consumers compile/test, documented public APIs work; existing example schema checks remain clean.
