---
name: type-history-release-boundaries
description: Plan Type History freezing, released-ledger protection, service upgrades, and rollback around durable records and actual readers and writers.
---

# Type History release boundaries

Use this skill when a Type History change reaches durable storage or separately
deployed services. Read [lifecycle](https://github.com/sformisano/type-history/blob/main/book/src/lifecycle.md)
and [distributed upgrades](https://github.com/sformisano/type-history/blob/main/README.md#using-type-history-in-a-distributed-system)
at the revision matching the consumer's locked dependency. Documentation owns
commands, build settings, and API details.

## Protect retained history

Freeze a version before writing bytes that must survive later edits, including
development data that must be kept. A development build permitting a draft does
not make its bytes safe to retain. Use reset only when every affected record is
truly disposable; durable history needs a successor version.

For histories already released, compare working-ledger changes against an
independent ledger from a trusted release. Source and working ledger can agree
after a joint edit that breaks released history. The [released-baseline guidance](https://github.com/sformisano/type-history/blob/main/book/src/lifecycle.md#compare-with-a-released-ledger)
defines that comparison; the application or CI owns the baseline's trusted
origin. A copied current ledger supplies no independent release evidence.

## Follow the real delivery path

Identify every reader and writer of the affected history, including queued and
stored records. Count a service that both reads and writes in both roles. Its
dependency upgrade changes those roles together: current values are written at
the current head, and an old reader rejects versions absent from its crate.
Do not plan to create older-version data from an upgraded service. Keeping an
old producer means retaining its old dependency and deployment.

Before enabling new production, establish support in every receiving reader and
test that it interprets the new values correctly. Reading the new structure is
only part of readiness: totals, decisions, and conversion-error handling must
still have the intended meaning. The [consumer-first rollout](https://github.com/sformisano/type-history/blob/main/README.md#upgrade-consumers-before-producers)
applies where reader and writer upgrades can be separated. Coordinate services
with both roles, or use separate events when that suits the domain.

Plan rollback against versions already emitted. Reverting a producer leaves its
newer records in storage and queues. Every reader used during rollback must
still handle those versions; deploying an older binary does not erase them.
