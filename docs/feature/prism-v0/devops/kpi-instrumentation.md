# Prism v0 — KPI instrumentation

- **Wave**: DEVOPS
- **Author**: `@nw-platform-architect` (Apex, dispatched by Bea)
- **Date**: 2026-05-08
- **Inputs**: `outcome-kpis.md` (the only DISCUSS file routed to
  DEVOPS per parallel-handoff posture); `observability-design.md`
  (KPI 1 / 2 emission path); `ci-cd-pipeline.md` Gates 6 / 7 / 10.
- **Companion**: `monitoring-alerting.md`,
  `platform-architecture.md`, `wave-decisions.md`.

---

## 1. Scope

Five KPIs from `outcome-kpis.md` plus three cross-KPI guardrails. For
each, this document specifies:

- **Where the metric is captured** — CI fixture, browser-emitted, or
  both.
- **How DEVOPS instruments it** — which gate, which test, what
  assertion shape.
- **Where the dashboard lives at v0** — CI artefact retention vs the
  Loom Phase 2 dashboard graduation.

The pattern mirrors the DISCUSS-side `Measured by` and `Measurement
Plan` sections; this document is the v0 implementation contract for
each.

---

## 2. KPI 1 — first-chart-rendered latency

### 2.1 What it measures

95th percentile of "page open → first ECharts canvas paint with the
backend's data" under 2 seconds, on a developer's laptop against a
local Prometheus container with one metric and 24 hours of retention.

### 2.2 Where the metric is captured

**Both** CI fixture and browser-emitted:

| Capture site | Mechanism | Purpose |
|---|---|---|
| CI fixture (Gate 7 Playwright) | Playwright's `page.evaluate(() => performance.now())` markers around the Slice 01 walking-skeleton flow | Gate-level enforcement at PR / push time |
| Browser-emitted (production) | `prism.first_chart_latency_ms` gauge emitted on every page load with a non-empty query | Operator-side production visibility |

The CI fixture is the structural enforcement; the browser emission
is the production observation surface.

### 2.3 How DEVOPS instruments it (CI fixture)

Gate 7 Playwright spec `slice-01-walking-skeleton.spec.ts` runs the
KPI 1 fixture:

- Open `http://localhost:5173/?q=up&from=-15m` (the Slice 01
  walking-skeleton URL).
- Mark `t0 = performance.now()` at `DOMContentLoaded`.
- Wait for the first ECharts canvas paint by polling for
  `[data-prism-chart-painted="true"]` on the chart container element
  (the SPA sets this attribute in the `<EChart>` wrapper's first-mount
  `useEffect` after `setOption` returns).
- Mark `t1 = performance.now()`.
- Record `delta = t1 - t0`.
- Repeat 20 times in a loop within the same test, against the same
  Prometheus fixture.
- Assert: `quantile(0.95, deltas) < 2000`.

The 20-run sample is the orchestrator's brief (KPI 1 §
Measurement plan). The fixture's flakiness budget targets 0 over
100 CI runs; the assertion is deterministic on the CI runner's
hardware. If a single CI run sees one outlier above 2 s, the gate
fails — the discipline is to land more performance work, not to
loosen the bound.

> **HIGH-2 note (Forge iter-1)**: shared-pool runner hardware
> (`ubuntu-latest`) introduces CPU throttling, network jitter, and
> contention. A "0 over 100 runs" budget is an asymptotic target,
> not a hard contract. In practice: if a developer observes a
> single transient spike to >2 s, this is a CI-infrastructure
> signal, not a Prism regression — re-run the job. If >5% of runs
> see persistent spikes, escalate: investigate runner allocation
> (GitHub Actions diagnostics), consider a dedicated runner pool
> for Gate 7, or loosen the flakiness budget to ≤5% tolerance if
> hardware cannot be isolated.

### 2.4 How DEVOPS instruments it (browser emission)

Per `observability-design.md` § 7.1: the SPA's `<EChart>` wrapper's
first-mount `useEffect` calls
`Em.markFirstChartPainted(performance.now())` immediately after
`setOption` returns. The emitter computes the delta against the
`DOMContentLoaded` mark and enqueues a metric:

```ts
{
  name: 'prism.first_chart_latency_ms',
  value_ms: 1247.3,
  context: { backend_label, browser, page_load: true },
}
```

The emitter flushes via `fetch POST /v1/metrics` (same-origin) and
through Aperture to the operator's backend.

### 2.5 Where the dashboard lives at v0

**CI artefact**: the Playwright report
(`apps/prism/playwright-report/`) carries the per-run latency
distribution as a test attachment. Retained 30 days per
`environments.yaml`.

**Production dashboard**: NOT at v0. Loom Phase 2 builds the panels
against the operator's Prometheus, querying:

```promql
quantile_over_time(0.95, prism_first_chart_latency_ms[5m])
```

Loom inherits this without back-propagation.

### 2.6 Slice that lights it up

Slice 01. Slice 01's acceptance criterion is met when the CI
fixture's p95 < 2 s assertion passes.

---

## 3. KPI 2 — query-to-chart-update latency on iterate

### 3.1 What it measures

95th percentile of "Run pressed → chart updated" under 800 ms.

### 3.2 Where the metric is captured

Same as KPI 1: both CI fixture and browser-emitted.

### 3.3 How DEVOPS instruments it (CI fixture)

Gate 7 Playwright spec `slice-01-walking-skeleton.spec.ts` (extends
the KPI 1 fixture):

- After the first chart paint (KPI 1 captured), simulate Run-press
  20 times with the same query.
- For each Run-press: mark `t0` at the click event handler entry,
  wait for the next `[data-prism-chart-painted="true"]` toggle (the
  attribute is re-set on every `setOption` call), mark `t1`.
- Assert: `quantile(0.95, iterate_deltas) < 800`.

Same flakiness budget (0 over 100 CI runs). Same fixture, same
Prometheus container.

### 3.4 How DEVOPS instruments it (browser emission)

`Em.markIterateStart(performance.now())` on the Run button's click
handler entry; `Em.markIteratePainted(performance.now())` in the
`<EChart>` wrapper's non-first-mount `useEffect` after `setOption`.
The emitter computes the delta and enqueues:

```ts
{
  name: 'prism.iterate_latency_ms',
  value_ms: 412.1,
  context: { backend_label, browser, iterate_count, page_load: false },
}
```

### 3.5 Where the dashboard lives at v0

Same as KPI 1: CI artefact in Playwright report; Loom Phase 2 owns
the production dashboard.

### 3.6 Slice that lights it up

Slice 01.

---

## 4. KPI 3 — data-fidelity invariant (no client-side smoothing)

### 4.1 What it measures

100% byte-equality between the rendered chart's series data and the
backend's `data.result.values`. Zero smoothing, zero interpolation,
zero aggregation, zero NaN-bridging.

### 4.2 Where the metric is captured

**CI only** (Vitest unit test). No production telemetry, by design:
the invariant is structural and the unit test is the structural
enforcement.

### 4.3 How DEVOPS instruments it

Gate 6 Vitest test `slice-01-walking-skeleton.test.ts` (or its KPI 3
specialisation `kpi-3-fidelity.test.ts` per the crafter's choice):

- Hand-craft a fixture: five points with NaNs at positions 2 and 4:
  `[[t1, 1], [t2, NaN], [t3, 3], [t4, NaN], [t5, 5]]`.
- Mock `lib/promql/queryRange` to return a `QueryOutcome.success`
  with that series.
- Call `buildOption(outcome, ctx)`.
- Assert: `option.series[0].data` is byte-identical to the fixture's
  `[ts, value]` tuples in the same order with NaNs preserved.
- Assert: `option.series[0].smooth === false`.
- Assert: `option.series[0].connectNulls === false`.
- Assert: `option.series[0].sampling === undefined`.

The assertion is deep-equal byte-for-byte; any mutation to the
option-builder that smooths, downsamples, or interpolates breaks
the assertion.

The mutation-testing gate (Gate 10) covers the same surface from a
different angle: any boolean flip on `smooth`/`connectNulls` is a
mutation that must be killed by the unit test.

### 4.4 Where the dashboard lives at v0

**No dashboard**. KPI 3 is a 100%-or-fail invariant; a dashboard
shows variance, but variance here is a defect. The CI gate is the
ground truth; if Gate 6 stays green, KPI 3 holds.

### 4.5 Slices that light it up

01, 03, 04 (the auto-refresh ticks each go through the same
`buildOption` path, so the unit test's coverage extends across the
slices).

---

## 5. KPI 4 — URL roundtrip fidelity

### 5.1 What it measures

100% byte-equality on within-session reload of any Prism URL. For
absolute time ranges, 100% on cross-day reload provided the backend's
retention covers the range.

### 5.2 Where the metric is captured

**CI only** (Playwright E2E + Vitest property test). No production
telemetry: the invariant is structural and the codec is pure-function.

### 5.3 How DEVOPS instruments it

**Vitest property test** (Gate 6) in `slice-01-walking-skeleton.test.ts`:

- For every canonical `UrlState`, assert
  `decode(encode(state)).value === state`.
- Run across all relative-range presets, the four refresh intervals,
  and a sample of absolute timestamps.

**Playwright E2E** (Gate 7) in `slice-02-relative-presets.spec.ts`
and `slice-05-absolute-range.spec.ts`:

- Open Prism with a hard-coded URL.
- Wait for the chart to paint.
- Snapshot the rendered series JSON via
  `await page.evaluate(() => echart.getOption().series)`.
- Open a fresh Playwright context (cold tab) with the same URL.
- Wait for the chart to paint.
- Snapshot the rendered series.
- Assert: snapshots are byte-equal.

Both fixtures use the local Prometheus container so the underlying
data is reproducible across the two opens.

### 5.4 Where the dashboard lives at v0

**No dashboard**. Same reasoning as KPI 3: 100%-or-fail invariant;
the CI gate is the ground truth.

### 5.5 Slices that light it up

02 (relative roundtrip), 05 (absolute roundtrip).

---

## 6. KPI 5 — page-stays-usable invariant under failure

### 6.1 What it measures

100% across every documented failure mode. No blank page, no
JavaScript exception, no stuck spinner.

### 6.2 Where the metric is captured

**CI only at v0** (Playwright E2E). v0.x adds a browser-emitted
counter `prism.uncaught_error_count` that complements the CI gate
with production observation; v0 ships with CI as the ground truth.

### 6.3 How DEVOPS instruments it

Gate 7 Playwright spec `slice-03-errors.spec.ts` drives each failure
mode:

| Failure mode | How Playwright drives it | What to assert |
|---|---|---|
| PromQL parse error | URL with `?q=invalid syntax)(` | Inline warning banner; query input still focused; URL preserved |
| Transport failure (network) | Stop the Prometheus container mid-test, reload | Inline warning naming `backend.label`; chart area shows last-fetched time; SPA does not crash |
| Transport failure (HTTP 500) | Use Playwright's request interception to inject a 500 | Inline warning naming the status; query input still interactive |
| Empty result | Query a metric that returns 0 results: `?q=nonexistent_metric` | Calm "No data for {range}" message, NOT an error banner |
| `/config.json` unreachable | Playwright fixture serves Prism without `/config.json` | Composition root renders the calm error UI; `<App>` does NOT mount |
| Hand-edited URL with bad `from` | Open `?q=up&from=garbage` | Calm "URL parameters were invalid: from" banner; defaults applied; SPA stays interactive |

For each, the assertions are:

1. No uncaught console errors (`page.on('pageerror', fn)` + `page.on
   ('console', msg => msg.type() === 'error')` accumulate; assert
   the array is empty at end-of-test).
2. The query input remains focusable (Playwright's
   `await locator('input[name="q"]').focus()` succeeds).
3. The URL still encodes the broken state (Playwright's
   `await page.url()` matches the expected canonical form).

### 6.4 Where the dashboard lives at v0

**No dashboard at v0**. The CI gate is the ground truth. v0.x adds
the `prism.uncaught_error_count` metric to the emitter; Loom Phase 2
graphs it.

### 6.5 Slices that light it up

03, 06 (accessibility audit confirms keyboard recoverability from
every state).

---

## 7. Cross-KPI guardrail — operator-time

### 7.1 What it measures

Walking-skeleton flow (open Prism → type query → see chart) under 5
seconds median on a developer's laptop.

### 7.2 Where the metric is captured

**CI only**: the sum of KPI 1 (page open → first chart) plus a
median Run-press cycle. The Playwright fixture computes this
implicitly: the test from "Playwright opens the page" to "first
chart paints after typing `up` and pressing Run" must complete in
under 5 seconds.

### 7.3 How DEVOPS instruments it

Gate 7 Playwright spec asserts a wall-clock timeout on the Slice 01
walking-skeleton test. If the test exceeds 5 seconds, the gate
fails.

### 7.4 Where the dashboard lives at v0

CI artefact in Playwright report. Loom Phase 2 graphs the sum
metric (`prism.first_chart_latency_ms + prism.iterate_latency_ms`).

---

## 8. Cross-KPI guardrail — bundle size

### 8.1 What it measures

Prism's gzipped JS bundle ≤ 300 KB at v0.

### 8.2 Where the metric is captured

**CI only** (Gate 8). The deployed bundle is a copy of the gated
artefact; no production telemetry needed.

### 8.3 How DEVOPS instruments it

Gate 8 (`gate-8-prism-bundle-size`):

- Run `pnpm --filter prism build`.
- Run `node apps/prism/scripts/check-bundle-size.js`.
- Script walks `apps/prism/dist/`, sums gzipped sizes of JS chunks,
  asserts total ≤ 300 KB.
- Emits `apps/prism/dist/bundle-size-report.json` with per-chunk
  breakdown.

Implementation contract documented in `ci-cd-pipeline.md` § 3.3.

### 8.4 Where the dashboard lives at v0

CI artefact (`bundle-size-report.json`), 30-day retention. Loom
Phase 2 may add a bundle-size trend panel sourced from the artefact
history; v0 does not.

---

## 9. Cross-KPI guardrail — browser support

### 9.1 What it measures

Latest two stable versions of Chrome / Firefox / Safari engines
render Prism correctly.

### 9.2 Where the metric is captured

**CI only** (Gate 7 Playwright matrix). Production browser
diversity is the operator's surface.

### 9.3 How DEVOPS instruments it

Per `environments.yaml > runtime-matrix`: Playwright runs all six
specs against three engines (Chromium, Firefox, WebKit) on every CI
run. WebKit covers Safari for cross-browser purposes; the Slice 06
manual a11y audit on a real Mac+Safari is the operator-deployment
spot check.

### 9.4 Where the dashboard lives at v0

CI artefact (Playwright report shows per-engine pass/fail). v0 has
no production browser-fingerprint telemetry.

---

## 10. KPI → Gate → Slice traceability matrix

| KPI / guardrail | Gate(s) | Slice(s) | Browser-emitted? | Dashboard at v0? |
|---|---|---|---|---|
| KPI 1 — first-chart latency p95 < 2s | Gate 7 (Playwright fixture) | 01 | YES (`prism.first_chart_latency_ms`) | CI artefact only; Loom Phase 2 owns prod |
| KPI 2 — iterate latency p95 < 800 ms | Gate 7 (Playwright fixture) | 01 | YES (`prism.iterate_latency_ms`) | CI artefact only; Loom Phase 2 owns prod |
| KPI 3 — data fidelity 100% | Gate 6 (Vitest unit), Gate 10 (Stryker) | 01, 03, 04 | NO (structural) | None — invariant |
| KPI 4 — URL roundtrip 100% | Gate 6 (Vitest property), Gate 7 (Playwright) | 02, 05 | NO (structural) | None — invariant |
| KPI 5 — page-stays-usable 100% | Gate 7 (Playwright) | 03, 06 | v0.x (`prism.uncaught_error_count`) | None at v0 |
| Operator-time guardrail | Gate 7 timeout | 01 | implicit (KPI 1 + 2) | CI artefact only |
| Bundle size 300 KB gzipped | Gate 8 | every slice's build | NO | CI artefact (`bundle-size-report.json`) |
| Browser matrix (Chrome / FF / Safari) | Gate 7 (3 engines) | every slice's E2E | NO | CI artefact (Playwright report) |

---

## 11. Earned-trust three-layer per KPI

| KPI | Subtype | Structural | Behavioural |
|---|---|---|---|
| KPI 1 | n/a (latency is a measured value, not a type) | Gate 7 fixture asserts p95 < 2s | Browser-emitted gauge per real session |
| KPI 2 | n/a | Gate 7 fixture asserts p95 < 800ms | Browser-emitted gauge per real session |
| KPI 3 | TS types for `EChartsOption` invariants | Gate 6 byte-equality test; Gate 10 mutation kill rate | Gate 7 visual-regression baseline |
| KPI 4 | TS types for `UrlState` discriminated union | Gate 6 property test `decode(encode(s)) === s` | Gate 7 byte-equality on rendered series across reload |
| KPI 5 | TS exhaustive switch on `QueryOutcome.kind` | Gate 7 four-failure-mode suite + console error assertion | v0.x browser counter `prism.uncaught_error_count` |
| Bundle size | n/a | Gate 8 ceiling | n/a (build-time only) |

Every KPI lands in at least two layers; KPI 3 / 4 / 5 (the load-
bearing invariants) land in all three.

---

## 12. Cross-references

- **CI gates (Gates 6, 7, 8, 10)**: `ci-cd-pipeline.md`.
- **Browser emission path**: `observability-design.md`.
- **Loom / Aegis graduation roadmap**: `monitoring-alerting.md` § 6.
- **DISCUSS source**: `outcome-kpis.md` (the only file routed to
  DEVOPS per parallel-handoff posture).
- **DESIGN-side coverage strategy**: `component-design.md` § 10
  (quality attributes by KPI).
