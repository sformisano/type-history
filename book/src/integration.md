# Integrating Type History

This page covers module placement, renamed dependencies, nested records, and supported field types.
Start with [package setup](setup.md) if the build hook and ledger are not configured yet.

## Modules and dependency names

Put histories in ordinary Rust modules and import the macro explicitly.
Types, conversion functions, and expressions resolve where the declaration
appears, including supported `self`, `super`, and `crate` paths.

For a renamed runtime dependency, Cargo's dependency key becomes the crate name:

```toml
[dependencies]
history_api = { package = "type-history", version = "0.1" }
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
- **Generated names:** Avoid collisions with names such as `InvoiceV1` and
  `InvoiceV2`. Raw identifiers keep their Rust spelling; `r#type` is stored as `type`.
  Duplicate serialized field names are rejected.
- **Source directories:** A source directory can be named `target`. Lifecycle
  snapshots exclude Cargo's actual build output directory.

## Nested records

A field can contain another named struct. Type History needs to know that struct's
schema so a later edit cannot silently change a frozen record's nested fields.

For a supporting struct such as `Address`:

1. Derive `type_history::Schema` to describe its structure for the build checks.
2. Derive Serde's `Serialize` and `Deserialize` for reading and writing its values.
3. Add `#[serde(deny_unknown_fields)]` so decoding rejects unexpected fields.

`Schema` supplies `ResolvedSchema` and schema export support. The other derives
supply serialization; constructors and application validation remain your code's responsibility.

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

## Supported scope

History declarations use structs with named fields, including empty structs.
Supported field types include:

| Kind | Supported types |
| --- | --- |
| Scalars | `bool`, `String`, and signed or unsigned integers from 8 to 128 bits |
| Containers | Vectors, fixed arrays, and optional values with supported element types |
| Nested records | Named records with schema support, including generated history versions |

Array length is part of the schema. `Vec<u8>` has a dedicated byte schema;
other vectors use a sequence schema.

The authoring API currently rejects:

- **Unsupported shapes:** Enums, tuple structs, unit structs, generics, and recursive histories.
- **Unsupported field types:** Floats, pointer-sized integers, maps, sets, and nested options.
- **Unsupported callbacks:** Async functions and functions that consume the whole previous record.
- **Conditional declarations:** The set of histories must not change with conditional compilation.
  Supported feature-selected type aliases still undergo frozen schema checks.
- **Serialization overrides:** Serde `rename`, `default`, `flatten`, `skip`, `alias`, and `with`.
  Use history attributes to describe field changes.

Generated code grows with the number of supported versions. The ledger limits
which version ranges can be generated, and these checks run before expansion.
A history starting above V1 needs an explicit [import](lifecycle.md#import-a-retained-floor).

## Optional Redactable integration

If records contain sensitive data, the optional Redactable integration provides
redacted output and errors for the histories you select. It is disabled by default.

To use it:

1. Enable the `redactable` feature on `type-history`, `type-history-build`, and the CLI.
2. Select each history with `redacted` and declare its field policies.

For example, this separate receipt history hides the customer in redacted output:

<!-- reference:redacted-history.rs -->
```rust
use type_history::versioned;

#[versioned(stable_name = "billing.receipt.recorded", redacted)]
pub struct ReceiptRecorded {
    #[sensitive]
    pub customer: String,
    #[not_sensitive]
    pub amount: u32,
}
```

Request a redacted projection when displaying the value:

<!-- reference:redacted-output.rs -->
```rust
use type_history::redactable::ToRedacted;

let receipt = ReceiptRecorded { customer: "Alice".to_owned(), amount: 42 };
let safe = receipt.to_redacted();
assert_eq!(safe.json()["customer"], "[REDACTED]");
assert_eq!(safe.json()["amount"], 42);
```

Enabling the feature alone does not change ordinary histories, even when another
dependency enables it. They keep their ordinary traits and error types.
Ordinary Serde serialization still writes the original field values, including
for redacted histories. Persist those payloads; use redacted output for display.

The [integration guide](redactable.md) covers setup, field policies, nested
records, callback errors, and renamed dependencies, using Redactable 0.12.0.
