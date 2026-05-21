# Wave Decisions: pulse-series-identity-v0 (DISCUSS)

British English. No em dashes.

## Configuration

| Decision | Value | Note |
|----------|-------|------|
| Feature type | Backend | Pulse is a library; no human-facing UI of its own. The operator-visible consequence surfaces one layer up in query-api / Prism, but the corrected behaviour lives entirely in the Pulse data model. |
| Walking skeleton | Yes | Ingest two metrics sharing a name but differing by `service.name`, query the name, get two distinct series each carrying its own `service.name`. One `@walking_skeleton` scenario against a real durable `FileBackedMetricStore`. |
| UX research depth | Lightweight | The persona (operator querying a multi-service metric) and the job were validated by the upstream discovery in `query-api-label-matchers-v0/deliver/upstream-issues.md`. No fresh UX research. |
| JTBD | No | Job grounding is inherited from the upstream-issues diagnosis and ADR-0042's read-side discipline. No fresh JTBD run (per task Decision 4: No). |

## Discovery grounding (DIVERGE absent)

No DIVERGE artifacts exist at `docs/feature/pulse-series-identity-v0/diverge/`. This is
expected: the feature was discovered DOWNSTREAM, during DELIVER of
`query-api-label-matchers-v0`. The job and the problem are documented with code-level
precision in `docs/feature/query-api-label-matchers-v0/deliver/upstream-issues.md`. Journey
work is grounded in that diagnosis rather than a re-run discovery conversation.

Risk noted: no independent JTBD validation of "tell two services apart" as a standalone
job. Mitigation: the problem is verified directly against the code (see below) and is a
correctness defect independent of any consumer; the standalone user value (one correct
series per service) is self-evident. LOW.

## Verified-against-code facts

1. **Series identity is name-only** (`crates/pulse/src/store.rs` line 144;
   `crates/pulse/src/file_backed.rs` line 303): the series map is
   `HashMap<(TenantId, MetricName), SeriesEntry>`. Two metrics sharing a name but differing
   only by `resource_attributes` collide on the same key.

2. **`resource_attributes` is overwritten on every ingest**
   (`crates/pulse/src/store.rs` line 161;
   `crates/pulse/src/file_backed.rs` line 318): `entry.metric.resource_attributes =
   metric.resource_attributes;`. The last-ingested batch's resource attributes win for the
   whole collapsed series. The inline comment is explicit that v0 treats
   `resource_attributes` as refreshable metadata, not as part of series identity.

3. **Consequence**: querying a metric emitted by checkout, cart, and search returns ONE
   series wearing whichever service ingested last. Per-service provenance is destroyed at
   ingest, before any query or matcher runs. This is wrong today, independent of label
   matchers.

4. **Point attributes are already per-point** (`crates/pulse/src/metric.rs`):
   `MetricPoint.attributes` is stored per point and is not collapsed. Only the
   metric-level `resource_attributes` is lost. This feature does NOT touch point
   attributes.

5. **The trait already returns multiple series**
   (`crates/pulse/src/store.rs` lines 77-82): `query` returns
   `Vec<(Metric, MetricPoint)>`, where each point carries its owning `Metric` (and thus its
   own `resource_attributes`). The shape already supports many series under a name. No trait
   signature change is needed; only the ingest keying and the query fan-out change beneath
   it.

6. **Durable recovery is append-and-sort** (`crates/pulse/src/file_backed.rs` lines 122-146):
   the WAL replays every `Ingest` record through the SAME `apply_ingest` used by the live
   path, then each series's point vector is re-sorted by `time_unix_nano`. Because live
   ingest and recovery share `apply_ingest`, the identity fix lands in one place and both
   paths inherit it. This is the property that lets a single change cover the in-memory
   store AND durable recovery.

## Relationship to ADR-0040

ADR-0040 (Beacon rule-state store seam) Decision 2 records the platform's two recovery
disciplines: **append-and-sort** (the storage pillars, where each WAL record is an event in
a time series and recovery re-sorts by `time_unix_nano`) versus **keyed-latest-wins**
(beacon, where a rule has one current state and the last `Put` per key wins). Pulse is an
append-and-sort pillar.

This feature does NOT change which discipline Pulse uses; it remains append-and-sort. The
change is to the SERIES KEY (full label set instead of metric name alone), which is
orthogonal to the latest-wins-vs-append-sort axis. The one place ADR-0040's framing is
relevant: the present `resource_attributes`-overwrite behaviour is a quiet, accidental
keyed-latest-wins applied to metadata WITHIN an append-and-sort series, which is precisely
the latent bug ADR-0040 warns against copy-pasting. **ADR-0040 is untouched** by this
feature. A new ADR for this feature (DESIGN wave) should record the key change and may cite
ADR-0040 Decision 2 as the framing for why metadata must not be latest-wins inside a series.

## Scope boundary held

- IN: series identity by full label set (`metric name` + `resource_attributes`) at ingest,
  in BOTH the in-memory store and the durable snapshot/WAL recovery; query returns every
  series under a name, each carrying its own correct `resource_attributes`.
- OUT, explicitly:
  - Label matchers. That is the dependent feature `query-api-label-matchers-v0`, already
    mid-flight and blocked on this. No matcher work here.
  - No new query language.
  - **No change to the public `MetricStore` trait signature.** `query` already returns
    `Vec<(Metric, MetricPoint)>`, shaped for multiple series. Verified at fact 5.
  - **No migration story.** The durable snapshot format may change. There is NO production
    Pulse data (Pulse is library-only, v0/v1 in-process, no daemon, per `lib.rs`), so a
    format change needs no migration or compatibility shim. Stated explicitly so DESIGN does
    not invent one.

## Peer review

Reviewer: `nw-product-owner-reviewer`. Max two iterations on rejection. To be run at
`*handoff-design`.
