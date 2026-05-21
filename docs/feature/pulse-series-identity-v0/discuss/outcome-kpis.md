# Outcome KPIs: pulse-series-identity-v0

British English. No em dashes.

These KPIs measure one thing: that a metric emitted by several services
is preserved as one correctly-labelled series per service, through
ingest and through a durable restart. The baseline is today's collapse,
where every multi-service metric returns one series wearing the
last-ingested service's labels.

## North Star

**Per-service provenance survives ingest and recovery.**

- **Who**: Pulse's ingest/query consumer (query-api, and the on-call
  operator behind it).
- **Does what**: ingests a metric emitted by several services under one
  tenant and queries it back as one correctly-labelled series per
  service, instead of one collapsed series.
- **By how much**: 100% of distinct `resource_attributes` under a shared
  metric name are preserved as distinct series; 0 series whose
  `resource_attributes` are overwritten by a later ingest.
- **Measured by**: the acceptance suite ingesting >= 2 services under one
  name and asserting each returned series carries its own
  `resource_attributes`, both on the live path and after a restart.
- **Baseline**: 0% today (every multi-service metric collapses to one
  series wearing the last-ingested service's labels).

## Correctness guardrails

| KPI | Target | Measured by | Baseline |
|-----|--------|-------------|----------|
| Distinct series preserved at ingest | 100% of distinct label sets under a name | US-01 acceptance scenarios | 0% (name-only keying collapses them) |
| No cross-service label overwrite | 0 overwrites | US-01: assert neither service's `resource_attributes` overwrites the other's | every later ingest overwrites today |
| Identical label set merges, not duplicates | 1 series, points ascending | US-01 boundary scenario (same label set, two ingests) | n/a (distinct series do not exist today) |
| Distinct series survive snapshot + reopen | 100% present, correctly labelled | US-02 snapshot-path scenario | n/a |
| Distinct series survive WAL-only reopen | 100% present, correctly labelled | US-02 WAL-replay scenario | n/a |
| `MetricStore` trait signature unchanged | 0 signature changes | code review + compile of existing consumers | n/a |
| Point attributes untouched | per-point, unchanged | US-01 edge scenario (point attrs do not split a series) | already correct |

## Learning hypothesis (feature level)

We believe that keying a series by its full label set (metric name +
`resource_attributes`), and applying it in the shared `apply_ingest`,
will make per-service provenance survive both live ingest and durable
recovery. We will know this is true when one acceptance run ingests
checkout then cart under one name and gets two distinct, correctly
labelled series back, and the same two series survive a snapshot+reopen
and a WAL-only reopen. We will know it is false if any path returns a
single collapsed series or re-merges the two on restart.
