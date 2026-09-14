# Field-history evolution matrix

Use this page to check whether a particular field change is supported. For
example, a rename removes the old field and adds a new one whose backfill reads
the old value. The [field history guide](field-history.md) walks through those
changes; the [lifecycle guide](lifecycle.md) explains how to freeze them.

Supporting enums can be fields, including in supported containers. Their changes
use the containing struct's field history rules; `#[versioned]` remains restricted
to concrete named-field structs. See [enum fields](integration.md#enum-fields).
The Class column uses **S** for supported, **R** for rejected, and **U** for a
capability the API does not provide. **S/R** means part of the case is supported
and part is rejected; the last column explains the distinction. **U/R** means
the API has no such capability and rejects declarations that try to request it.
Case IDs identify rules so they can be referred to elsewhere. They do not report
whether a test has run.

## Presence, type, and conversion combinations

| ID | Case | Class | Rule / expected outcome |
|---|---|---|---|
| M01 | Unannotated V1 field | S | Present from V1, declared type copied across unchanged versions |
| M02 | Unannotated imported field | S | Present from V1; every imported version must match the source |
| M03 | Added field | S | Use `added_in` with exactly one `backfill_value` or `backfill_fn` |
| M04 | Added `Option<T>` | S | Supply `backfill_value = None` or a callback explicitly |
| M05 | Added type that implements `Default` | S | Supply `backfill_value = Type::default()` or another explicit backfill |
| M06 | Added field with no backfill | R | A backfill is required even for `Option<T>` and types that implement `Default` |
| M07 | Construct the current struct with a field omitted | R | Construction still requires every field; backfills only supply values when converting historical records |
| M08 | Added field later changes type | S | The addition's backfill returns the field's type when it first appeared, reconstructed from the earliest later update's `previous_type` |
| M09 | One type update | S | updated_in, previous_type=previous type, exactly one backfill_value or backfill_fn |
| M10 | Repeated updates | S | Newest-first records reconstruct every intermediate type, including multi-version spans |
| M11 | Update keeps same type | S | Explicit previous_type=same type and callback executes semantic conversion |
| M12 | Type alias changes spelling only | S | Resolved same shape passes frozen checks; ordinary Rust checks callback compatibility |
| M13 | Retained nested/newtype wire shape edited in place | R | Resolved frozen shape fails; preserve old type, introduce new type and field update |
| M14 | Removed field | S | removed_in exclusive, no callback; declaration retained for older payloads |
| M15 | Last field removed | S | The latest struct is empty; callbacks can still read the old value before it is dropped |
| M16 | Field added, updated several times, then removed | S | A field has one lifetime; write its history records newest first |
| M17 | Field rename | S | Old name removed and distinct new name added at same step; payload callback reads/clones old field |
| M18 | Two old fields merged | S | New destination added from payload; both old fields may be removed at same boundary |
| M19 | One old field split into two | S | Each destination has payload callback; both see intact predecessor, can call shared pure helper |
| M20 | Several new fields derived from old values | S | Every callback reads the previous record; none can read another callback's output |
| M21 | Several backfill functions read the same field | S | Every callback borrows the intact previous record before unchanged fields move |
| M22 | Fallible contextual addition | S | backfill_fn returns Result; first failure stops this conversion chain |
| M23 | Fallible update | S | backfill_fn borrows the complete previous struct and returns `Result<destination, E>` |
| M24 | Infallible contextual update | S | Function returns Ok(value) using supported error type |
| M25 | Fallible backfill expression | R | backfill_value is destination value, not Result; use payload callback for failure |
| M85 | Backfill expression uses `return` or `?` | S/R | The expression runs in a closure returning the field type. `return` exits that closure; `?` must be valid for its return type and cannot propagate an error out of the generated conversion |
| M26 | Callback takes an unknown or wrong previous type | R | Rust rejects the call from the generated conversion |
| M27 | Wrong callback output or borrowed output | R | Output must be owned exact destination field type |
| M28 | Callback error lacks required traits | R | Errors require `Error + Send + Sync + 'static` |
| M29 | Non-Copy field | S | Unchanged fields move; backfill functions clone borrowed fields when ownership is needed |
| M30 | Field lacks a trait needed by the generated struct | R | Retained fields require `Clone`, `PartialEq`, and `Debug` |
| M31 | Consume one old value to produce several fields without cloning | U | Callbacks borrow the previous record. There is no callback that consumes it once and returns the whole next record; helpers may clone explicitly |
| M32 | Several outputs require one shared side effect | U | Write deterministic callbacks without side effects. Type History provides no shared callback cache, external transaction, or exactly-once guarantee |

## Grammar, chronology, and stable name combinations

| ID | Case | Class | Rule / expected outcome |
|---|---|---|---|
| M33 | Declare a new version with no field change | U | There is no `current` argument. A new draft needs a field boundary at the next version; existing intermediate versions with unchanged fields remain supported |
| M34 | Completely empty record | S | Empty payload inventory infers V1; version struct and JSON object decoding retained |
| M35 | One record declaration without current | S | Infer max(V1, all field-history boundaries); reject an explicit current key |
| M36 | Zero/overflow/malformed version identifier | R | Positive supported version integer required |
| M37 | Imported history missing V1 | R | The ledger reader rejects histories that do not start at V1 |
| M38 | Field boundary at V1, before V1, or beyond the next allowed version | R | Boundaries must follow V1 and stay within the ledger's allowed next version; unannotated fields already exist at V1 |
| M39 | Two records at same version on one field | R | Addition/update/removal boundary conflict; combine business work in one legal callback |
| M40 | Records not newest first | R | Source chronology error points to field/boundaries |
| M41 | Update before birth or at/after removal | R | No active predecessor/destination interval |
| M42 | Multiple addition or removal records | R | One field lifetime only |
| M43 | Removed field later reintroduced with same name | U/R | Reserved historical Rust/wire name; parser rejects repeated birth/duplicate declaration |
| M44 | Business concept reintroduced under distinct field name | S | New added_in rule; prior field remains removed and readable historically |
| M45 | Raw identifier clashes with same canonical name | R | Canonicalize r# prefix before uniqueness and JSON key checks |
| M46 | backfill_value plus backfill_fn | R | Exactly one backfill rule is required per addition or update |
| M47 | Update assigns a constant | S | previous_type plus backfill_value replaces the predecessor value without reading it |
| M48 | Removed field supplies a backfill or previous_type | R | Removal is structural only |
| M49 | Update omits previous_type or a backfill rule | R | Both the predecessor type and one explicit backfill are required |
| M50 | Unsupported history key or duplicate key | R | The macro reports the invalid key; only documented keys are accepted |
| M52 | Serde rename/default/flatten/skip/alias/with | R | Historical decoders use the declared field names and generated serialization rules |
| M53 | Conditional history or field declaration | R | Histories and their fields must remain visible to source discovery |
| M138 | Undiscovered macro-generated, included, or function-local history | R | Each expansion must match a directly discovered module-level declaration, including in development builds |
| M54 | Supported alias selected by a feature | S | The compiler-resolved schema for the selected features must match the frozen ledger |
| M55 | self/super type, callback, expression, array length | S | Original invocation module and spans retained |
| M56 | Author field names that resemble helpers (`previous`, `value`, `json`, or `__type_history_*`) | S | Generated identifiers hygienic; field names cannot shadow transition input/local bindings |
| M57 | Unsupported field attributes | R | Accept history, documentation, and lint attributes; reject other attributes |
| M58 | Per-version derives or Serde customization | U/R | All generated versions use the history's selected traits and exact historical decoders; unsupported attributes fail |
| M59 | Asynchronous or capturing-closure callback | R | Ordinary synchronous function path required |

## Freeze, draft, decode, and delivery combinations

| ID | Case | Class | Rule / expected outcome |
|---|---|---|---|
| M60 | Add, update, or remove a field in the draft | S | The current draft may change while every earlier frozen schema stays unchanged |
| M61 | Change a field's serialized shape without a new update record | R | The reconstructed historical schema changes, so the generated compile-time check fails |
| M62 | Delete historical field or a previous_type record that changes reconstructed wire shape | R | Frozen wire shape/inventory mismatch |
| M63 | Reorder fields or rename equivalent alias | S | Same canonical wire shape; callback order follows new source order |
| M64 | Edit a callback, remove a same-type update, or substitute a type with the same schema | S | The ledger protects schemas and retained versions; remaining field boundaries must still preserve the latest version. Rust checks types, but author tests must check conversion meaning |
| M65 | Discard a latest draft update | S | Remove all draft boundaries/fields, restore prior declared types and adapt current callers; infer the prior head |
| M66 | Incomplete discard leaves draft annotation or wire-changing declared type | S/R | A remaining draft boundary keeps the draft alive and may pass development checks; removing its update but keeping a changed prior wire type fails frozen checks |
| M67 | Draft-only history declaration deleted | S | No frozen inventory reservation; dev bytes remain disposable |
| M68 | Frozen history declaration deleted | R | Full ordinary build inventory fails despite missing macro expansion |
| M69 | Second successor above an unfrozen draft | R | One latest ordinary draft policy |
| M70 | Freeze, reset, undo, or import | S | Commands validate a snapshot under a lock before writing the ledger atomically. Destructive commands require exact selectors; reset keeps the version reserved |
| M71 | Reset latest shape changes earlier generated history accidentally | R | Earlier frozen assertions still active |
| M72 | Delete reset declaration or skip its version | R | Retained reservation blocks build/release; exact undo/refreeze required |
| M73 | Dev draft vs release draft | S/R | Warning in dev; hard error in release family and strict dev |
| M75 | Decode an old record that needs a backfill | S | Decode its historical type first, then run adjacent conversions; the current type's decoder is not tried first |
| M76 | Omitted `Option<T>` key versus explicit `null` | S | The selected historical type follows Serde's behavior; an omitted optional field may deserialize as `None` |
| M77 | Missing required field/unknown JSON key/invalid newtype | R | Exact historical decoder fails; `Versioned` returns the format's error, while low-level ordinary JSON errors retain their source |
| M78 | Duplicate JSON object key | R | Historical decoders reject duplicates; `Versioned` rejects duplicate payload fields while buffering |
| M79 | Requested zero/future version | R | Unsupported version, no speculative current decode |
| M83 | Failed adjacent conversion | R | Preserve the stored version, failing step, and original cause |
| M113 | Custom macro or generator captures callback errors | S | Constructors named by `GenerationPaths.error` receive the concrete owned error before type erasure |

## Versioned values and stable names

| ID | Case | Class | Rule / expected outcome |
|---|---|---|---|
| M120 | `stable_name` declaration | S | All values and versions share the type's stable name; it stays unchanged after freezing, including across Rust renames and module moves |
| M121 | Unknown struct argument | R | Only documented history argument keys are accepted |
| M122 | Current value serialized as `Versioned<T>` | S | `into_versioned` supplies the stable name and current version; callers cannot replace metadata independently |
| M123 | Historical `Versioned<T>` converted through current alias | S | Deserialization selects the exact retained payload; `from_versioned` applies adjacent conversions with source context |
| M124 | Historical envelope serialized without conversion | S | Keep its historical version and matching data; wrap the converted current value to emit the latest version |
| M125 | Missing, duplicate, or unknown wrapper fields, wrong stable name, or invalid version | R | Require exactly `stable_name`, a positive numeric `u32` `version`, and `payload`; validate the complete wrapper and historical payload before conversion |
| M126 | JSON or MessagePack with named fields | S | Accept wrapper fields in any order; buffer payload values without losing duplicate fields or integer precision, then use the exact historical decoder |
| M127 | Format using payload struct arrays | U | The generated payload decoder requires maps; support depends on the format and field types |
| M129 | Raw payload JSON supplied as a versioned envelope | R | Use the low-level decoder with separately stored metadata |
| M130 | Simple or namespaced stable name | S | `receipt_created`, `shop.receipt`, and longer dotted names are valid; each segment starts with a lowercase ASCII letter and uses only lowercase letters, digits, or underscores, within 255 bytes total |
| M131 | Empty stable name, empty segment, or invalid characters | R | Declaration and deserialization reject invalid stable name syntax |
| M132 | Supporting enum field, including `Option`, `Vec`, or fixed array | S | Derive `Schema` and matching Serde traits; use externally tagged unit, newtype, tuple, or named-field variants |
| M133 | Enum field type update | S | Keep the earlier enum and use the containing struct's `updated_in`, `previous_type`, and exactly one backfill rule |
| M134 | Frozen enum variant or nested payload change | R | Preserve the frozen wire shape; changes need a containing field update in the next version |
| M135 | Independently versioned enum or variant history attribute | R | `#[versioned]` accepts concrete named-field structs; field attributes describe enum changes |
| M136 | Released baseline compared with edited source and ledger | S | `check --released-baseline FILE` requires a distinct frozen ledger and preserves all released entries |
| M137 | Machine-readable schema differences | S | `check --format json` reports stable codes, complete nested paths, expected/actual values, and attributable locations while compiler enforcement stays active |

## Verification and limits

Parser tests live in [codegen](https://github.com/sformisano/type-history/blob/main/crates/core/type-history-codegen/src/history/tests.rs).
Real standalone compiler and lifecycle cases live in
[cargo-type-history](https://github.com/sformisano/type-history/blob/main/crates/core/cargo-type-history/tests/standalone.rs). Its
[tutorial tests](https://github.com/sformisano/type-history/blob/main/crates/core/cargo-type-history/tests/standalone/tutorial.rs) execute
the quick start progression and reference examples. The [invoice reference application](https://github.com/sformisano/type-history/blob/main/crates/examples/invoice-history/README.md)
demonstrates a current consumer with its frozen ledger.

Generated records require `Clone`, `PartialEq`, and `Debug` for their fields.
Generated `Debug` implementations delegate to each field's `Debug` implementation.
Backfill expressions and functions run in field declaration order.
Functions borrow the intact predecessor before unchanged fields move into the destination.
A callback may explicitly clone a value; transitions add no cloning pass.

A field has one lifetime. A retired wire name cannot be reused. Contextual
callbacks read the immediate predecessor; they cannot read partially computed
output or consume the whole payload once for several destination fields.
