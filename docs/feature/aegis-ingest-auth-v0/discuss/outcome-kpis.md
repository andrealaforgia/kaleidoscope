# Outcome KPIs — aegis-ingest-auth-v0

## Feature: aegis-ingest-auth-v0

### Objective

Within this feature's delivery, make the live aperture OTLP ingest path
enforce tenant authentication fail-closed, so every accepted record
carries a tenant proven by a validated bearer token and no record is ever
accepted without one — turning the platform's tenancy TYPES into tenancy
ENFORCEMENT at the boundary.

### Outcome KPIs

| # | Who | Does What | By How Much | Baseline | Measured By | Type |
|---|-----|-----------|-------------|----------|-------------|------|
| 1 | Clients ingesting OTLP to aperture | Accepted batches carry a tenant from a validated token | 100% of accepted ingest batches carry an authenticated `tenant_id` (3 signals × 2 transports) | 0% (no auth on the path; tenant is caller-claimed) | aperture audit `allow` events correlated with `sink_accepted` events | Leading (Outcome) |
| 2 | Unauthenticated / invalid-token callers | Are rejected with nothing stored | 100% of tokenless/invalid ingest requests rejected (`UNAUTHENTICATED`/`401`) with zero `sink_accepted` | 0% (all accepted) | reject status + absence of `sink_accepted` per rejected request | Leading (Outcome) |
| 3 | Operators triaging denials | Distinguish denial cause by reason | 100% of deny events carry exactly one of the 8 distinct aegis reasons; zero "unknown/other" bucket | n/a (no denials today) | distribution of the `reason` field across deny events | Leading (Secondary) |
| 4 | Operators deploying aperture | Never ship an unauthenticated ingest path by config omission | 100% of startups that would leave the path unauthenticated by omission exit non-zero with zero listeners bound | 0.0 (no such refusal exists) | exit code + `config_validation_failed` event + absence of `listener_bound` | Leading (Outcome) |

### Metric Hierarchy

- **North Star**: % of accepted ingest batches that carry an
  authenticated tenant id (KPI-1). The single number that says "the
  boundary is enforced". Target 100%; baseline 0%.
- **Leading Indicators**: reject-coverage of invalid requests (KPI-2);
  fail-closed-config refusal rate (KPI-4). Both predict the north star —
  if either drops, an unauthenticated record can slip through.
- **Guardrail Metrics** (must NOT degrade):
  - The existing authenticated-client accept latency and accept-response
    shape (non-regression — System Constraint 5). A correctly-
    authenticated client must ingest as fast and in the same shape as
    today.
  - Backpressure refusal behaviour (the concurrency cap), graceful-
    shutdown drain behaviour, and serve-loop death handling — unchanged.
  - Zero secret bytes in any log/event/error line (System Constraint 4) —
    a hard guardrail: any occurrence is a Critical defect.

### Measurement Plan

| KPI | Data Source | Collection Method | Frequency | Owner |
|-----|------------|-------------------|-----------|-------|
| 1 | aperture stderr JSON audit events | correlate `decision=allow` events with `sink_accepted` events per request | per request; rolled up per deploy | platform-architect (DEVOPS) |
| 2 | aperture audit + sink events | per rejected request, assert reject status AND no `sink_accepted` | per request | platform-architect |
| 3 | aperture deny events | tally `reason` field distribution | continuous; reviewed per incident | platform-security operator (Priya) |
| 4 | aperture exit code + `config_validation_failed` | startup-time observation | per startup | platform-architect |

### Hypothesis

We believe that wiring the correct `aegis::Validator` onto aperture's
ingest path, fail-closed, for the platform's clients will achieve a
gateway that accepts telemetry only under a proven tenant identity. We
will know this is true when 100% of accepted ingest batches carry an
authenticated tenant id (KPI-1), 100% of invalid/missing-token requests
are rejected with nothing stored (KPI-2), every denial reports a distinct
aegis reason (KPI-3), and no startup ships an unauthenticated ingest path
by omission (KPI-4) — with the existing authenticated-client happy path,
backpressure, and shutdown behaviours unchanged (guardrails).

### Handoff to DEVOPS

- **Data collection**: audit events already structured (aegis field
  contract `tenant_id`/`role`/`decision`/`subject`/`reason`); aperture's
  JSON-stderr subscriber captures them. DEVOPS instruments the
  allow↔`sink_accepted` and deny↔no-`sink_accepted` correlation.
- **Dashboards**: north-star authenticated-tenant coverage (KPI-1);
  reason-distribution panel for denials (KPI-3).
- **Alerting thresholds**: KPI-1 < 100% (any accepted batch without an
  authenticated tenant) is a Critical alert. Any secret-bytes-in-logs
  occurrence is a Critical alert (guardrail).
- **Baseline measurement**: none needed — baselines are 0% by
  construction (no auth on the path today).
