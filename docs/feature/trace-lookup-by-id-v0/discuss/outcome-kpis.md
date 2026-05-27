# Outcome KPIs: trace-lookup-by-id-v0

## Feature: trace-lookup-by-id-v0

### Objective

Close the by-id read shape on the traces pillar: an operator with a
`trace_id` in hand GETs the trace lookup endpoint and sees that
trace's spans, scoped to their tenant, served from the durable ray
store, in one HTTP call, without naming a service or estimating a
window. A thin parse-and-wire growth on top of `ray-query-api-v0` and
the existing `crates/trace-query-api` crate; the ray substrate is
unchanged.

### Outcome KPIs

| # | Who | Does What | By How Much | Baseline | Measured By | Type |
|---|-----|-----------|-------------|----------|-------------|------|
| 1 | On-call operator (and the future prism trace client) | Pivots from "I have a trace_id" to "here are the spans for this trace, for my tenant" in one HTTP call | From "no by-id read path exists; operator must estimate a window and filter client-side" to "a GET with the trace_id returns exactly that trace's spans for the tenant from the durable store" | 0% (only the window+service shape exists; no by-id shape) | E2E: seed two trace_ids, GET the lookup endpoint with one, assert only that trace's spans return | Leading (Outcome) |
| 2 | Trace lookup endpoint | Returns the trace's `Span`s carrying every field without loss or rename | 100% of `Span` fields (`trace_id`, `span_id`, `parent_span_id`, `name`, `kind`, start/end times, `status`, `attributes`, `resource_attributes`, `events`, `links`) round-trip; spans in ascending `start_time_unix_nano` order | n/a (the existing window arm already proves the field-fidelity property; this KPI tracks the same on the new arm) | Field-fidelity acceptance test asserting each field survives the round-trip on the lookup arm | Leading (Outcome) |
| 3 | Trace lookup endpoint | Reads the durable store within a latency budget for a representative trace | p95 lookup latency at most 200 ms on GitHub Actions ubuntu-latest for a trace of <= 1000 spans | No lookup arm exists | Timed acceptance test in CI (ubuntu-latest), cross-checked with the store's own `record_query` recorder | Leading (Secondary) |
| 4 | Trace lookup endpoint | Refuses to serve when no tenant resolves and never leaks across tenants on the by-id key | 100% of no-tenant lookups refused (401); 0 cross-tenant span leaks on the `(tenant, trace_id)` key | n/a | Tenant-isolation acceptance tests on the lookup arm (no-tenant fixture; two-tenant-same-trace_id fixture); `FailingTraceStore` double assertion that no `get_trace` call is made on the 401 path | Guardrail |
| 5 | Trace lookup endpoint | Rejects malformed `trace_id` and never echoes the raw value | 100% of non-hex / wrong-length / missing trace_ids return 400; 0 raw values echoed; 0 "SECRET" / "Bearer" occurrences in any error body; 0 store calls on the 400 path | n/a | Redaction acceptance tests on the lookup arm; `FailingTraceStore` double assertion that no `get_trace` call is made on the 400 path | Guardrail |

### Metric Hierarchy

- **North Star**: KPI 1 - the operator pivots from a trace_id to the
  trace's spans in one HTTP call (the by-id arm closes).
- **Leading Indicators**: KPI 2 (fields round-trip on the new arm),
  KPI 3 (latency within budget on the new arm).
- **Guardrail Metrics**: KPI 4 (tenant fail-closed and zero cross-
  tenant leak on the lookup arm; both the "no tenant" and the
  "wrong tenant" paths); KPI 5 (redaction and no-store-on-400). KPI 2
  (field fidelity) must NOT regress below 100%.

### Measurement Plan

| KPI | Data Source | Collection Method | Frequency | Owner |
|-----|------------|-------------------|-----------|-------|
| 1 | E2E seed -> lookup -> read (tower oneshot) | CI acceptance stage | Per commit | DELIVER |
| 2 | Field-fidelity acceptance test on the lookup arm | CI acceptance stage | Per commit | DELIVER |
| 3 | Store-emitted query duration + CI timing | Timed test on ubuntu-latest | Per commit | DEVOPS/DELIVER |
| 4 | Tenant-isolation acceptance tests on the lookup arm | CI acceptance stage | Per commit | DELIVER |
| 5 | Redaction acceptance test + `FailingTraceStore` assertion | CI acceptance stage | Per commit | DELIVER |

### CI realism

All latency budgets are stated against GitHub Actions `ubuntu-latest`,
not developer hardware. KPI 3's 200 ms p95 is a CI-runner budget for
a trace of up to ~1000 spans against the file-backed store; it is
tighter than the window arm's 500 ms because the by-id key is a
direct hashmap lookup (`state.by_trace.get(&key)` at
`crates/ray/src/store.rs:189`), not a window filter over a service
bucket. Revisit if representative trace shapes grow. Per project
memory: Kaleidoscope is pure trunk-based, CI is feedback not a gate,
so these KPIs are correctness signals, not merge blockers.

### Hypothesis

We believe that an HTTP endpoint reading
`TraceStore::get_trace(&tenant, &trace_id)` over the durable ray
store, for the on-call operator with a `trace_id` in hand, will close
the by-id arm of the traces pillar without changing the ray
substrate. We will know this is true when the operator looks up a
trace by id and reads exactly that trace's spans (KPI 1) with every
field intact (KPI 2 = 100%), within the CI latency budget (KPI 3),
with tenant fail-closed and zero cross-tenant leak (KPI 4) and with
the redaction posture honoured on the 400 arm (KPI 5). The open
questions are the URL shape (FLAG 1) and the exact trace_id parse
rule (FLAG 2); the KPIs hold under whichever options DESIGN chooses.

## Handoff to DEVOPS (platform-architect)

- Instrument: per-lookup duration (already a ray recorder seam:
  `record_query` is reused by `get_trace` via the in-memory adapter
  at `store.rs:190`; verify the durable adapter records the same
  signal), returned span count, tenant-resolution outcome (resolved
  vs refused), `trace_id` parse outcome (parsed vs rejected; do NOT
  log the raw value).
- Dashboards: trace-lookup p95 latency, lookup empty-vs-non-empty
  ratio, refused-request rate, malformed-trace_id rate.
- Alerting thresholds: KPI 3 p95 > 200 ms; any cross-tenant leak (KPI
  4) is a hard alert; any raw `trace_id` value appearing in error
  responses (KPI 5) is a hard alert.
- Baseline: none needed; this is a greenfield by-id arm (baseline is
  "no lookup arm exists") on an existing crate.
