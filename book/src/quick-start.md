# A type's journey

This example follows a shop's `ReceiptCreated` event type as its fields change.
The shop initially accepts only US dollars, so the event starts with an amount
in cents. Later, the shop adds other currencies and supports refunds. At each
step, we keep the receipts already in storage readable by the current code.

Complete the [package setup](setup.md) first. Then replace `src/lib.rs` with each
declaration below as the example advances. The reading and writing snippets belong
in a function or test that returns a compatible `Result`.

## Evolve the event

{{#include ../../README.md:journey}}

[formats]: decoding.md#choose-a-serde-format

Continue with [current types and historical data](core-model.md) to see how the
wrapper can hold V1 even when `ReceiptCreated` names V3.
