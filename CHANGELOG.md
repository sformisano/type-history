# Changelog

## Unreleased

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
