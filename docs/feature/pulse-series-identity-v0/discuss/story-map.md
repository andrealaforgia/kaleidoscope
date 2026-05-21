# Story Map: pulse-series-identity-v0

## User: An operator (and the query-api consumer behind them) querying a metric emitted by several services
## Goal: See one correctly-labelled series per service under a shared metric name, instead of a single collapsed series wearing the last-ingested service's labels

British English. No em dashes.

## Backbone

| Ingest a metric | Identify its series | Persist durably | Query the name |
|-----------------|---------------------|-----------------|----------------|
| Accept a batch for a tenant | Key by full label set (name + resource_attributes) | Append to WAL / fold into snapshot | Return every series under the name |
| Same name, different `service.name` | Two distinct services stay distinct | Recovery rebuilds the same distinct series | Each series carries its own `resource_attributes` |
| (point attributes already per-point) | (no overwrite of resource_attributes) | (append-and-sort, unchanged discipline) | (trait shape unchanged) |

This feature corrects the **Identify its series** rib (and the **Persist durably** rib's
recovery, which shares the same `apply_ingest`). The ingest and query entry points already
exist; their plumbing is corrected, not added.

---

### Walking Skeleton

US-01 alone is the walking skeleton: ingest two metrics with the same name differing by
`service.name` into a real `FileBackedMetricStore`, query that name, and get two distinct
series back, each with its own `service.name` intact. One `@walking_skeleton` scenario,
real durable Pulse, demonstrable in a single session. It is the thinnest slice that proves
the identity correction end-to-end (ingest -> identity -> in-memory query).

### Release 1 (this feature): a metric is identified by its full label set

- US-01 (walking skeleton): two same-named metrics differing by `service.name` ingest as
  two distinct series; querying the name returns both, each with its own
  `resource_attributes`. In-memory and live-ingest path.
- US-02: the two distinct series SURVIVE a snapshot and reopen (durable recovery preserves
  per-series identity, not just the live path).

Target outcome: an operator querying a multi-service metric sees one correct series per
service. KPI: North Star + Correctness (see outcome-kpis.md).

### Deferred (NOT in this feature scope)

- Label matchers (`query-api-label-matchers-v0`): the dependent feature, resumes once this
  ships.
- Histogram / exponential histogram / summary point types: unchanged from the existing v1
  roadmap.
- Resource-attribute hoisting to batch level (mentioned as a future idea in
  `metric.rs`): out of scope; not required for correct identity.

## Priority Rationale

Priority by outcome impact and dependency, not technical layer. Both stories share one
change to series identity (the keying in `apply_ingest` and the in-memory `ingest`); they
are sliced by user-verifiable outcome.

1. **US-01 distinct series at ingest + query** (P1, Must) - the headline correctness fix and
   the walking skeleton. Without it the read side returns one arbitrary service's labels for
   all points. Derisks the core assumption: does keying by full label set actually keep two
   same-named services apart through ingest and query? It is the prerequisite the dependent
   matcher feature is blocked on.
2. **US-02 distinct series survive recovery** (P1, Must) - the durable angle. The fix lands
   in `apply_ingest`, which both live ingest and WAL replay share, so US-02 should fall out
   of US-01; but recovery is an independently verifiable behaviour (a snapshot + reopen is a
   different code path from a live query) and the durable store is the realistic invocable
   surface, so it earns its own story and scenario. Must ship with US-01: a fix that holds
   live but not across restart would be a half-fix on a durable store.

Deferred (Won't-Have this feature): label matchers, new point types, resource-attribute
hoisting.

## Scope Assessment: PASS - 2 stories, 1 module touched (crates/pulse/src; aegis reused unchanged), estimated 1-2 days

Oversized signals checked (none tripped at the 2+ threshold):

- User stories: 2 (<= 10). PASS.
- Bounded contexts/modules: only `crates/pulse/src/` changes (`store.rs` ingest keying,
  `file_backed.rs` `apply_ingest` + snapshot/recovery; `metric.rs` may gain a derived series
  key, a DESIGN call). aegis reused unchanged. 1 (<= 3). PASS.
- Walking skeleton integration points: none new; the existing ingest/query/WAL/snapshot
  seams are reused. 0 new (<= 5). PASS.
- Estimated effort: 1-2 days, two right-sized stories of <= 1 day each. PASS.
- Independent shippable outcomes: one (correct per-service series identity). PASS.

Right-sized. No split required. Each story is <= 1 day with a learning hypothesis.
