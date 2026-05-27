# Outcome KPIs: log-query-severity-filter-v0

## Feature: log-query-severity-filter-v0

### Objective

By the close of slice 01, an on-call SRE (or an automated alerting client)
can ask `GET /api/v1/logs` for "WARN or worse" with one optional query-string
parameter and have the platform return only the matching records, in the
same JSON shape as today. The default (no parameter) behaves exactly as
before. The error envelope on a malformed value is the existing redaction-
preserving 400. No lumen trait change. No cap change. Slice <= 1 day.

### Outcome KPIs

| # | Who | Does What | By How Much | Baseline | Measured By | Type |
|---|---|---|---|---|---|---|
| KPI-1 | SRE operators and automation clients on tenants with INFO-heavy log traffic | Issue narrowed reads against `/api/v1/logs` using the new parameter, receiving only at-or-above-floor records | At least 5x reduction in response payload bytes against a representative INFO-heavy fixture (60s window, 80% INFO, 20% WARN+ERROR) when `<min_severity_param>=WARN` is supplied vs absent | 100% of in-window records returned today (no filter exists in the HTTP boundary) | Seeded-fixture acceptance assertion in `crates/log-query-api/tests/slice_01_severity_filter.rs`: byte-length comparison of the two response bodies | Leading (outcome) |
| KPI-2 | Existing log-query-api clients (e.g. Marcus's hourly alerting script, any current `curl` user, any prism log panel when it lands) | Continue issuing parameter-less GETs and receive the same response as the slice-prior tag | 0 broken clients: 100% of slice-prior acceptance scenarios in `tests/slice_01_logs_read.rs` and `tests/slice_02_caps.rs` continue to pass on the new build | Today: 100% pass on the slice-prior tag | Existing acceptance suites continue green after the slice ships; no test deletion, no test rewrite (only ADDITIONS in the new `slice_01_severity_filter.rs`) | Leading (guardrail) |
| KPI-3 | SRE operators submitting a malformed severity value (typos, non-OTel names) | Receive a redacted 400 in the existing envelope, never a 500, never a leaked raw parameter value, never a silent empty success | 100% of unknown-severity requests on the acceptance fixture yield HTTP 400 + `{"status":"error","error":"unknown severity"}` + body does NOT contain the raw parameter value | Today: no parameter exists, so 100% of unknown-severity behaviour is undefined | Acceptance assertion in `tests/slice_01_severity_filter.rs`: status, envelope shape, and redaction substring test on the unknown-severity 400 arm | Leading (guardrail) |

### Metric Hierarchy

- **North Star (KPI-1)**: response-payload reduction on narrowed reads.
  The slice's whole point is "operator asks for less, server delivers less".
  If KPI-1 does not move, the slice has no value regardless of what else
  ships.
- **Leading indicators**:
  - Narrowed-read adoption: count of `/api/v1/logs` requests carrying the
    new parameter, vs total. (Not instrumented at slice 01; the slice ships
    no new metric per ADR-0050 Decision 8 posture — the platform has no live
    observability of its own at v0/v1. Recorded as a follow-up.)
  - Average post-filter record count vs pre-filter record count on the same
    window-and-tenant pair. Not instrumented at slice 01; recorded as a
    follow-up.
- **Guardrail metrics (KPI-2, KPI-3)**:
  - Backward-compat: pre-existing acceptance suites stay green; pre-existing
    response bytes are byte-equal on the no-parameter path.
  - Redaction: the new 400 arm honours ADR-0047 Decision 1 + ADR-0050
    Decision 7 (no raw parameter value, no forwarded credential).
  - Cap preservation: `MAX_WINDOW_SECONDS` and `MAX_RESULT_ROWS` constants
    unchanged; existing cap acceptance scenarios green.

### Measurement Plan

| KPI | Data Source | Collection Method | Frequency | Owner |
|---|---|---|---|---|
| KPI-1 | Seeded fixture in the new acceptance test | Direct byte-length assertion on two response bodies (with and without the new parameter, same window, same tenant) | On every CI run (existing per-feature mutation + acceptance gate) | crafter (DELIVER wave) |
| KPI-2 | Existing acceptance suites (`tests/slice_01_logs_read.rs`, `tests/slice_02_caps.rs`) | CI green gate on the pre-existing suites; no test deletion, no test rewrite | On every CI run | crafter (DELIVER wave) |
| KPI-3 | New acceptance test in `tests/slice_01_severity_filter.rs` | Status code + envelope shape + redaction substring assertions on the unknown-severity 400 arm | On every CI run | crafter (DELIVER wave) |

No new dashboard. No new metric counter. No new tracing event beyond the
existing `tracing::error!` calls. This is consistent with ADR-0050 Decision
8: at v0/v1 the platform has no live observability stack of its own; a
contract-shaped outcome IS the signal.

### Hypothesis

We believe that exposing the existing `lumen::Predicate::min_severity`
through one optional query-string parameter on `GET /api/v1/logs`, for SRE
operators and automation clients on INFO-heavy tenants, will produce at
least a 5x reduction in response payload on narrowed reads while keeping
every existing client byte-equal on the default path. We will know this is
true when (a) the new acceptance fixture's narrowed-read response is at
least 5x smaller than the same fixture's parameter-less response, (b) every
pre-existing acceptance scenario in `tests/slice_01_logs_read.rs` and
`tests/slice_02_caps.rs` stays green, and (c) every unknown-severity input
yields a redacted 400 in the existing envelope.

### Handoff to DEVOPS

No instrumentation requested at slice 01. The KPIs above are CI-test-fixture
measured (KPI-1) and CI-gate enforced (KPI-2, KPI-3). A successor slice may
add narrowed-read-adoption counters and post-filter record-count histograms
once the platform has a live observability stack of its own; that is OUT of
this slice's scope and recorded as a forward-looking item.
