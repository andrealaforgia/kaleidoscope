# Outcome KPIs — spark-ingest-auth-v0

> Synthesises Gothelf/Seiden (Who-Does-What-By-How-Much), Maurya
> (actionable not vanity), Doerr (measurable Key Results). The feature
> is the client-side sibling of `aegis-ingest-auth-v0` (ADR-0068): the
> gateway demands a bearer token; this feature lets the Spark SDK send
> one. The lagging impact is the verifier's E01-E04 returning GREEN.

## Feature: spark-ingest-auth-v0

### Objective

Give the Spark SDK a key that fits the gateway's lock — so an integrator
can ship authenticated telemetry that is ACCEPTED across all three OTLP
signals, with the credential supplied (programmatically or via the
standard env var) and never leaked, unblocking the Spark→Aperture
round-trip (E01-E04) by end of this wave's delivery.

### Outcome KPIs

| # | Who | Does What | By How Much | Baseline | Measured By | Type |
|---|-----|-----------|-------------|----------|-------------|------|
| 1 | Integrators exporting to an authenticated gateway | ship authenticated telemetry ACCEPTED across all three signals (traces, logs, metrics) by configuring a bearer token | 100% of the three signals carry the configured bearer when a token is set; E01-E04 round-trip returns GREEN | 0% — no auth knob; every signal denied `missing_claim`; E01-E04 BLOCKED | E01-E04 suite + gateway `decision=allow`/`sink_accepted` for each `ingest_*` correlated with a Spark export | Leading (Outcome) |
| 2 | Integrators managing the credential via deployment manifest | authenticate the SDK by setting `OTEL_EXPORTER_OTLP_HEADERS`, no application rebuild | 100% of exports with a valid `authorization=Bearer%20<jwt>` env var accepted; credential rotation needs 0 rebuilds | 0% — Spark ignores `OTLP_HEADERS` (F3); env-set credential silently dropped | gateway `decision=allow` for an env-var-only token; precedence test when both paths set | Leading (Outcome) |
| 3 | Integrators shipping the SDK into log-aggregated services | present a gateway credential without ever leaking it into logs | 0 occurrences of a token value on any Spark log/`Debug`/error surface (a single occurrence is a defect) | n/a — no token field exists today | redaction test grepping every Spark log/Debug/error surface for the configured token | Guardrail |
| 4 | Integrators targeting unauthenticated collectors | keep the no-auth path working unchanged when no token is configured | 100% of no-token exports to an unauthenticated endpoint still accepted; slice_01..07 stay green | 100% today (no auth exists) — must not regress | no-token negative-control export test against an unauthenticated collector + existing slice suite | Guardrail |

### Metric Hierarchy

- **North Star**: KPI-1 — authenticated three-signal export ACCEPTED
  (the E01-E04 round-trip GREEN). This is the one metric that proves the
  key fits the lock for the whole ingest surface.
- **Leading Indicators**: KPI-2 (the conventional env-var path also
  delivers an accepted credential), which predicts adoption by
  deployment-managed teams.
- **Guardrail Metrics**: KPI-3 (zero token leak — the load-bearing
  security guardrail, must hold from the first slice that adds the
  field) and KPI-4 (the no-auth path must not regress — an unauthenticated
  local collector must keep accepting).

### Measurement Plan

| KPI | Data Source | Collection Method | Frequency | Owner |
|-----|-------------|-------------------|-----------|-------|
| 1 | E01-E04 round-trip harness + aperture audit stream | assert accept-with-tenant for each of traces/logs/metrics when a token is configured | per CI run (DISTILL acceptance + the verifier's E-suite) | acceptance-designer / verifier |
| 2 | env-var-only export test + precedence test | set `OTEL_EXPORTER_OTLP_HEADERS`, assert accept; set both paths, assert programmatic wins | per CI run | acceptance-designer |
| 3 | Spark log/Debug/error capture | configure a recognisable token, grep every Spark surface for its value, assert absent | per CI run (mutation-anchored) | software-crafter / acceptance-designer |
| 4 | unauthenticated-collector export test + slice_01..07 | no token configured, assert accept against an unauth endpoint; existing slices green | per CI run | software-crafter |

### Hypothesis

We believe that adding a bearer-token knob (`with_bearer_token` plus the
`OTEL_EXPORTER_OTLP_HEADERS` env path) for integrators exporting to an
authenticated Kaleidoscope gateway will achieve an accepted,
authenticated three-signal round-trip. We will know this is true when
integrators ship telemetry that is ACCEPTED with the token's tenant
across traces, logs, and metrics (KPI-1, E01-E04 GREEN) — with zero
token leaks (KPI-3) and no regression of the no-auth path (KPI-4).

## Smell Tests

| Check | KPI-1 | KPI-2 | KPI-3 | KPI-4 |
|-------|-------|-------|-------|-------|
| Measurable today? | Yes — E01-E04 harness + audit stream | Yes — env-var + audit | Yes — log grep | Yes — slice suite |
| Rate not total? | Yes — % of signals carrying the bearer | Yes — % accepted | Yes — occurrence count, target 0 (defect gate) | Yes — % still accepted |
| Outcome not output? | Yes — telemetry ACCEPTED, not "knob shipped" | Yes — accepted via env path | Guardrail — leak-free behaviour | Guardrail — preserved behaviour |
| Has baseline? | Yes — 0% / E01-E04 BLOCKED | Yes — 0% (F3) | n/a — new field | Yes — 100% today |
| Team can influence? | Yes — the helper + the slice | Yes — the parser | Yes — the redaction | Yes — the conditional header |
| Has guardrails? | KPI-3, KPI-4 | KPI-3, KPI-4 | is a guardrail | is a guardrail |

## Handoff to DEVOPS (platform-architect)

- **Data collection requirements**: none new at the platform level —
  the signal is the existing aperture audit stream (`decision`,
  `subject`, `tenant_id`, `sink_accepted`) correlated with a Spark
  export, plus the E01-E04 round-trip harness. No new metric, no new
  dashboard (mirrors ADR-0068's "no new metric, no new dashboard").
- **Dashboard/monitoring needs**: none. KPI-1/2 ride the gateway audit;
  KPI-3/4 are CI test gates, not runtime dashboards.
- **Alerting thresholds**: KPI-3 is a hard CI defect gate (a single
  token occurrence on a log surface fails the build); no runtime alert.
- **Baseline measurement**: E01-E04 are currently BLOCKED (the baseline
  for KPI-1); confirm they return GREEN post-delivery.
