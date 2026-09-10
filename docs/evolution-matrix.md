# Field-history evolution matrix

This matrix summarizes the [Typehistory contract](../README.md).
Typehistory originated in Axis's event field-history implementation.
The [Axis matrix at the extraction revision](https://github.com/sformisano/axis/blob/af7b96c3f0744cdfd170c50bb321d5dfd02be8ea/docs/design/event-evolution/evolution-matrix.md)
documents that revision's aggregate-specific cases.
S means supported, R means rejected, and U means outside the API.
A compound class carries two of those outcomes for one case; the rule column says which letter covers which half.
S/R means the case splits: the rule column names the supported half and the rejected half.
U/R means the API offers no such capability, and source that declares it anyway is rejected.
Case IDs identify behavior families; this document is not a test-run report.

## Presence, type, and conversion combinations

| ID | Case | Class | Rule / expected outcome |
|---|---|---|---|
| M01 | Unannotated V1 field | S | Present from default floor V1, declared type copied across unchanged versions |
| M02 | Unannotated imported-floor field | S | Present from explicit authorized retained_from; no fabricated lower type |
| M03 | Added ordinary field | S | added_at plus exactly one backfill expression or payload callback |
| M04 | Added Option<T> | S | Explicit backfill_value=None or callback; omission is an error |
| M05 | Added Default type | S | Explicit backfill_value=Type::default(); trait presence changes no macro rule |
| M06 | Added field with default omitted | R | Missing initializer fails even for Option/Default |
| M07 | Backfill required by current producer | R | Latest struct construction still requires every field; historical expression supplies no current producer default |
| M08 | Added field later changes type | S | Initializer targets birth type reconstructed from earliest later update's previous_type |
| M09 | One type update | S | updated_at, previous_type=previous type, exactly one value/payload callback |
| M10 | Repeated updates | S | Newest-first records reconstruct every intermediate type, including multi-version spans |
| M11 | Update keeps same type | S | Explicit previous_type=same type and callback executes semantic conversion |
| M12 | Type alias changes spelling only | S | Resolved same shape passes frozen checks; ordinary Rust checks callback compatibility |
| M13 | Retained nested/newtype wire shape edited in place | R | Resolved frozen shape fails; preserve old type, introduce new type and field update |
| M14 | Removed field | S | removed_at exclusive, no callback; declaration retained for older payloads |
| M15 | Last field removed | S | Latest named payload struct is empty; old value dropped after contextual callbacks |
| M16 | Field add -> many updates -> removal | S | One birth, strictly increasing semantic timeline, newest-first source records |
| M17 | Field rename | S | Old name removed and distinct new name added at same step; payload callback reads/clones old field |
| M18 | Two old fields merged | S | New destination added from payload; both old fields may be removed at same boundary |
| M19 | One old field split into two | S | Each destination has payload callback; both see intact predecessor, can call shared pure helper |
| M20 | Several dependent-looking outputs | S | Every callback reads old payload only; no callback reads partially produced destination |
| M21 | Value cast plus payload casts reading that same field | S | All shared reads finish before owned value cast/move |
| M22 | Fallible contextual addition | S | cast_from_previous_payload returns Result; first failure stops this conversion chain |
| M23 | Fallible value update | S | cast_from_previous_value consumes exact previous type and returns Result<destination,E> |
| M24 | Infallible contextual update | S | Function returns Ok(value) using supported error type |
| M25 | Fallible backfill expression | R | backfill_value is destination value, not Result; use payload callback for failure |
| M85 | Backfill expression uses return or ? | S/R | Typed zero-argument closure contains return; invalid Result propagation fails inside birth-value context and cannot escape transition |
| M26 | Unknown/wrong callback predecessor type | R | Generated unconditional transition does not satisfy ordinary Rust signature |
| M27 | Wrong callback output or borrowed output | R | Output must be owned exact destination field type |
| M28 | Callback error without the selected contract | R | Ordinary errors require Error + Send + Sync + 'static; redacted errors require ToRedacted |
| M29 | Non-Copy field | S | Owned unchanged/value fields move; contextual helpers explicitly clone if needed |
| M30 | Field missing a generated record trait | R | Retained fields require Clone and PartialEq; ordinary histories also require Debug, redacted histories require the selected field policy contracts |
| M31 | Opaque value split by consuming ownership once, without cloning | U | No whole-payload ownership/caching DSL; generated fields already Clone, helpers may clone explicitly |
| M32 | Cross-output computation requiring exactly-once shared side effect | U | Conversion code must be deterministic/pure; no framework shared cache or external transaction |

## Grammar, chronology, and identity combinations

| ID | Case | Class | Rule / expected outcome |
|---|---|---|---|
| M33 | Independently declared no-op version | U | No current header; creating a new draft requires a field boundary at N+1; retained intermediate identity steps remain supported |
| M34 | Completely empty record from floor | S | Empty payload inventory infers retained floor (V1 normally); version struct and JSON object decoding retained |
| M35 | One record declaration without current | S | Infer max(retained floor, all field-history boundaries); reject an explicit current key |
| M36 | Zero/overflow/malformed version identifier | R | Positive supported version integer required |
| M37 | Unauthorized non-V1 floor | R | Range validation and ledger/import authority reject it |
| M38 | Boundary beyond ledger-authorized successor or at/below retained floor | R | Cannot describe unretained transitions; unannotated fields already exist at floor |
| M39 | Two records at same version on one field | R | Addition/update/removal boundary conflict; combine business work in one legal callback |
| M40 | Records not newest first | R | Source chronology error points to field/boundaries |
| M41 | Update before birth or at/after removal | R | No active predecessor/destination interval |
| M42 | Multiple addition or removal records | R | One field lifetime only |
| M43 | Removed field later reintroduced with same name | U/R | Reserved historical Rust/wire name; parser rejects repeated birth/duplicate declaration |
| M44 | Business concept reintroduced under distinct field name | S | New added_at rule; prior field remains removed and readable historically |
| M45 | Raw identifier clashes with same canonical name | R | Canonicalize r# prefix before uniqueness and JSON key checks |
| M46 | Both cast forms or backfill plus cast | R | Exactly one assignment producer required |
| M47 | Added field uses cast_from_previous_value | R | No previous same-named field exists |
| M48 | Removed field supplies a callback/previous_type | R | Removal is structural only |
| M49 | Update omits previous_type or conversion | R | No inferred From/Into/default conversion |
| M50 | Unsupported history key or duplicate key | R | Closed attribute grammar with actionable diagnostic |
| M52 | Serde rename/default/flatten/skip/alias/with | R | Existing closed wire rules; field name is durable key |
| M53 | Conditional history/field declaration | R | Existing source inventory restrictions retained |
| M54 | Supported feature-selected alias | S | Selected compiler-resolved shape checked against same authority |
| M55 | self/super type, callback, expression, array length | S | Original invocation module and spans retained |
| M56 | Author field names that resemble helpers (`previous`, `value`, `json`, `__typehistory_*`, or `__axis_*`) | S | Generated identifiers hygienic; field names cannot shadow transition input/local bindings |
| M57 | Unsupported field attributes | R | Accept history, documentation, lints, and redaction policies on opted histories; reject other attributes |
| M58 | Per-version derives or Serde customization | U/R | Generated versions use the history's selected trait contract and strict codecs; unsupported attributes fail |
| M59 | Asynchronous or capturing-closure callback | R | Ordinary synchronous function path required |

## Freeze, draft, decode, and delivery combinations

| ID | Case | Class | Rule / expected outcome |
|---|---|---|---|
| M60 | Add draft field/update/removal | S | Earlier reconstructed frozen schemas unchanged; only current draft editable |
| M61 | Edit current declared type to a different wire shape without new previous_type record | R | Changes retained wire schemas; frozen const fails |
| M62 | Delete historical field or a previous_type record that changes reconstructed wire shape | R | Frozen wire shape/inventory mismatch |
| M63 | Reorder fields or rename equivalent alias | S | Same canonical wire shape; callback order follows new source order |
| M64 | Edit helper body, remove same-type update, or substitute wire-equivalent nominal type | S | Frozen ledger protects wire shapes and retained inventory; remaining field boundaries must preserve its head. Rust nominal identity/conversion semantics may change; ordinary typing still applies |
| M65 | Discard a latest draft update | S | Remove all draft boundaries/fields, restore prior declared types and adapt current callers; infer the prior head |
| M66 | Incomplete discard leaves draft annotation or wire-changing declared type | S/R | A remaining draft boundary keeps the draft alive and may pass development checks; removing its update but keeping a changed prior wire type fails frozen checks |
| M67 | Draft-only history declaration deleted | S | No frozen inventory reservation; dev bytes remain disposable |
| M68 | Frozen history declaration deleted | R | Full ordinary build inventory fails despite missing macro expansion |
| M69 | Second successor above an unfrozen draft | R | One latest ordinary draft policy |
| M70 | Freeze/reset/undo/import | S | Prior exact selectors, lock, checked snapshot, atomic ledger and reset reservations retained |
| M71 | Reset latest shape changes earlier generated history accidentally | R | Earlier frozen assertions still active |
| M72 | Delete reset declaration or skip its version | R | Retained reservation blocks build/release; exact undo/refreeze required |
| M73 | Dev draft vs release draft | S/R | Warning in dev; hard error in release family and strict dev |
| M75 | Decode old record requiring backfill despite current serde permissiveness | S | Decode exact old type then explicit adjacent addition; never try current first |
| M76 | Omitted Option key versus explicit null | S | Preserve existing exact-type serde behavior; omission may deserialize as None |
| M77 | Missing required field/unknown JSON key/invalid newtype | R | Exact historical decoder fails; `Versioned` returns the format's error, while low-level ordinary JSON errors retain their source |
| M78 | Duplicate JSON object key | R | Feed raw bytes to exact typed decoder, preserving existing duplicate-field rejection; do not preprocess into map/Value |
| M79 | Requested below-floor/future version | R | Unsupported version, no speculative current decode |
| M83 | Failed adjacent conversion | R | Preserve stored version and failing step; ordinary errors retain the original source, redacted errors retain only the safe projection |

## Optional Redactable combinations

| ID | Case | Class | Rule / expected outcome |
|---|---|---|---|
| M100 | Feature disabled | S | Ordinary consumers resolve no Redactable package |
| M101 | Feature enabled, ordinary history | S | Same ordinary record traits and original callback/JSON error sources |
| M102 | Plain and redacted consumer libraries in one graph | S | Feature unification never strengthens plain callback bounds |
| M103 | Explicit `redacted` history | S | All retained records and current alias have fixed masked Debug and structural redaction |
| M104 | Policy without explicit opt-in or required capability | R | Diagnostic; no requested policy is ignored |
| M105 | Invalid policy or missing inherited Redactable | R | Declaration or trait failure, including predecessor field types |
| M106 | Nested type with raw or panicking Debug | S | Generated redacted Debug never reads fields |
| M107 | Add, convert, or remove a policy-bearing field | S | Policy follows its retained lifetime and is checked for every retained type |
| M108 | Edit formatting policy only | S | Revalidate policies without changing frozen wire schemas |
| M109 | Redacted callback or low-level JSON failure | R | Preserve identity and version context; capture only safe causes and terminate raw source chains |
| M110 | Redactable wrapper or nested history field | S | Core-owned transparent schema adapters and generated nested schema contracts |
| M111 | Renamed Typehistory without direct Redactable | S | Generated records use runtime reexports |
| M112 | Upstream Redactable derive on supporting record | S | Requires direct Redactable 0.12.0 dependency; renamed dependency supported |
| M113 | Frontend-owned Axis error projection | S | Generic GenerationPaths.error constructors receive concrete owned callback errors |

## Versioned values and stable identity

| ID | Case | Class | Rule / expected outcome |
|---|---|---|---|
| M120 | `stable_id` declaration | S | Exact identity stays unchanged after freezing, including across Rust renames and module moves |
| M121 | Former `id` argument | R | Change the argument spelling to `stable_id`; preserve the identity value and existing ledger |
| M122 | Current value serialized as `Versioned<T>` | S | `into_versioned` stamps the current identity and version; callers cannot replace metadata independently |
| M123 | Historical `Versioned<T>` converted through current alias | S | Deserialization selects the exact retained payload; `from_versioned` applies adjacent conversions with source context |
| M124 | Historical envelope serialized without conversion | S | Keep its historical version and matching data; wrap the converted current value to emit the latest version |
| M125 | Empty envelope, extra map entries, wrong identity, or invalid version | R | Two single-entry maps are required; check identity and supported version before decoding payload fields |
| M126 | JSON or MessagePack with named fields | S | Serde reads maps directly through the exact historical codecs; no generic value buffer |
| M127 | Format using payload struct arrays | U | The generated payload decoder requires maps; support depends on the format and field types |
| M128 | Redacted `Versioned` parse failure | R | Discard raw errors inside `Deserialize`; serializer and external trailing-input errors remain outside that boundary |
| M129 | Raw JSON supplied as a versioned envelope | R | Existing raw JSON needs the low-level decoder and separately stored metadata |

## Verification and limits

Parser tests live in [codegen](../crates/typehistory-codegen/src/history/tests.rs).
Real standalone compiler and lifecycle cases live in
[cargo-typehistory](../crates/cargo-typehistory/tests/standalone.rs). Its
[tutorial tests](../crates/cargo-typehistory/tests/standalone/tutorial.rs) execute
the README progression. The [invoice reference application](../examples/invoice/README.md)
demonstrates a current consumer with its frozen ledger.

Ordinary generated records require `Clone`, `PartialEq`, and `Debug` for their fields.
Opted redacted records require `Clone`, `PartialEq`, and the selected field policy contracts;
their fixed `Debug` has no field `Debug` bound. See the [optional guide](redactable.md).
Transitions borrow the
intact predecessor for contextual callbacks, then move values into the destination.
A callback may explicitly clone a value; transitions add no cloning pass.

A field has one lifetime. A retired wire name cannot be reused. Contextual
callbacks read the immediate predecessor; they cannot read partially computed
output or consume the whole payload once for several destination fields.
