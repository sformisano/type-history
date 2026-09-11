# Changelog

## Unreleased

- Provide six crates for the runtime, schema model, code generation, procedural
  macros, build integration, and `cargo type-history` commands. Keep runnable
  examples under `examples/`.
- Generate numbered structs and adjacent conversions from `#[versioned]` and
  field `#[history]` attributes. The authored struct name always aliases the
  latest type. Support additions, type changes, renames, and removals.
- Identify each history with a frozen `stable_name`. Accept lowercase letters,
  digits, and underscores, with optional dot-separated namespaces.
- Keep the stable name, numeric version, and payload together in `Versioned<T>`.
  Deserialize the matching historical type, then use `from_versioned` to apply
  conversions until the value reaches the current type. Support Serde formats
  with named maps, including JSON and MessagePack, and preserve 128-bit integers.
  Document JSON's reserved numeric-marker parsing limitation.
- Provide lower-level JSON decoding when applications manage version metadata
  themselves. Both decoding APIs require named fields in nested records and
  reject positional records before conversion.
- Track drafts and freeze supported versions in a committed schema ledger.
  Build checks protect frozen wire schemas and stable names. CLI checks and
  unchanged freezes also validate ordinary library builds before reporting success.
- Keep generated checks and type references correct when callers shadow Rust
  built-ins, use absolute macro paths or unrelated conditional imports, or reference
  declaration-scope `Self` in field types.
- Validate candidate ledgers in owned snapshots of the local Cargo graph.
  Preserve symlinked Cargo configuration and absolute Cargo paths, and support
  workspace-local temporary directories without capturing the snapshot itself.
- Offer an optional `redactable` feature across all six crates. Opted histories
  support field policies, nested records, masked `Debug`, and safe callback errors.
  Ordinary histories retain their traits and error sources when the feature is
  enabled elsewhere. Redaction policies do not change payloads or frozen schemas.
- Support custom frontend error projection through `GenerationPaths.error`,
  exercised by a standalone consumer with structured errors.
- Follow a receipt from creation through freezing, changes, and decoding in the
  README, with generated types, ledger entries, compiler errors, and serialized
  values. Provide an mdBook for concepts, tasks, and reference, plus a GitHub Pages
  artifact build. Rust documentation exposes the current generated type's fields,
  methods, and traits; docs.rs builds include the optional redaction API.
- Validate README and book examples through Cargo integration tests, including
  their generated ledger entries, compiler errors, and serialized values. Include a
  runnable invoice example with a frozen ledger, and check external consumers,
  doctests, and release builds with and without optional redaction support.
