# Optional Redactable integration

Enable the integration, then select `redacted` on each history that needs it.
Ordinary histories keep their existing traits, callback bounds, and error sources.
Cargo combines dependency features, so enabling the feature alone never selects a redacted history.

## Setup

Enable `features = ["redactable"]` on both `typehistory` and `typehistory-build` in the consuming package.
Keep the regular `build.rs`, `typehistory.toml`, and ledger setup from the main tutorial.
Build the CLI with the same capability:

```sh
cargo install --path "$TYPEHISTORY_CHECKOUT/crates/typehistory/cargo-typehistory" --locked --features redactable
```

All six Typehistory crates expose the default-disabled feature.
The runtime enables its core and procedural macro capabilities.
The build hook and CLI propagate the capability through code generation.
Without this feature, the resolved dependency graph contains no Redactable package.
The repository lockfile still records optional packages so both configurations can use `--locked`.

The integration pins **Redactable 0.12.0** with `default-features = false` and `features = ["redaction"]`.
Its released runtime and derive sources define the supported contracts:
[runtime documentation](https://docs.rs/redactable/0.12.0/redactable/),
[release source](https://docs.rs/crate/redactable/0.12.0/source/), and
[Cargo feature unification](https://doc.rust-lang.org/cargo/reference/features.html#feature-unification).

## Declare and read a redacted history

This example belongs in an initialized consumer library:

```rust
use typehistory::{history, PayloadVersion};
use typehistory::redactable::ToRedacted;
use typehistory::redaction::{self, HasRedactedHistory, RedactedReadError};

#[history(id = "billing.receipt.recorded", redacted)]
pub struct Receipt {
    #[sensitive]
    pub customer: String,
    #[not_sensitive]
    pub amount: u32,
}

pub fn read_receipt(bytes: &[u8]) -> Result<Receipt, RedactedReadError> {
    let receipt = redaction::decode::<Receipt>(
        &Receipt::TYPE, PayloadVersion::INITIAL, bytes,
    )?;
    let safe_json = receipt.to_redacted().json();
    assert_eq!(safe_json["customer"], "[REDACTED]");
    Ok(receipt)
}
```

The external consumer suite compiles this example from this document.
The ordinary `HasHistory`, `decode`, `DecodeError`, and `ReadError` remain unchanged.
Redacted histories instead implement `redaction::HasRedactedHistory` and use `redaction::decode`.
Their descriptor is `History<T, RedactedDecodeError>`.

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
It remains separate from redacted output and preserves payload bytes and strict decoding rules.
Never persist `to_redacted().json()` as a historical payload.

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
The adapters live in `typehistory-core`, which owns the schema traits and can legally implement them for foreign wrappers.

## Callback errors and causes

Ordinary callbacks still require `Error + Send + Sync + 'static`.
Their concrete callback errors and JSON errors remain available through `Error::source()`.
This contract stays unchanged in a graph containing both ordinary and redacted consumers.

Redacted callbacks require only `ToRedacted`.
The generated call passes the concrete error into `RedactedDecodeError::upcast` before any type erasure.
The constructor projects it once and drops the raw value.
The safe wrapper never stores a raw error for later formatting.

Redacted JSON failures discard the raw decoder error and retain a fixed safe message.
`Debug`, `Display`, Serde output, and `ToRedacted` expose only validated metadata and captured safe causes.
Metadata includes identity, stored version, failure kind, and failing adjacent versions.
`RedactedDecodeError::source()` returns `None`; `redacted_cause()` exposes the captured safe value.
`RedactedReadError::source()` exposes only its `RedactedDecodeError`, whose source chain ends there.
An identity mismatch has no source.

## Renamed dependencies and derives

Generated Typehistory paths follow the consumer's dependency name, including renamed dependencies.
The integration addresses the pinned Redactable runtime through Typehistory's reexport.
Generated records do not invoke Redactable derives, so they require no direct Redactable dependency.
Generated code contains no `cfg(feature = "redactable")` check against the consumer's own feature names.

Authors who use upstream derives such as `Sensitive` on supporting types must add a direct Redactable dependency.
Redactable 0.12.0 derives find that dependency with `proc_macro_crate`; it may be renamed.
They fall back to `::redactable` when discovery fails and do not discover Typehistory's runtime reexport.
Unlike Typehistory's shorthand, upstream derives require an explicit policy, such as `#[sensitive(redactable::Secret)]`.
Use the same pinned release to avoid distinct trait versions.

## Axis and other macro frontends

`RecordInput.redacted` selects record contracts for `generate_history`.
`GenerationPaths.support` supplies the runtime facade; opted records require its `redactable` reexport.
`GenerationPaths.error` remains the error adaptation interface.
The generator calls these associated constructors:

```text
unsupported(identity: TypeIdentity, stored: PayloadVersion) -> Self
decode(identity: TypeIdentity, stored: PayloadVersion, error: serde_json::Error) -> Self
upcast<E>(identity: TypeIdentity, stored: PayloadVersion,
          from: PayloadVersion, to: PayloadVersion, error: E) -> Self
```

The frontend owns the generic bound on `E` and projects the concrete error in `upcast`.
Axis can require its own `ToRedactedError`, preserving structured error-tree fields before erasure.
Redactable's `ToRedacted` returns a `RedactedValue`; it is not Axis's `ToRedactedError` contract.
Axis error trees, event traits, and metadata stay in Axis.
Public `DecodeContext` constructors support frontend-owned error storage without requiring a shared error-tree abstraction.

`redaction::record` in `typehistory-codegen` generates only redaction traits for a concrete named supporting record.
The caller supplies `Clone`, `Serialize`, and its own codecs and schema implementations.
This interface does not implement Axis event traits or an enum/value macro frontend.

The standalone consumer suite exercises these generation interfaces with a frontend-owned structured error.
The Axis extraction checkout still calls removed helpers and uses different constructor signatures.
This integration does not establish that the existing Axis checkout builds against these changes.
