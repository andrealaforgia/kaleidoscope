# DoR Validation: prism-echarts-paint-e2e-v0

Author: Luna (nw-product-owner). Wave: DISCUSS. Date: 2026-06-13.
British English. No em dashes in body.

Validates the 9-item Definition of Ready for US-PE-01, US-PE-02, US-PE-03
against `user-stories.md`, `story-map.md`, `outcome-kpis.md`, and
`wave-decisions.md`, citing the code loci verified on this branch. The DoR
is a hard gate: every item must PASS with evidence before handoff to
DESIGN. Verdict at the end.

## Verified code loci (re-confirmed on this branch, feeding every story)

| Locus | Confirmed | Used by |
|---|---|---|
| `apps/prism/src/lib/echarts/EChart.tsx:69-84` ECharts lifecycle skipped under jsdom (canvas-2D probe `null` -> `echarts.init` never called) | yes | US-PE-01, C2, D1 |
| `apps/prism/src/lib/echarts/EChart.tsx:88-98` `setOption` paint error swallowed (empty-body `catch`) | yes | US-PE-03, D3, KPI-3 |
| `apps/prism/src/lib/echarts/EChart.tsx:104` `data-tick-count` doc-hidden attribute (the parallel for the new paint signal) | yes | US-PE-01, D1 |
| `grep data-prism-chart-painted apps/prism/src` -> no matches (the paint signal does NOT exist yet) | yes | US-PE-01, D1, KPI-1 |
| `apps/prism/e2e/slice-01-walking-skeleton.spec.ts:63` waits on `[data-prism-chart-painted="true"]`; body throws `UNIMPLEMENTED` | yes | US-PE-01, D2 |
| `apps/prism/e2e/slice-03-error-and-empty-states.spec.ts:33-49` zero-uncaught-error invariant scaffolded; bodies throw `UNIMPLEMENTED` | yes | US-PE-02, US-PE-03 |
| `apps/prism/playwright.config.ts:57` `testMatch: ['__no-spec-matches-yet__.spec.ts']` (matcher rigged to match nothing) | yes | US-PE-01, D5, C5 |
| `apps/prism/playwright.config.ts:35-41` `PROMETHEUS_IMAGE_DIGEST` SSOT; `:48-56` per-slice re-add roadmap | yes | C5, KPI-6 |
| `apps/prism/playwright.config.ts:70-74` chromium / firefox / webkit projects | yes | C7, Scope boundary |
| `apps/prism/e2e/global-setup.ts:42-53` docker-run of the pinned Prometheus fixture; `/-/ready` wait | yes | US-PE-01, D4 honest limit |
| 6 e2e specs, 44 blocks, every body throws `UNIMPLEMENTED` (`e2e/slice-0{1..6}-*.spec.ts`) | yes | Scope boundary, D5 |
| `apps/prism/src/lib/echarts/buildOption.ts` fidelity flags (`smooth:false`, `connectNulls:false`, `sampling:'none'`) | yes (4Q Q1) | C1 |
| `docs/evolution/claims-honesty-pass-2-v0-evolution.md` MARK (not REMOVE) of the e2e scaffold; preserved digest + roadmap; follow-up item 12 | yes | Origin, C5, KPI-6 |

### Paint-signal confirmation (D1, the heart of slice 1)

The specs already reference `[data-prism-chart-painted="true"]`
(`slice-01-*.spec.ts:63`) but the component does not emit it (`grep` finds
no match in `src`); only `data-tick-count` exists (`EChart.tsx:104`).
Conclusion: a new doc-hidden paint signal wired to the real ECharts render
lifecycle is required (DELIVER); the exact mechanism (the `finished` event,
the non-empty-series condition, the reset-on-new-query semantics) is
DESIGN's D1 call. The change is internal / doc-hidden; no operator-visible
change (C1).

### CI-browser confirmation (D4, the load-bearing question)

Running the un-MARKed spec needs a real headless Chromium and docker for
the Prometheus fixture (`e2e/global-setup.ts:45-49`). This is why
`claims-honesty-pass-2` MARKed the gate scaffold rather than implementing
it. D4 (the CI job: browser install, the pinned image, headless run, gate
vs continue-on-error) is flagged for DEVOPS, NOT resolved here. The honest
limit (C6) forbids any "CI-verified" claim before that job is observed
green; the interim claim is "verified locally under headless Chromium".

---

## US-PE-01: The headline chart is proven to genuinely paint in a real browser

| # | DoR item | Verdict | Evidence |
|---|---|---|---|
| 0 | Elevator Pitch (Before / After / Decision enabled; real entry point; concrete output) | PASS | Before / After / Decision enabled all present. Entry point is user-invocable: the prism URL `http://localhost:5173/` + typing `up` + Enter (not a service function or test runner). Output is concrete and observable: a non-blank painted ECharts canvas with a rendered series. Decision enabled is real: Priya trusts the chart shape; the maintainer ships on automated proof. |
| 1 | Problem statement clear, domain language | PASS | Priya reads the chart at 03:14 to judge the signal; no automated test ever instantiates / asserts the chart paints (jsdom-skip, rigged matcher); a blank canvas would pass every check. Domain language (on-call SRE, PromQL, the chart); no solution prescription (the paint mechanism is left to DESIGN). |
| 2 | User/persona with specific characteristics | PASS | Priya the on-call operator (opens prism in a browser at incident time, reads the chart) plus the maintainer / release engineer (ships prism, wants automated proof). US-PE-01 Who. |
| 3 | 3+ domain examples with real data | PASS | Three examples: `up` paints a real line against the pinned fixture; the paint signal is false before the render completes; a blank canvas fails the proof. Real query (`up`), real signal, real paint-signal semantics. |
| 4 | UAT in Given/When/Then (3-7 scenarios) | PASS | 3 Gherkin scenarios (paints in a real browser; not painted until genuine render; blank canvas fails). Within 3-7. |
| 5 | AC derived from UAT | PASS | 5 AC each trace to a scenario / constraint (paint asserted; signal not set on mount; fails on blank + today's behaviour; testMatch un-MARK + SSOT preserved; jsdom narrow + Vitest green). |
| 6 | Right-sized (1-3 days, 3-7 scenarios) | PASS | 3 scenarios; one paint-signal wiring + one spec body + one testMatch un-MARK + the DEVOPS CI job. 1-2 days. Slice 1 (story-map R1). |
| 7 | Technical notes: constraints/dependencies | PASS | US-PE-01 Technical Notes: depends on D1/D2/D5/D4; paint signal parallel to `data-tick-count`; specs already reference the selector; honest limit C6; perf blocks out of scope (D5). |
| 8 | Dependencies resolved or tracked | PASS | D1/D2/D5 flagged for DESIGN, D4 for DEVOPS (`wave-decisions.md`); loci enumerated and bounded; no unresolved blocker (D4 is the load-bearing item, surfaced not resolved). |
| 9 | Outcome KPIs defined with measurable targets | PASS | US-PE-01 KPI block + `outcome-kpis.md` KPI-1 (paint tests 0 -> at least 1) and KPI-2 (human inspection -> automated run), falsifiable against today. |

US-PE-01 DoR: **9/9 PASS** (+ Dimension 0 Elevator Pitch PASS).

## US-PE-02: An empty result renders an honest empty state, not a blank that looks broken

| # | DoR item | Verdict | Evidence |
|---|---|---|---|
| 0 | Elevator Pitch | PASS | Before / After / Decision enabled present. Entry point: the prism SPA, running a valid no-data query (`up{job="nonexistent"}`). Output: the visible "No data for {range}..." message in the chart area, no warning banner, URL preserved. Decision enabled: Priya widens the range / fixes the metric name rather than wondering if prism is broken. |
| 1 | Problem statement clear, domain language | PASS | An empty-but-honest chart and a broken-blank chart look identical if both are an empty rectangle; Priya cannot act confidently. Domain language; no solution prescribed (the empty-vs-paint semantics is D6). |
| 2 | User/persona with specific characteristics | PASS | Priya the on-call operator running valid queries that sometimes match no series at incident time. US-PE-02 Who. |
| 3 | 3+ domain examples with real data | PASS | `up{job="nonexistent"}` shows the calm "no data" message; the empty state shows legible text distinct from a blank; a successful `up` does not show the message. Real queries, real message text. |
| 4 | UAT in Given/When/Then (3-7 scenarios) | PASS | 3 Gherkin scenarios (honest empty message; visibly distinct from blank; negative control). |
| 5 | AC derived from UAT | PASS | 4 AC trace to scenarios + the un-MARK / Vitest-green guard; written as observable behaviour (visible message text, no banner, URL preserved). |
| 6 | Right-sized (1-3 days, 3-7 scenarios) | PASS | 3 scenarios; the slice-03 empty-result spec body over US-PE-01's mechanism. < 1 day on top of US-PE-01. Slice 2. |
| 7 | Technical notes: constraints/dependencies | PASS | US-PE-02 Technical Notes: depends on US-PE-01 + D6; part of slice 2; no banner-copy change (C1). |
| 8 | Dependencies resolved or tracked | PASS | Depends on US-PE-01 (tracked, slice 1). D6 (empty-vs-paint semantics) flagged for DESIGN. |
| 9 | Outcome KPIs defined with measurable targets | PASS | US-PE-02 KPI block + `outcome-kpis.md` KPI-4 (empty / error states visibly distinct, 0 -> at least 3 tests). |

US-PE-02 DoR: **9/9 PASS** (+ Dimension 0 Elevator Pitch PASS).

## US-PE-03: A paint or query failure is surfaced visibly, not swallowed into a blank

| # | DoR item | Verdict | Evidence |
|---|---|---|---|
| 0 | Elevator Pitch | PASS | Before / After / Decision enabled present. Entry point: the prism SPA, entering an invalid PromQL query or hitting a backend 500. Output: a visible warning banner with the backend's error text, the page interactive, no uncaught console error; a genuine paint failure surfaces (signal never flips / console error). Decision enabled: Priya fixes her query or escalates the outage; the maintainer gets a red test when the chart stops painting. |
| 1 | Problem statement clear, domain language | PASS | The worst incident-time outcome is a blank chart with no explanation; paint errors are swallowed (`EChart.tsx:91-97`); in a browser the swallow would hide a genuine paint failure. Domain language; no solution prescribed (the swallow remediation is D3). |
| 2 | User/persona with specific characteristics | PASS | Priya the on-call operator (query typos, backend outages) plus the maintainer (relies on the e2e to catch paint regressions). US-PE-03 Who. |
| 3 | 3+ domain examples with real data | PASS | `/?q=invalid syntax)(` shows a banner; a backend 500 renders inline; a forced paint failure surfaces and reds the e2e. Real query, real HTTP status, real swallow locus. |
| 4 | UAT in Given/When/Then (3-7 scenarios) | PASS | 3 Gherkin scenarios (invalid query banner; backend error inline; paint failure surfaced not swallowed). |
| 5 | AC derived from UAT | PASS | 4 AC trace to scenarios + the un-MARK guard; observable outcomes (visible banner, page interactive, no uncaught error, swallow narrowed, Vitest green). |
| 6 | Right-sized (1-3 days, 3-7 scenarios) | PASS | 3 scenarios; the slice-03 error spec bodies + narrowing the swallow over US-PE-01's mechanism. 1 day on top of US-PE-01. Slice 2. |
| 7 | Technical notes: constraints/dependencies | PASS | US-PE-03 Technical Notes: depends on US-PE-01 + D3; the zero-uncaught-error invariant already scaffolded (`slice-03-*.spec.ts:33-49`); no banner-copy change (C1). |
| 8 | Dependencies resolved or tracked | PASS | Depends on US-PE-01 (tracked). D3 (swallow remediation keeping jsdom narrow) flagged for DESIGN. |
| 9 | Outcome KPIs defined with measurable targets | PASS | US-PE-03 KPI block + `outcome-kpis.md` KPI-3 (swallowed failures -> 0 surfaced) and KPI-4 (failures visibly distinct). |

US-PE-03 DoR: **9/9 PASS** (+ Dimension 0 Elevator Pitch PASS).

## Feature-level checks

- **Solution-neutrality**: D1-D6 are stated as requirements ("the chart is
  genuinely proven to paint", "the empty state is visibly distinct", "a
  paint failure is surfaced not swallowed", "no CI-verified claim before the
  job is green"), not mechanisms. DESIGN owns the paint-signal mechanism,
  the canvas-assertion technique, the swallow remediation, the
  empty-vs-paint semantics, and the testMatch scope; DEVOPS owns the
  CI-browser job. PASS.
- **Real data**: every example uses Priya, real queries (`up`,
  `up{job="nonexistent"}`, `/?q=invalid syntax)(`), real HTTP statuses
  (500), the real pinned fixture, and the real loci (`EChart.tsx:69-84`,
  `:91-97`, `playwright.config.ts:57`). No generic `user123` /
  `test@test.com`. PASS.
- **No technical-AC anti-pattern**: AC are observable outcomes ("a non-blank
  ECharts canvas with a rendered series", "a visible 'no data' message", "a
  warning banner with the backend's error text", "the e2e fails loudly"),
  not implementation ("subscribe to the `finished` event"). The mechanism
  is deferred to DESIGN as D1-D6. PASS.
- **Scenario titles are business outcomes**: each names what the operator /
  system achieves ("Typing a query paints the headline chart in a real
  browser", "A valid query with no matching series shows an honest empty
  message", "A real-browser paint failure is surfaced, not swallowed"), not
  a class / method. PASS.
- **Not infrastructure (Dimension 0 item 4/5)**: although the deliverable is
  an e2e test, every story's observable is the operator-visible chart /
  message at the prism URL, and every Decision enabled names a real
  operator decision (read the shape, widen the range, fix the query) plus
  the maintainer's ship decision. No slice is all-`@infrastructure`: slice
  1 (US-PE-01) and slice 2 (US-PE-02/03) each deliver a user-visible,
  browser-observable behaviour. PASS.
- **Error/edge ratio**: of the ~10 scenarios, the strong majority are
  error / edge / falsifiability (paint-signal-not-set-on-mount, blank-canvas
  fails, empty-distinct-from-blank, negative control, invalid query,
  backend 500, paint-failure-surfaced). US-PE-03 is 3/3 error; US-PE-02 is
  2/3 edge/control; US-PE-01 is 2/3 edge/error. Well above the >= 40%
  target. PASS.
- **Right-sizing (feature)**: 3 stories, ~10 scenarios, 1 app
  (`apps/prism`), 1 persona, no new UI; bounded loci; the remaining 4 specs
  + perf blocks + browser matrix explicitly OUT OF SCOPE. PASS (story-map
  Scope Assessment).
- **No DIVERGE artifacts**: recorded as a Low-impact risk in
  `wave-decisions.md`; the job is grounded in the four-quadrants Q3 finding,
  the claims-honesty-pass-2 MARK, and the Earned-Trust posture, and verified
  in code. Does not block.
- **Earned-Trust / honesty guard**: the un-MARK is the honest inverse of the
  claims-honesty-pass-2 MARK; the `PROMETHEUS_IMAGE_DIGEST` SSOT and the
  per-slice roadmap are preserved (C5); no "CI-verified" claim before the D4
  job is green (C6, KPI-6); every paint AC is falsifiable against today's
  behaviour (C4). PASS.
- **No-regression guard (C2 / KPI-5)**: the jsdom skip stays narrow and the
  existing Vitest suite stays green; the operator-visible chart and banners
  are unchanged (C1). PASS.

## Verdict

**DoR PASS for US-PE-01, US-PE-02, US-PE-03 (9/9 each, + Dimension 0
Elevator Pitch PASS each).** All three stories are ready for the DESIGN
wave, with D1-D6 correctly flagged (D1/D2/D3/D5/D6 DESIGN-owned, D4 the
load-bearing DEVOPS CI-browser item) and every KPI falsifiable against
today's no-paint, swallowed-error, rigged-matcher behaviour. The honest
limit (no "CI-verified" claim before the CI-browser job is green) is
surfaced for DESIGN / DEVOPS to word truthfully; the digest SSOT and the
per-slice roadmap are preserved. Component change expected internal /
doc-hidden only (no operator-visible change). Proceed to the peer-review
gate before handoff.
</content>
