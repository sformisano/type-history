# Integration and supported types

The shop's receipt starts with integers and strings. Other events may need an address, a delivery status, or types declared in another module. This chapter shows how those types participate in the same historical schema checks.

Start with [package setup](setup.md) if the build hook and ledger are not configured yet.

## Modules and dependency names

Put histories in ordinary Rust modules and import the macro explicitly.
Types, callbacks, and backfill expressions resolve where the declaration
appears, including supported `self`, `super`, and `crate` paths.
Each expansion must match the source position, library module, and record type
discovered by the build hook. Copying a discovered declaration through `include!`
or another macro does not authorize a second history. Undiscovered or copied
histories are rejected in every build profile. Each admitted declaration also
receives the current ledger, frozen-shape, and strict checks.

Suppose the application calls its Type History dependency `history_api`. Cargo uses that key as the Rust crate name. With the sibling checkout from [package setup](setup.md), the dependency is:

<!-- integration:Cargo.toml -->
```toml
[dependencies]
history_api = { package = "type-history", path = "../type-history/crates/core/type-history" }
```

Use that name in the module containing the history:

<!-- reference:renamed-dependency.rs -->
```rust
use history_api::versioned;

#[versioned(stable_name = "shop.delivery.requested")]
pub struct Delivery {
    pub postal_code: String,
}
```

If you rename `type-history-build`, use that name in `build.rs` too.
The following rules also apply:

- **Dependency tables:** Runtime dependencies can be target-specific or inherited
  from the workspace. History declarations themselves must be unconditional.
- **Multiple dependency versions:** Use one Type History version, or the same
  dependency name across target tables. The macro's dependency-name resolver can
  choose an inactive alias when different versions use different names, causing
  compilation to fail. See the resolver's [documented edge cases](https://docs.rs/proc-macro-crate/3.5.0/proc_macro_crate/#edge-cases).
- **Generated names:** Avoid collisions with names such as `InvoiceV1` and
  `InvoiceV2`. Raw identifiers keep their Rust spelling; `r#type` is stored as `type`.
  Duplicate serialized field names are rejected.
- **Source directories:** A source directory can be named `target`. Lifecycle
  snapshots exclude Cargo's actual build output directory.
- **Source symlinks:** Package-local source symlinks keep Rust's module lookup
  relative to the authored path. Cargo uses modification times for change
  detection, so retargeting a symlink can require a forced rebuild. After
  retargeting source links, clean the package for the profile you will build,
  for example `cargo clean --package YOUR_PACKAGE --profile release`.

Discovery watches existing source files and directories that can gain a
competing module file. Unchanged file modules reuse Cargo's previous build when
build output stays outside those directories. Changes elsewhere in a watched
directory can also rerun the hook. If source modules sit beside `Cargo.toml`, use
a separate target directory to avoid watching build output. Custom generators
must report their own input files through Cargo's build-script directives.

Lifecycle snapshots preserve Cargo's resolved workspace boundaries, including
standalone packages excluded from an ancestor workspace. A snapshot can live
beneath that ancestor, as it does in Cargo's target directory, without making the
copied package join it. Changes to ancestor workspace manifests during validation
invalidate the snapshot. Workspace lookup ignores unrelated Cargo settings above
the snapshot directory while preserving the caller's selected toolchain.

## Nested records

A delivery event might contain an `Address` struct. Its fields are part of the stored event, so changing `Address` also changes the delivery's schema. Type History needs a schema for the nested struct to catch that change after freezing.

For a supporting struct such as `Address`:

1. Derive `type_history::Schema` to describe its structure for the build checks.
2. Derive Serde's `Serialize` and `Deserialize` for reading and writing its values.
3. Add `#[serde(deny_unknown_fields)]` so decoding rejects unexpected fields.

`Schema` generates the structure used by the build checks through `ResolvedSchema` and the schema export traits. Serde's derives read and write the values. Your constructors and validation still decide which addresses the application accepts.

Add this module to an initialized library:

<!-- example:nested-example.rs -->
```rust
pub mod addresses {
    use serde::{Deserialize, Serialize};
    use type_history::{versioned, Schema};

    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Schema)]
    #[serde(deny_unknown_fields)]
    pub struct Address {
        pub number: u32,
    }

    #[versioned(stable_name = "billing.address.record")]
    pub struct AddressRecord {
        pub address: Address,
    }

    #[test]
    fn nested_values_keep_their_closed_codec() {
        use serde_json::from_slice;
        use type_history::Versioned;

        let stored = from_slice(
            br#"{"stable_name":"billing.address.record","version":1,"payload":{"address":{"number":7}}}"#,
        ).unwrap();
        assert!(AddressRecord::from_versioned(stored).is_ok());
        assert!(from_slice::<Versioned<AddressRecord>>(
            br#"{"stable_name":"billing.address.record","version":1,"payload":{"address":{"number":7,"unknown":1}}}"#,
        ).is_err());
    }
}
```

The test accepts an address with `number` and rejects one with an unknown field.
Adding this module creates a new V1 draft. Freeze it before a release build.

Nested types follow these additional rules:

- Public supporting records may contain private nested field types.
- Generated history versions and current aliases can also be nested fields,
  including through `Option`. Their schema is the selected version's structure.
- The `Schema` derive accepts `#[serde(deny_unknown_fields)]` on the type. It rejects
  every other Serde or Schemars attribute on the type, its fields, and its variants.
- If a supporting type uses Schemars directly, use `schemars = "=1.2.2"`.

## Enum fields

A delivery can be pending or shipped with a tracking number. An enum represents those states inside a versioned struct. Derive `Schema` and Serde's traits on the enum, just as for `Address`. Keep `#[versioned]` on the containing struct:

<!-- example:enum-example.rs -->
```rust
pub mod deliveries {
    use serde::{Deserialize, Serialize};
    use type_history::{versioned, Schema};

    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Schema)]
    #[serde(deny_unknown_fields)]
    pub enum DeliveryState {
        Pending,
        Shipped { tracking: String },
    }

    #[versioned(stable_name = "shop.delivery.state")]
    pub struct Delivery {
        pub state: Option<DeliveryState>,
        pub previous_states: Vec<DeliveryState>,
    }

    #[test]
    fn read_delivery() {
        let stored = serde_json::from_str(
            r#"{"stable_name":"shop.delivery.state","version":1,"payload":{"state":{"Shipped":{"tracking":"PKG-42"}},"previous_states":["Pending"]}}"#,
        ).unwrap();
        let delivery = Delivery::from_versioned(stored).unwrap();
        assert_eq!(delivery.state, Some(DeliveryState::Shipped {
            tracking: "PKG-42".to_owned(),
        }));
        assert_eq!(delivery.previous_states, vec![DeliveryState::Pending]);
    }
}
```

Enums use Serde's externally tagged representation. A unit variant such as
`Pending` is the JSON string `"Pending"`. A payload variant uses its name as
the object's single key, as shown by `Shipped` above.

Supported variants are unit, one-field newtype, tuple with at least two fields,
and named-field variants. Their fields can contain other supported types.
Named-field variants require maps; use `rmp_serde::to_vec_named` for MessagePack.
Empty enums and zero-field tuple variants are unsupported. Empty named-field
variants are supported.

After freezing the delivery, adding a variant to `DeliveryState` would change its historical schema. Keep the old enum and introduce a new enum type through an `updated_in` field change. Supply `previous_type` and a backfill, as shown in the [payment example](https://github.com/sformisano/type-history/blob/main/crates/examples/invoice-history/src/payment.rs).

The containing struct owns the history. Enum variants have no history attributes or independent version numbers.

A variant containing one named struct can serialize exactly like a variant declaring those same fields itself. The frozen check compares their serialized structures, so it accepts that equivalent representation.

## Supported field contracts

History declarations use concrete structs with named fields, including empty
structs. Supporting records, enums, and nonempty tuple structs can appear as
fields. These supporting types do not own separate histories.

| Kind | Supported types and contract |
| --- | --- |
| Scalars | `bool`, `String`, and signed or unsigned integers from 8 to 128 bits |
| Sequences and options | `Vec<T>`, arrays of lengths 0–32, and `Option<T>` |
| Maps | `HashMap<String, V, S>` and `BTreeMap<String, V>` |
| Sets | `HashSet<T, S>` and `BTreeSet<T>` when `T` declares stable membership |
| Tuples | Bare tuples with 1–16 items and supporting tuple structs with at least one field |
| Owned pointers | `Box<T>`, plus `Rc<T>` and `Arc<T>` with the `rc` feature |
| Supporting types | Named records, externally tagged enums, and nonempty tuple structs with `Schema` and matching Serde support |

Array length, tuple length, and tuple position are part of the storage contract.
`Vec<u8>` has a dedicated byte contract. Other vectors use a sequence contract.
A one-field tuple struct uses Serde's newtype encoding. It differs from `(T,)`.

Maps require `String` keys. `HashMap` and `BTreeMap` have the same storage
contract when their value contracts match. The hasher and iteration order do
not affect compatibility. Serialization order is not a deterministic byte-order
promise.

Sets differ from sequences. `HashSet` and `BTreeSet` have the same storage
contract only when the element encoding and membership declaration match.
Standard Rust bounds still apply. `HashSet` needs `Eq + Hash`, and `BTreeSet`
needs `Ord`.

Built-in supported values provide membership declarations. Custom elements
must implement `SetMembership` with a stable, namespaced ID:

```rust
use type_history::{ConstantMembership, SetMembership};

impl SetMembership for CustomerCode {
    const MEMBERSHIP: ConstantMembership =
        ConstantMembership::custom("com.example:customer-code:case-folded:v1");
}
```

The declaration is a contract you maintain. `Schema` does not derive it or
prove custom `Eq`, `Hash`, or `Ord` behavior. Change the ID when membership
behavior changes. Introduce that change through a new history version.
Type History does not scan stored collections for duplicate members.

## Presence and nullable values

Named-field presence and nullability are separate parts of the contract.
`Option<T>` permits an omitted field or an explicit null. `Box`, `Rc`, and
`Arc` preserve the wrapped value's field-presence rule.

A one-field tuple struct is a required field even when its inner value is
`Option<T>`. Its field rejects omission and accepts an explicit null.
This preserves Serde's newtype encoding without treating the newtype as an
optional field.

Ambiguous nested nulls remain unsupported. For example,
`Option<NullableNewtype>` is rejected when `NullableNewtype` wraps `Option<T>`.
Tuple positions are always present, although a supported position can contain
an explicit null.

Changing only field presence is still a storage-contract change. Add a new
history version and migrate explicitly.

## Optional adapters

The facade has no default features. Enable only the integrations a record uses:

```toml
[dependencies]
type-history = { version = "=0.3.1", features = ["uuid", "rust-decimal", "time"] }
```

These features were introduced in 0.2.0. See
[package setup](setup.md#1-install-the-components) for matching library, build-hook,
and CLI versions.

| Feature | Public field type | Stored contract |
| --- | --- | --- |
| `typed-floats` | `typed_floats::NonNaNFinite<f32>` and `NonNaNFinite<f64>` | Finite JSON numbers with distinct 32-bit and 64-bit profiles |
| `uuid` | `type_history::adapters::UuidText` | Lowercase hyphenated UUID text; all UUID bit patterns |
| `rust-decimal` | `type_history::adapters::DecimalText` | Signed decimal text with a 96-bit coefficient and scale 0–28 |
| `chrono` | Five adapters in `type_history::adapters::chrono` | Shared checked date and time text profiles |
| `time` | Five adapters in `type_history::adapters::time` | The same checked profiles as `chrono` |
| `rc` | `Rc<T>` and `Arc<T>` | The inner value by value |

The temporal modules expose `Date`, `LocalTime`, `LocalDateTime`, `UtcInstant`,
and `OffsetDateTime`. Their shared domains are:

- Dates use proleptic Gregorian years `0000` through `9999`.
- Local times use seconds `00` through `59` and at most nine fraction digits.
- Local datetimes join the date and time with `T`.
- UTC instants end in `Z`.
- Offset datetimes retain a minute-aligned numeric offset.

Offset datetimes require both local and UTC dates to stay in range. The adapters
reject leap seconds, second offsets, excessive precision, and unknown `-00:00`
offsets. They write the shortest exact nanosecond fraction.

`DecimalText` preserves coefficient and scale, including trailing zeros.
Its membership follows numeric equality, so `1.0` and `1.00` are equal set
members. `OffsetDateTime` membership follows instant equality and ignores the
retained offset. The bytes still retain scale and offset.

Each UUID, decimal, and temporal adapter provides checked `TryFrom<Native>`,
`as_inner()`, and `into_inner()`.
It provides no mutable native access. Native values outside the profile fail
conversion. Native dependency Serde features cannot change the adapter's domain,
schema, or encoding.

Each profile is distinct from unrestricted `String`. The finite `f32` and `f64`
profiles are also distinct. Switching between these contracts requires a new
history version and an explicit migration.

## Tuple trait limits

Type History supports bare tuples through arity 16 because the supported codecs
handle those arities. Rust's standard `Debug` and `PartialEq` tuple implementations
stop at arity 12.

Use per-history derive options for a field containing a tuple of arity 13–16:

```rust
use type_history::versioned;

type Coordinates = (u8, u8, u8, u8, u8, u8, u8, u8, u8, u8, u8, u8, u8);

#[versioned(
    stable_name = "mapping.coordinates",
    derive_debug = false,
    derive_partial_eq = false,
)]
pub struct CoordinatesRecord {
    pub value: Coordinates,
}
```

Both options default to `true`. Each option applies to every retained version
in that history. Disabling one removes that generated implementation and its
field bound. `Clone` remains required.

When an option stays enabled, generated code uses the native field trait.
Custom `Debug` and `PartialEq` behavior is preserved. Type History does not
replace missing traits with byte comparison or structural formatting.

These options do not change schemas, payload bytes, frozen ledgers, migrations,
or set membership. Sets still require the standard equality, hash, or ordering
traits for their chosen collection.

## Codec guarantees

The storage guarantees cover JSON and MessagePack with named struct fields.
Use `rmp_serde::to_vec_named` for MessagePack. The three `Versioned` fields and the
payload fields can appear in any order when decoding.

Supported `Versioned` writing and decoding preserve finite float width and bits,
including signed zero and subnormal values. They also preserve decimal
coefficient and scale, temporal nanoseconds, and retained offsets. Invalid
profile values return checked errors without rounding, truncation, clamping, or
null substitution.

Other Serde formats are outside this guarantee. MessagePack payload struct arrays
are unsupported. Map iteration order does not promise deterministic serialized
bytes.

## Changes that require migration

An unchanged version requires the same complete storage contract. This includes
representation, admitted domain, field presence, logical profile, set membership,
tuple order, and tuple arity.

For example, these substitutions require a new version:

- `Vec<T>` to a set
- `NonNaNFinite<f32>` to `NonNaNFinite<f64>`
- `UuidText` or `DecimalText` to `String`
- a required nullable newtype to `Option<T>`
- a changed custom membership ID

Keep the previous field type in the retained version. Add an `updated_in` record
with `previous_type` and an explicit conversion. Frozen checks do not certify
custom conversion meaning, `Eq`, `Hash`, `Ord`, or serialization behavior.
Existing frozen ledger bytes remain unchanged. Schema keywords such as
`x-type-history-membership` appear only in entries whose fields use them.

## Unsupported scope

The authoring API rejects:

- history roots that are enums, tuple structs, unit structs, generic, or recursive
- non-string map keys, unit values, and empty tuple structs
- raw or non-finite floats, pointer-sized integers, and ambiguous nested options
- borrowed data, weak pointers, cycles, locks, and atomics
- arbitrary-precision decimals, named timezones, and epoch-unit profiles
- arbitrary Serde `rename`, `default`, `flatten`, `skip`, `alias`, or `with` overrides
- async callbacks and callbacks that consume the whole previous record
- conditionally compiled history declarations

Supported feature-selected aliases still undergo frozen storage-contract checks.
`Box`, `Rc`, and `Arc` store only the inner value. Allocation identity and
sharing are not preserved.

When a field type lacks schema support, compilation points to that field type.
For a supported record, enum, or tuple struct, derive `Schema` as shown above.
For another type, choose a supported stored representation.

Each retained version adds generated code. Type History validates the declared
version range before generation. A mistaken large version number fails before
the generator creates those versions.

An existing history can enter a package through [import](lifecycle.md#import-a-complete-history).
Its ledger must include every version starting at V1.
