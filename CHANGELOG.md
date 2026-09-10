# Changelog

## Unreleased

- Name each type's history with `stable_name` in declarations, generated APIs,
  and serialized `Versioned<T>` values. All values and versions share that name;
  builds reject changes after freezing.
- Serialize `Versioned<T>` with named `stable_name`, `version`, and `payload`
  fields. Accept fields in any order, require a positive numeric version, and
  check historical payload fields before conversion. Preserve 128-bit integers
  and document JSON's reserved numeric-marker parsing limitation.
- Keep `from_versioned` as the single API for converting a `Versioned<T>` into
  the current type. Remove `Versioned::into_current` and explain the wrapper
  before the README's deserialization and conversion examples.
- Rename the field-history attribute `added_at` to `added_in`.
  Frozen schemas remain unchanged.
- Show the generated structs and conversion before introducing the current
  type alias in the README overview.
- Explain the README's struct and field attributes in the order they appear.
- State explicitly that builds reject changes to a frozen `stable_name`.
- Clarify how numbered structs relate to the current type alias, with a
  generated alias example in the README and matching API documentation.
- Accept simple stable names such as `receipt_created` and optional namespaces
  such as `shop.receipt`. Remove the minimum segment count and its
  `InvalidSegmentCount` error variant; dotted names remain valid.
- Rename the packages to `type-history` and `type-history-*`, with
  `type_history` imports and the `cargo type-history` command.
  Configuration now uses `type-history.toml`, `type-history/schemas.json`, and
  `TYPE_HISTORY_*` build settings. Frozen schema identities stay unchanged.
- Name receipt event examples `ReceiptCreated` and `ReceiptRecorded` to match
  their stable names.
- Rename the struct macro to `versioned`; field attributes remain `history`.
  Use explicit imports in the README, guides, and reference application.
- Put a receipt example before setup and add tested field, decoding, and redaction
  snippets throughout the README.
- Document crates.io setup and separate the introduction, workflow, and field reference.
- Add `stable_name` declarations checked against the frozen history ledger.
- Add `Versioned<T>` with generated `into_versioned` and `from_versioned`
  methods on the current alias. Stable name, version, and data travel together;
  deserialization selects the historical type before conversion to the current
  type. Support Serde formats with named maps, including JSON and MessagePack.
  Existing raw JSON APIs and payload schemas remain available unchanged.

- Replace shell verification runners with Cargo integration tests that compile
  the README examples. Add a runnable invoice reference crate with a committed
  frozen ledger, and run Cargo checks directly in CI.

- Place each crate directly under `crates/`, using its package name. Move shared
  documentation and examples to root-level `docs/` and `examples/` directories.

- Add a default-disabled `redactable` feature across all six Type History crates,
  using Redactable 0.12.0. Each history selects its contract with `redacted`.
  Ordinary histories retain their existing traits and error sources when the
  feature is enabled elsewhere in the dependency graph.
- Add field policies, nested record support, fixed masked `Debug`, and captured
  safe callback errors for opted histories. Payload serialization and frozen
  wire schemas remain independent of redaction policies.
- Document custom frontend error projection through `GenerationPaths.error`.
  A standalone consumer exercises the interface with structured errors.
- Extend repository checks to both feature modes, including external consumers,
  documentation examples, doctests, and release builds.
