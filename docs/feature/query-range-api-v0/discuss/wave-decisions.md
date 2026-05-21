# Wave Decisions: query-range-api-v0 (DISCUSS)

British English throughout. No em dashes.

## Configuration (decided, not asked)

| # | Decision | Value | Implication |
|---|----------|-------|-------------|
| 1 | Feature type | Backend | HTTP service, no TUI/web mockups; contract-first |
| 2 | Walking skeleton | No (brownfield) | Pulse store, aegis tenancy, gateway pattern all exist; this is the read half of an existing write loop |
| 3 | UX research depth | Lightweight | Primary "user" is Prism's HTTP client (machine consumer) plus the operator behind it; emotional arc kept brief |
| 4 | JTBD | No | No diverge artifacts; job grounded informally in the read-loop closure |

## DIVERGE artifacts

None present at `docs/feature/query-range-api-v0/diverge/`. No prior JTBD run.
Risk noted: job statement is grounded informally in the platform read-loop narrative
and Prism's pinned contract rather than a validated ODI job analysis. Low risk for
this feature because the contract is externally pinned (Prism's own validator and
ADR-0027 lock the response shape), removing most requirement ambiguity.

## The contract is pinned, not discovered

The defining constraint of this feature: the response shape is NOT open for design.
`apps/prism/src/lib/promql/queryRange.ts` and ADR-0027 lock both the request the
client builds and the response shape its validator (`isPromSuccess` / `isPromError`)
accepts. DISCUSS treats this as a fixed external contract and writes requirements
against it verbatim. Any deviation is a defect, not a design choice.

## Key decisions taken in DISCUSS

1. **Scope is metrics-only.** `query_range` returns `resultType: 'matrix'` which is a
   range vector / time series. Time series = metrics. Logs and traces are different
   Prism panels and different endpoints; explicitly out of scope (see story-map.md
   Scope Assessment and US-05 Won't-Have list).

2. **No full PromQL engine.** Prism sends a raw PromQL string in the `query` param
   (confirmed: `queryRange.ts` builds `URLSearchParams({ query: request.q, ... })`).
   Slice 01 parses only the MINIMAL selector Prism needs to render something real:
   a bare metric-name selector, optionally narrowed by a single `{label="value"}`
   matcher (slice 02). Operators, functions, aggregations, `rate()` etc. are deferred
   to v1. This boundary is explicit in the scope assessment.

3. **Tenant supply is the riskiest open question.** Pulse is per-tenant
   (`query(&TenantId, &MetricName, TimeRange)`). The gateway resolves tenancy on the
   WRITE path via `KALEIDOSCOPE_DEFAULT_TENANT` (fail-closed) plus per-record
   `tenant.id`. Aegis provides `TenantId` (newtype) and a JWT validator carrying a
   `tenant_id` claim with two roles (viewer/operator). Prism's client forwards
   `backend.headers` (ADR-0027 §6) and redacts them, so it CAN carry an auth/tenancy
   header but does not pin its name. This is surfaced as RED CARD 1 and as a key
   requirement in US-04. DISCUSS does not pin the mechanism; DESIGN decides. The
   recommended default for slice 01 (to keep the walking read slice thin) is a single
   configured tenant resolved the same way the gateway resolves its default
   (`KALEIDOSCOPE_QUERY_TENANT` env, fail-closed), with a header-based path
   (`X-Scope-OrgID`, the Mimir/Cortex convention, or an aegis Bearer token) deferred
   to a later slice. DESIGN owns the final choice.

4. **Where it lives is NOT decided here.** Likely a new crate/binary depending on
   pulse + aegis, exposing HTTP. Flagged for DESIGN; not pinned (constraint honoured).

## Red cards (open questions for DESIGN / clarification)

- RED CARD 1 (tenant supply): which mechanism resolves the tenant for a query
  (configured single tenant vs `X-Scope-OrgID` header vs aegis Bearer token)?
  Recommended slice-01 default: configured single tenant, fail-closed. Owner: DESIGN.
- RED CARD 2 (step / alignment): Prism sends `step=15s` and start/end as float
  epoch SECONDS. Pulse stores `time_unix_nano` (u64 nanoseconds). The matrix Prism
  expects has `values: [[ts_seconds, "value"], ...]`. Slice 01 returns the raw stored
  points mapped to `[seconds, stringified value]` WITHOUT step-resampling (Prism's
  chart tolerates irregular spacing; `connectNulls:false`, `smooth:false`). True
  step-aligned resampling is deferred. Owner: DESIGN to confirm raw-points is
  acceptable for v0; the Prism validator does not require regular spacing.
- RED CARD 3 (matrix grouping): Pulse `query` returns `Vec<(Metric, MetricPoint)>`
  where points carry per-point `attributes`. A Prometheus matrix groups points into
  series by their full label set. Slice 01 must decide the grouping key (resource
  attributes + point attributes). Recommended: one series per distinct merged label
  set. Owner: DESIGN.

## Risk register

| Risk | Prob | Impact | Mitigation |
|------|------|--------|------------|
| Tenant mechanism mismatch with platform | Med | High | RED CARD 1; mirror gateway fail-closed default; DESIGN decides |
| Response shape drift from Prism validator | Low | High | Round-trip KPI against Prism's own `isPromSuccess`/`isPromError`; contract test |
| Scope creep into full PromQL | Med | High | Explicit scope boundary; selector grammar frozen at bare-name + single matcher |
| seconds/nanoseconds unit error | Med | Med | RED CARD 2; explicit conversion AC + boundary example |
