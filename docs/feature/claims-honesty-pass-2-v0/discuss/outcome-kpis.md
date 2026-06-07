# Outcome KPIs — claims-honesty-pass-2-v0

## Feature: claims-honesty-pass-2-v0

### Objective

Within this honesty pass, a reader of any residual flagged surface (pulse crate
doc, gateway comments + test prose, the platform README's Prism claims, the prism
e2e config) finds the claim matches the code — neither over-trusting a capability
that does not ship (columnar pulse, Prism dashboards, a green e2e gate) nor
under-trusting one that does (pulse durability).

This is an honesty/correctness feature: the KPI is the count of overstated (or
inverted) claims driven to zero, verifiable by a doc-lint/grep guard cross-read
against the cited code. There is no user-behaviour analytics signal to instrument
(the "user" is a reader of static prose); the measurable outcome is the residual
overstatement count and the guard suite that pins it.

### Outcome KPIs

| # | Who | Does What | By How Much | Baseline | Measured By | Type |
|---|-----|-----------|-------------|----------|-------------|------|
| KPI-1 | Evaluators/integrators reading the pulse docs | read a durability+substrate posture matching the shipped `FileBackedMetricStore` (durable, not volatile; JSON+WAL, not columnar) | residual inverted/over-stated pulse claims = 0 | 2 (1 inverted-volatility `lib.rs:46` + 1 columnar over-claim `lib.rs:20-21,41`/`Cargo.toml:7`) | grep/doc-lint guard: false phrases ABSENT + corrected phrases PRESENT, cross-read against `file_backed.rs` + dep list | Leading |
| KPI-2 | Contributors reading the gateway source comments + test prose | read comments/test docs matching the delivered green code (real subscriber, default-Stub config, green always-run scenarios) | residual stale/inaccurate gateway comment loci = 0 | 3 comment loci (`main.rs:62-63`, `:118-120`+`:24-25`) + 1 stale test-module prose block | grep guard: false phrases ABSENT + corrected phrases PRESENT, cross-read against `init_tracing` / `Config::builder().build()`; suite green | Leading |
| KPI-3 | Evaluators reading the platform's Prism claims + prism CI config | scope Prism as a single-metric PromQL explorer (not Grafana-class) and not count a non-existent browser-matrix e2e gate | residual prism overstatement loci = 0 | 3 (README row `:184` + cost line `:222` + vacuous e2e gate advertisement) | grep/doc-lint guard: false phrases ABSENT + corrected phrases PRESENT, cross-read against `apps/prism/README.md` + `testMatch` reality | Leading |

### Metric Hierarchy

- **North Star**: residual verified overstatements (or inversions) across the
  in-scope loci = **0** (baseline 8: 2 pulse + 3 gateway-style + 3 prism, counting
  the test-module prose and the cost line as their own loci).
- **Leading Indicators**: per-locus guard pass (KPI-1/2/3) — each slice's
  false-string-absent + true-string-present + cross-read-matches-code check.
- **Guardrail Metrics** (must NOT degrade):
  - No production-logic line changed (durability, subscriber, probe, config
    behaviour all unchanged) — the pulse + gateway test suites stay green.
  - No `#[ignore]`d / RED in-flight marker altered (crash-durability tests, the
    gateway fixed-port AC-01 scenarios, the prism per-spec `UNIMPLEMENTED` e2e
    bodies).
  - No FALSE caveat introduced (the cli durability doc, now accurate, is left
    untouched — adding "may lose data on crash" would degrade honesty).
  - No claim pass-v0 already corrected is re-touched or regressed
    (Spark/Strata/Cinder/Loom rows, query-api `step`, README durability section).

### Measurement Plan

| KPI | Data Source | Collection Method | Frequency | Owner |
|-----|------------|-------------------|-----------|-------|
| KPI-1 | `pulse/src/lib.rs`, `pulse/Cargo.toml` vs `file_backed.rs` + dep list | grep/doc-lint guard in the DISTILL guard suite | per CI run | acceptance-designer (DISTILL) |
| KPI-2 | `kaleidoscope-gateway/src/main.rs` + `tests/slice_01_tracing_subscriber.rs` vs `init_tracing`/`Config::builder().build()` | grep guard; gateway suite green | per CI run | acceptance-designer (DISTILL) |
| KPI-3 | `README.md:184,222`, `apps/prism/playwright.config.ts`, prism README vs `apps/prism/README.md` + `testMatch` | grep/doc-lint guard | per CI run | acceptance-designer (DISTILL) |
| Guardrails | pulse + gateway test suites; in-flight marker inventory; cli doc | existing test suites + guard asserting in-flight markers PRESENT and cli caveat ABSENT | per CI run | acceptance-designer (DISTILL) |

### Hypothesis

We believe that correcting the residual flagged claims (pulse volatility +
columnar, gateway comments + test prose, prism README + cost line + e2e gate
advertisement) to match the code, for evaluators/integrators/contributors reading
those surfaces, will achieve **zero residual verified overstatements/inversions**
across the in-scope loci.

We will know this is true when **a reader of any in-scope surface finds the claim
matches the cited code** — measured by the per-locus guard (false string absent +
true string present + cross-read matches code), the unchanged behaviour suites,
and the intact in-flight markers.

### Handoff to DEVOPS

Minimal: the only instrumentation is the guard suite (grep/doc-lint + the existing
behaviour suites) that DISTILL authors. No runtime telemetry, dashboard, or
alerting threshold is required — the "metric" is a static-analysis guard over
prose/config, run as part of CI feedback (trunk-based, CI-is-feedback per project
memory). The guards must assert BOTH directions: false phrases absent / true
phrases present, AND in-flight markers + the cli durability claim left intact (no
over-reach).
