# Changelog

## 0.3.1

- Reuse compiled dependencies between lifecycle commands through a locked `type-history-snapshot` directory in Cargo's target directory. Every command still copies and rebuilds local packages; a busy directory, a missing target directory, or `TYPE_HISTORY_PRIVATE_SNAPSHOT=1` selects a private temporary snapshot.
- Prepare all six crates at 0.3.1 with matching installation instructions.

## 0.3.0

- Share historical payload generation and adjacent upgrades across frontends. Frameworks can own payload traits through `GenerationOptions` and inspect typed declarations through `GeneratedVersion::contract`.
- Build frozen schema documents from resolved structural shapes. Dedicated observation builds report drift at authored source locations while ordinary, strict, release, and final candidate checks retain their validation.
- Reject malformed frontend-provided shapes, including duplicate record field names, before writing schema documents.
- Breaking framework API changes: remove `SameSchema` and compiler-marker diagnostics; add fields to `GeneratedVersion` and `Declaration`, and add `JsonSchemaErrorReason::DuplicateField`. Update custom frontends for the V2 shape export. See [framework integration](book/src/crates.md#framework-integration) for the supported APIs. Frozen ledgers and stored payload formats remain unchanged.
- Reuse standalone test setup across overlapping fixtures while preserving private consumers, executable copies, and independent source installation checks.
- Prepare all six crates at 0.3.0 with matching installation instructions.

## 0.2.0

- Add stable field contracts for string-keyed maps, declared-membership sets, tuples through arity 16, supporting tuple structs, and owned wrappers.
- Add optional finite-float, UUID, exact-decimal, Chrono, Time, `Rc`, and `Arc` integrations with checked conversions and owned encodings.
- Add per-history `derive_debug` and `derive_partial_eq` options. Both default to enabled and can be disabled for long tuple fields.
- Compare field presence, profiles, tuple structure, and set membership during frozen compatibility checks. Incompatible changes require explicit migrations.
- Point unsupported persisted field errors at the field type and explain the missing schema support without suggesting private trait implementations.
- Allow public supporting tuple structs to contain private field types without exposing them through generated schema types. Preserve nullable-newtype and nested-option checks.
- Capture lifecycle inputs by package, including declared extra inputs and symlink routes, and release inherited file locks when commands exit.
- Preserve lifecycle operations for explicit workspace members outside the workspace directory.
- Publish all six crates together at 0.2.0 and document matching registry dependencies.
- Breaking API changes for framework and macro integrations: `RecordInput` requires `derives`, `json_schema::record` accepts `Vec<SchemaField>` with explicit presence, and public schema enums and constant fields carry the extended storage contracts.

## 0.1.0

- Enable crates.io publication for the six Type History crates and include both license texts in each package.
- Add crates.io installation instructions while retaining the source checkout setup.
- Add the missing dependency entry in the fuzz workspace lockfile so locked checks can run.
- Document fixed-array support as lengths 0–32 for the current codecs.
- Document local API-reference generation.
