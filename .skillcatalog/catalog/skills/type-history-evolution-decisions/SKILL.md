---
name: type-history-evolution-decisions
description: Choose truthful Type History evolution and backfills when retained records must become current values or an event's meaning changes.
---

# Type History evolution decisions

Use Type History when retained facts outlive their Rust definitions and the
application needs those facts as current records. Choose an evolution only when
you can explain what each older value means after conversion.

Read the [usage guide](https://github.com/sformisano/type-history/blob/main/README.md#how-to-use)
and [field history](https://github.com/sformisano/type-history/blob/main/book/src/field-history.md)
at the revision matching the consumer's locked dependency. Documentation owns
attributes, generated names, types, and examples; this skill guides the decisions
behind them.

## Preserve the facts

Judge a backfill against the records it will read. Derive values from facts
actually present in the previous record, or from an established rule that held
for every affected record. A constant is justified by that historical rule,
not by today's default. A current constructor default supplies no evidence
about older data.

When the old record cannot establish a new fact, preserve an explicit unknown,
make any accepted loss clear, or return a conversion failure. Do not invent a
currency, timestamp, identifier, or other fact to make the new type convenient.
Keep replay deterministic: the same retained facts must produce the same meaning
without depending on today's clock, settings, or mutable external data.

Choose conversions that preserve meaning across the complete path to the current
type. A rename, split, merge, or changed Rust type needs a domain explanation;
successful compilation alone does not establish one. The
[payment example](https://github.com/sformisano/type-history/blob/main/crates/examples/invoice-history/src/payment.rs)
derives paid currency from the previous record and fails for an unsupported code.

## Keep one history for one fact

A new version can change how the same fact is represented, including a change
that keeps the Rust type. If the change introduces a distinct domain event,
consider its own history and delivery path. The
[refund example](https://github.com/sformisano/type-history/blob/main/README.md#use-a-separate-event-for-refunds)
shows this choice. Separate events still need coordinated readiness when a
consumer's result depends on both facts.
