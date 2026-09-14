# Integrating Type History

The shop's receipt starts with integers and strings. Other events may need an address, a delivery status, or types declared in another module. This chapter shows how those types participate in the same historical schema checks.

Start with [package setup](setup.md) if the build hook and ledger are not configured yet.

## Modules and dependency names

Put histories in ordinary Rust modules and import the macro explicitly.
Types, conversion functions, and expressions resolve where the declaration
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
standalone packages excluded from an ancestor workspace. Temporary output can
live beneath that ancestor without making the copied package join it.
Changes to ancestor workspace manifests during validation invalidate the
snapshot. Workspace lookup ignores unrelated Cargo settings above temporary
output while preserving the caller's selected toolchain.

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
- Other Serde or Schemars attributes that change the representation are rejected.
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

## Supported scope

History declarations use structs with named fields, including empty structs.
Supported field types include:

| Kind | Supported types |
| --- | --- |
| Scalars | `bool`, `String`, and signed or unsigned integers from 8 to 128 bits |
| Containers | Vectors, fixed arrays of lengths 0–32, and optional values with supported element types |
| Nested records | Named records with schema support, including generated history versions |
| Enums | Concrete supporting enums with externally tagged variants and schema support |

Array length is part of the schema. `Vec<u8>` has a dedicated byte schema;
other vectors use a sequence schema.

The authoring API currently rejects:

- **Unsupported history declarations:** Enums, tuple structs, unit structs, generics, and recursive histories. Supporting enums are allowed as fields.
- **Unsupported field types:** Floats, pointer-sized integers, maps, sets, and nested options.
- **Unsupported callbacks:** Async functions and functions that consume the whole previous record.
- **Conditional declarations:** The set of histories must not change with conditional compilation.
  Supported feature-selected type aliases still undergo frozen schema checks.
- **Serialization overrides:** Serde `rename`, `default`, `flatten`, `skip`, `alias`, and `with`.
  Use history attributes to describe field changes.

Each retained version adds generated code. Before generating those types, Type History checks that the declared version range agrees with the ledger. A mistaken large version number is rejected before the generator tries to create all those versions.
An existing history can be brought into a package through [import](lifecycle.md#import-a-complete-history).
Its ledger must include every version starting at V1.
