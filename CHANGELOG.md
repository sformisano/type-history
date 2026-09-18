# Changelog

## Unreleased

- Add stable field contracts for string-keyed maps, declared-membership sets, tuples through arity 16, supporting tuple structs, and owned wrappers.
- Add optional finite-float, UUID, exact-decimal, Chrono, Time, `Rc`, and `Arc` integrations with checked conversions and owned encodings.
- Add per-history `derive_debug` and `derive_partial_eq` options. Both default to enabled and can be disabled for long tuple fields.
- Compare field presence, profiles, tuple structure, and set membership during frozen compatibility checks. Incompatible changes require explicit migrations.
- Point unsupported persisted field errors at the field type and explain the missing schema support without suggesting private trait implementations.

## 0.1.0

- Enable crates.io publication for the six Type History crates and include both license texts in each package.
- Add crates.io installation instructions while retaining the source checkout setup.
- Add the missing dependency entry in the fuzz workspace lockfile so locked checks can run.
- Document fixed-array support as lengths 0–32 for the current codecs.
- Document local API-reference generation.
