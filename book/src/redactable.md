# Optional Redactable integration

Enable the integration, then select `redacted` on each history that needs it.
Ordinary histories keep their existing traits, callback bounds, and error sources.
Cargo combines dependency features, so enabling the feature alone never selects a redacted history.

## Setup

Enable the feature on both dependencies in `Cargo.toml`:

```toml
[dependencies]
type-history = { version = "0.1", features = ["redactable"] }

[build-dependencies]
type-history-build = { version = "0.1", features = ["redactable"] }
```

Keep the regular `build.rs`, `type-history.toml`, and ledger from [package setup](setup.md).
Install the CLI with the same feature:

```sh
cargo install cargo-type-history --version 0.1 --locked --features redactable
```

All six Type History crates expose the default-disabled feature.
The runtime enables its core and procedural macro capabilities.
The build hook and CLI propagate the capability through code generation.
Without this feature, the resolved dependency graph contains no Redactable package.

The integration pins **Redactable 0.12.0** with `default-features = false` and `features = ["redaction"]`.
Its released runtime and derive sources define the supported contracts:
[runtime documentation](https://docs.rs/redactable/0.12.0/redactable/),
[release source](https://docs.rs/crate/redactable/0.12.0/source/), and
[Cargo feature unification](https://doc.rust-lang.org/cargo/reference/features.html#feature-unification).

## Declare and read a redacted history

This example uses a `ReceiptRecorded` event and belongs in an initialized consumer library:

```rust
use serde_json::from_slice;
use std::error::Error;
use type_history::redactable::ToRedacted;
use type_history::versioned;

#[versioned(stable_name = "billing.receipt.recorded", redacted)]
pub struct ReceiptRecorded {
    #[sensitive]
    pub customer: String,
    #[not_sensitive]
    pub amount: u32,
}

pub fn read_receipt(bytes: &[u8]) -> Result<ReceiptRecorded, Box<dyn Error>> {
    let stored = from_slice(bytes)?;
    let receipt = ReceiptRecorded::from_versioned(stored)?;
    let safe_json = receipt.to_redacted().json();
    assert_eq!(safe_json["customer"], "[REDACTED]");
    Ok(receipt)
}
```

The external consumer suite compiles this example from this document.
Its input is a complete stored record:

```json
{"stable_name":"billing.receipt.recorded","version":1,"payload":{"customer":"Alice","amount":42}}
```

Write a current `ReceiptRecorded` event with `serde_json::to_vec(&receipt.into_versioned())`.
Both generated methods use the current alias and need no trait imports.
`Versioned` keeps `stable_name`, the numeric `version`, and `payload` together.
It accepts these fields in any order and validates the payload against its historical type.
The application owns storage and chooses a format supporting maps and named payload fields.

The ordinary `HasHistory`, `decode`, `DecodeError`, and `ReadError` remain unchanged.
For existing raw JSON with separate metadata, redacted histories retain `redaction::decode` and the `redaction::HasRedactedHistory` trait.
Their low-level descriptor is `History<T, RedactedDecodeError>`.

## Record traits and field policies

Every retained redacted version implements `Clone`, `PartialEq`, strict Serde codecs, and the ordinary structural schema contracts.
It also implements `RedactableWithMapper`, `Redactable`, and `ToRedacted`.
Its current alias shares those implementations.
The macro owns `Debug` and rejects competing derives.

`Debug` prints only `RecordV1(<redacted>)`, including during tests and release builds.
It never reads fields or calls their `Debug`, `Clone`, or serialization implementations.
This protects inherited nested fields even when their own `Debug` reveals raw data or panics.
`ToRedacted` alone does not certify a nested type's other formatting paths.

| Field declaration | Structural redaction contract |
| --- | --- |
| `#[sensitive]` | Apply Redactable's `Secret` policy |
| `#[sensitive(Policy)]` | Apply the selected policy through release `PolicyField<Policy>` dispatch |
| `#[not_sensitive]` | Preserve the field; the author declares it public |
| No policy | Require the field type to implement `Redactable` and traverse its declared structure |

Inherited bare strings and scalars fail compilation because they have no declared redaction behavior.
Containers inherit this requirement for their contents.
Explicit policies must support each retained Rust type that carries the field.
For example, an `Email` policy cannot apply to an earlier integer version.
Duplicate policies, contradictory policies, malformed declarations, and policies on ordinary histories are errors.
Missing integration capabilities are errors; requested policies are never silently discarded.

`ToRedacted` clones the record, applies its structural policies, and captures the result as owned JSON.
The release helper returns a fixed redacted fallback if serialization fails.
Its `text()` and `json()` outputs expose only this captured projection.
`Redactable::redact()` consumes the value and returns the same record type with transformed fields.
Generated records have no `Display` implementation; use the projected value's `text()` for text output.

Custom policies, handwritten `Redactable` implementations, and explicit public declarations remain the author's responsibility.
Structured projection executes the declared `Clone`, policy, and serialization code and can inherit its panics.
The fixed generated `Debug` does not execute that code.

## Payloads, nested records, and policy edits

Ordinary Serde serialization always writes the original field values.
This also applies to `Versioned`, whose envelope preserves the historical payload's strict codecs.
Never persist `to_redacted().json()` as a historical payload.
Changing a frozen history's `stable_name` fails the build.

A field policy applies across that field's complete retained lifetime.
Added fields acquire the policy at their first version.
Converted fields use the same policy on predecessor and current types.
Removed fields keep their policy in every older version containing them.
Policy edits rerun source validation and compilation, including historical trait checks.
They do not change the frozen wire schema or require a new payload version.

Nested supporting records can derive `Schema` alongside their Serde and Redactable contracts.
Generated history versions also implement `JsonSchemaField`; they can serve as nested fields, including through `Option`.
The optional core integration supplies transparent schema adapters for `SensitiveValue<T, P>`, `BypassRedaction<T>`,
`BypassDebugRedaction<T>`, and `BypassDisplayRedaction<T>`.
These adapters describe the inner wire type, matching the release's ordinary Serde behavior.
`SensitiveValue<T, P>` derives equality with a `P: PartialEq` bound in this release.
Built-in policies such as `Secret` lack that bound; use an equality-capable custom policy or annotate the raw field directly.
Logging-only wrappers without Serde support are not persisted field types.
The adapters live in `type-history-core`, which owns the schema traits and can legally implement them for foreign wrappers.

## Callback errors and causes

Ordinary callbacks still require `Error + Send + Sync + 'static`.
Their concrete errors remain available through `Error::source()` after `from_versioned` fails.
The low-level JSON API also preserves original JSON errors.
This contract stays unchanged in a graph containing both ordinary and redacted consumers.

Redacted callbacks require only `ToRedacted`.
The generated call passes the concrete error into `RedactedDecodeError::upcast` before any type erasure.
The constructor projects it once and drops the raw value.
The safe wrapper never stores a raw error for later formatting.

`ReceiptRecorded::from_versioned` returns `RedactedDecodeError` when a conversion fails.
`Debug`, `Display`, Serde output, and `ToRedacted` expose only validated metadata and captured safe causes.
Metadata includes the stable name, stored version, failure kind, and failing adjacent versions.
`RedactedDecodeError::source()` returns `None`; `redacted_cause()` exposes the captured safe value.
`RedactedReadError::source()` exposes only its `RedactedDecodeError`, whose source chain ends there.
A stable name mismatch has no source.

For a redacted history, `Versioned::deserialize` discards parser errors without formatting them and returns a fixed safe message.
The caller receives that message in the selected format's error type.
This boundary covers the stable name, version, wrapper fields, and historical payload decoding.
It does not cover errors raised outside the implementation, including a format's trailing-input checks.
Serialization also writes real data, and serializer errors are not universally redacted.
Do not treat every error from the example's `Box<dyn Error>` as safe to log.

The low-level `redaction::decode` API continues to discard raw JSON errors and retain a fixed safe cause.

## Renamed dependencies and derives

Generated Type History paths follow the consumer's dependency name, including renamed dependencies.
The integration addresses the pinned Redactable runtime through Type History's reexport.
Generated records do not invoke Redactable derives, so they require no direct Redactable dependency.
Generated code contains no `cfg(feature = "redactable")` check against the consumer's own feature names.

Authors who use upstream derives such as `Sensitive` on supporting types must add a direct Redactable dependency.
Redactable 0.12.0 derives find that dependency with `proc_macro_crate`; it may be renamed.
They fall back to `::redactable` when discovery fails and do not discover Type History's runtime reexport.
Upstream derives require an explicit policy. Import it with `use redactable::Secret;`
and apply `#[sensitive(Secret)]`.
Use the same pinned release to avoid distinct trait versions.

## Custom macro frontends

`RecordInput.redacted` selects record contracts for `generate_history`.
`GenerationPaths.support` supplies the runtime facade; opted records require its `redactable` reexport.
`GenerationPaths.error` remains the error adaptation interface.
The generator calls these associated constructors:

```text
unsupported(stable_name: StableName, stored: PayloadVersion) -> Self
decode(stable_name: StableName, stored: PayloadVersion, error: serde_json::Error) -> Self
upcast<E>(stable_name: StableName, stored: PayloadVersion,
          from: PayloadVersion, to: PayloadVersion, error: E) -> Self
```

The frontend owns the generic bound on `E` and projects the concrete error in `upcast`.
This lets a framework preserve its own structured errors before erasing their concrete types.
Public `DecodeContext` constructors support frontend-owned error storage without requiring a shared error-tree abstraction.

The standalone macro adds `Versioned` support through the separate `generate_versioned` entry point.
Existing `generate_history` consumers keep their raw codecs and frontend-owned contracts.

`redaction::record` in `type-history-codegen` generates only redaction traits for a concrete named supporting record.
The caller supplies `Clone`, `Serialize`, and its own codecs and schema implementations.

The standalone consumer suite exercises these generation interfaces with a frontend-owned structured error.
