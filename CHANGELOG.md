# Changelog

## Unreleased

- Rename the history argument `id` to `stable_id`. Keep the same identity value
  when updating declarations; frozen identities and ledger files remain unchanged.
- Add `Versioned<T>` with generated `into_versioned` and `from_versioned`
  methods on the current alias. Identity, version, and data travel together;
  deserialization selects the historical type before conversion to the current
  type. Support Serde formats with named maps, including JSON and MessagePack.
  Existing raw JSON APIs and payload schemas remain available unchanged.

- Replace shell verification runners with Cargo integration tests that compile
  the README examples. Add a runnable invoice reference crate with a committed
  frozen ledger, and run Cargo checks directly in CI.

- Place each crate directly under `crates/`, using its package name. Move shared
  documentation and examples to root-level `docs/` and `examples/` directories.

- Add a default-disabled `redactable` feature across all six Typehistory crates,
  using Redactable 0.12.0. Each history selects its contract with `redacted`.
  Ordinary histories retain their existing traits and error sources when the
  feature is enabled elsewhere in the dependency graph.
- Add field policies, nested record support, fixed masked `Debug`, and captured
  safe callback errors for opted histories. Payload serialization and frozen
  wire schemas remain independent of redaction policies.
- Document custom frontend error projection through `GenerationPaths.error`.
  A standalone consumer exercises the intended Axis interface; the existing
  Axis checkout still requires its own integration changes.
- Extend repository checks to both feature modes, including external consumers,
  documentation examples, doctests, and release builds.
