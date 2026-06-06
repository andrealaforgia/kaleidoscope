# Story Map: perf-kpi-ci-non-gating-v0

## User: Maintainer (Andrea) reading the GitHub Actions result

## Goal: A red Gate-1 build always means a real correctness regression, never runner disk variance; while the wall-clock perf KPIs stay visible as a tracked, non-gating signal.

## Backbone

The maintainer's journey through one CI run, left to right:

| Push triggers CI | Gate 1 evaluates correctness | Perf KPIs are observed | Maintainer interprets result | Reasoning is recorded |
|------------------|------------------------------|------------------------|------------------------------|-----------------------|
| US-01: Gate 1 stops opting in to perf | US-03: correctness still hard-gates Gate 1 | US-02: non-gating perf job runs + reports p95 | US-01 + US-03: red == real break, perf breach != red | US-04: honesty note (durable budgets are dev-indicative) |

---

### Walking Skeleton

This is brownfield CI; no standalone skeleton feature. The thinnest
end-to-end slice that makes the journey work is:

- **US-01** (de-gate Gate 1) + **US-03** (correctness still gates).

That pair alone delivers the urgent, verifiable behaviour change: a perf
breach no longer reds the build, while a real break still does. It is
demonstrable in a single CI run.

### Release 1 (urgent fix): Trustworthy green

- **US-01** — `gate-1-test` stops setting `KALEIDOSCOPE_PERF_TESTS`.
- **US-03** — negative control: correctness still hard-gates.
- Target outcome KPI: zero perf-induced false reds in `gate-1-test`
  (KPI-1). Rationale: this is the "old, annoying problem" Andrea flagged;
  it is the highest-urgency, highest-value, lowest-effort change.

### Release 2 (visibility): Tracked perf signal

- **US-02** — non-gating `perf-kpis` job that runs the 28 tests with the
  variable set and reports p95 numbers.
- **US-04** — honesty note (durable-op budgets are dev-indicative, not
  CI-contractual; threshold-chasing forbidden).
- Target outcome KPI: 100% of `main` runs produce readable p95 numbers
  (KPI-2); 0 threshold-raise commits to durable budgets (KPI-4 guardrail).
  Rationale: preserves visibility (C4) and records the reasoning so the
  family does not silently rot.

## Priority Rationale

Priority is by outcome impact and dependency, not by feature grouping.

1. **US-01 + US-03 (Release 1, P1)** — Value 5 (kills the named pain;
   makes red trustworthy), Urgency 5 (Andrea flagged it directly; it is
   actively eroding trust in red), Effort 1 (delete one `env` block;
   correctness is already gating). Walking-skeleton tie-break applies:
   this is the minimum end-to-end slice. US-03 must ship with US-01 as its
   negative control — de-gating perf without proving correctness still
   gates would be irresponsible.
2. **US-02 (Release 2, P2)** — Value 4 (preserves the tracked signal so a
   real sustained regression is observable), Urgency 3 (important but the
   false-red bleeding stops with R1), Effort 2 (new CI job, non-gating
   wiring). Depends on US-01 (Gate 1 must stop owning the perf run first).
3. **US-04 (Release 2, P2)** — Value 3 (prevents future
   misinterpretation and threshold-chasing), Urgency 3, Effort 1 (doc +
   ADR). Can co-land with US-01 or US-02; grouped into R2 as the
   reasoning record that accompanies the visibility job.

Every story traces to an outcome KPI in `outcome-kpis.md` (no orphans).

## Scope Assessment: PASS — 4 stories, 1 module (CI workflow + docs), estimated 1 day

Elephant Carpaccio gate. Oversized signals checked (need 2+ to be
oversized):

- Stories: 4 (<= 10). PASS.
- Bounded contexts / modules: 1 (the CI workflow `.github/workflows/
  ci.yml` plus an ADR doc). The 28 affected tests are NOT modified — they
  already carry the ADR-0058 guard; this feature only changes *whether the
  env var is set*. PASS.
- Walking skeleton integration points: 1 (the `gate-1-test` job env).
  PASS.
- Estimated effort: ~1 day (delete an env block; add one non-gating job;
  write one ADR). PASS.
- Independent user outcomes that could ship separately: 2 (trustworthy
  green; tracked signal) — these are the two carpaccio slices below, both
  small. Not an oversized signal at this scale.

Verdict: **right-sized**. No split required beyond the natural 2-release
carpaccio (R1 urgent fix, R2 visibility + honesty), which is already the
preferred shape per the orchestrator brief.

### Carpaccio taste tests (applied)

- **Vertical, not horizontal**: each slice is a working CI behaviour the
  maintainer can verify on a run page, not a technical layer.
- **Demonstrable**: R1 is demonstrable in one CI run (perf-slow run goes
  green; correctness break goes red). R2 is demonstrable by reading the
  `perf-kpis` log.
- **Independently valuable**: R1 delivers the urgent fix even if R2 never
  ships; R2 adds visibility on top. Shipping R1 alone is a coherent,
  valuable release.
- **Thin end-to-end**: R1 touches exactly the `gate-1-test` env and relies
  on the existing guard; nothing wider.
