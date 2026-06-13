# Wave Decisions: prism-echarts-paint-e2e-v0 (DISCUSS)

Author: Luna (nw-product-owner). Wave: DISCUSS. Date: 2026-06-13.
British English. No em dashes in body.

## Origin

The four-quadrants prism report
(`kaleidoscope-4-quadrants-theory/reports/prism.md`, Q3 finding 2, the
"end-to-end verification gap"): prism's headline feature, the ECharts
chart that is the visual query result, has NEVER been verified to paint by
any automated test. `EChart.tsx:69-84` skips the entire ECharts lifecycle
under jsdom (a canvas-2D probe returns `null`, so `echarts.init` is never
called in Vitest), and the update path swallows paint errors
(`EChart.tsx:91-97`). The 6 Playwright e2e spec files exist as detailed
pseudocode, but all 44 `test()` blocks throw `UNIMPLEMENTED`, and
`playwright.config.ts:57` sets `testMatch: ['__no-spec-matches-yet__.spec.ts']`
so Playwright matches no spec at all. The chart is verified only by the
hand-built `dist/` bundle and human inspection.

This feature is the honest completion of the e2e that
`claims-honesty-pass-2-v0` MARKed "NOT YET IMPLEMENTED". That feature did
the honest thing for a not-yet-built gate: it MARKed the scaffold rather
than deleting it, and it deliberately preserved the
`PROMETHEUS_IMAGE_DIGEST` SSOT pin and the slice-by-slice re-add roadmap so
the genuine coverage could be built later
(`docs/evolution/claims-honesty-pass-2-v0-evolution.md`, "The decision",
sub-decision 1; "Known follow-ups" item 12). This feature builds that
genuine coverage. It is the un-MARK with real assertions behind it, not a
new claim: the roadmap and the digest SSOT are kept exactly; the scaffold
note is corrected truthfully as each slice graduates.

It is also the read-path companion to the operator's incident-time job
that prism v0 exists to serve (`apps/prism/README.md`): an on-call
operator opens prism at 03:14, types a query, and reads the shape of the
signal off the chart. Today nothing automatically proves that chart paints.

No DIVERGE artifacts exist for this feature
(`docs/feature/prism-echarts-paint-e2e-v0/diverge/` is absent). The job
below is grounded in the four-quadrants Q3 finding, the
`claims-honesty-pass-2` MARK it follows, and the Earned-Trust posture
(`docs/product/architecture/brief.md` Principle 12). Absence of DIVERGE is
recorded as a risk; it does not block, because the gap and the fix
direction are verified directly in code on this branch.

## The Job (the operator who reads the chart + the maintainer who must trust it ships)

> **When** an on-call operator opens prism at 03:14, types a query, and
> presses Enter, **I want** the headline ECharts chart to genuinely paint
> the returned series in the browser (not a blank canvas, not a hollow DOM
> node), **so that** the operator can trust the shape they read is the real
> signal, and the maintainer can trust the product's headline feature is
> verified by an automated real-browser test before it ships, instead of
> by human inspection of a hand-built bundle.

The current behaviour: in Vitest (jsdom) the chart is never instantiated
(the canvas probe is `null`, so `echarts.init` is skipped) and any paint
error on the update path is swallowed; in Playwright no spec runs at all
(the matcher is rigged to match nothing). So the seam the platform sells
hardest, "type `up` and see a chart against real Prometheus", and ECharts
painting at all, are proven by no automated test.

## Feature framing decisions (DISCUSS, decided per the run brief)

| ID | Decision | Rationale |
|---|---|---|
| F-Type | **User-facing (frontend chart)** | The touchpoint is the prism SPA in a browser: the operator navigates to the prism URL, types a PromQL query, presses Enter, and the observable output is the painted ECharts canvas. Per the run brief, Decision 1 = User-facing. |
| F-Skeleton | **Walking Skeleton = No (brownfield)** | prism exists, is built, and ships a real ECharts integration, a real Prometheus client, a URL codec, and an auto-refresh state machine (4Q report Q1). The 6 e2e specs and the Prometheus container fixture already exist. This feature wires genuine paint coverage onto an existing app; it is not a greenfield bootstrap. Per the brief, the FIRST slice is nonetheless a thin end-to-end proof (one chart, one query, a real paint assertion, running locally + a CI job). |
| F-UX | **UX research = Lightweight** | Single primary persona (Priya, the on-call operator, established by the existing specs and `apps/prism/README.md`). The emotional arc is Confidence / Problem Relief: a chart that is verified only by eyeballing a bundle becomes a chart that is automatically proven to paint. Per the brief, Decision 3 = Lightweight. |
| F-JTBD | **No standalone job-analysis artifact** | Per the brief, Decision 4 = JTBD No. The job is stated above and carried into the stories. |
| F-Journey-artifacts | **No `journey-*-visual.md` / `journey-*.yaml`** | No prior user-facing sibling feature in `docs/feature/*/discuss/` produced journey SSOT artifacts; the matched precedent (`aperture-body-size-cap-v0`, and the prism-v0 spec structure) uses `user-stories.md` + `story-map.md` + `outcome-kpis.md` + `dor-validation.md` + this file. The brief instructs SSOT journey/jobs only if a user-facing sibling did so; none did. The single screen flow (open URL, type query, press Enter, read chart) is captured in the Elevator Pitches and UAT scenarios, not a separate journey schema. |
| F-Slicing | **Two slices, three stories** | Slice 1 = the walking-skeleton paint proof (US-PE-01). Slice 2 = honest failure-mode rendering with no swallow (US-PE-02 empty state, US-PE-03 surfaced paint/query error). See "Carpaccio slicing" in `story-map.md`. The remaining 4 specs and the perf-KPI / browser-matrix graduations are explicitly OUT OF SCOPE (see "Scope boundary" below). |

## Verified code findings (confirming the four-quadrants read, re-read on this branch)

All confirmed by reading the source on this branch.

| Claim | Verified location | Finding |
|---|---|---|
| ECharts lifecycle is SKIPPED under jsdom | `apps/prism/src/lib/echarts/EChart.tsx:69-84` | The mount effect probes `document.createElement('canvas').getContext('2d')`; if it returns `null` (jsdom) it returns early and `echarts.init` is never called. So no Vitest test ever instantiates the chart. |
| Paint errors are SWALLOWED on the update path | `apps/prism/src/lib/echarts/EChart.tsx:88-98` | `instance.setOption(option, {notMerge:true})` is wrapped in `try { } catch { }` with an empty body and a comment "jsdom: canvas paint unavailable". In a real browser this catch would also silently hide a genuine paint failure. |
| The paint SIGNAL the specs assert does NOT exist yet | `grep data-prism-chart-painted apps/prism/src` -> no matches | Only `data-tick-count` exists on the chart container (`EChart.tsx:104`). The specs wait on `[data-prism-chart-painted="true"]` (e.g. `e2e/slice-01-walking-skeleton.spec.ts:63`), a signal the component does not emit. Wiring this signal to the real ECharts paint lifecycle is the heart of slice 1. |
| The 6 e2e spec files exist but every block is UNIMPLEMENTED | `apps/prism/e2e/slice-0{1..6}-*.spec.ts` | 6 files, 44 `test()` blocks, every body throws `Error('UNIMPLEMENTED ...')` with detailed Given/When/Then comments and commented-out implementation. They are genuine specs awaiting bodies, not throwaway stubs. |
| The Playwright matcher is rigged to match nothing | `apps/prism/playwright.config.ts:57` | `testMatch: ['__no-spec-matches-yet__.spec.ts']`. The per-slice re-add roadmap is in the comment at `:48-56`. The 4Q report adds that `package.json` runs Playwright with `--pass-with-no-tests`, so the suite is green-by-vacuum. |
| The Prometheus container fixture is real and pinned | `apps/prism/playwright.config.ts:35-41`, `apps/prism/e2e/global-setup.ts:42-53` | `globalSetup` runs `docker run --rm -d ... ${PROMETHEUS_IMAGE_DIGEST}` and waits on `/-/ready`. `PROMETHEUS_IMAGE_DIGEST` is the SSOT shared with `.github/workflows/ci.yml`'s gate-11 services block; `claims-honesty-pass-2` preserved it deliberately. |
| The browser projects are Chrome / Firefox / Safari | `apps/prism/playwright.config.ts:70-74` | Three Playwright projects (chromium, firefox, webkit). The brief scopes this feature to headless Chromium; the full matrix is the future graduation (Scope boundary). |
| The chart IS a real ECharts integration (not a placeholder) | `apps/prism/src/lib/echarts/EChart.tsx:23-44,73`, 4Q report Q1 | Direct modular ECharts import (LineChart + CanvasRenderer), instance held in a ref, `setOption({notMerge:true})` on update. `buildOption.ts` enforces fidelity flags (`smooth:false`, `connectNulls:false`, `sampling:'none'`). The paint really happens in a real browser; only the automated proof is missing. |
| Error / empty states already render something | 4Q report Q2 INFO; `e2e/slice-03-error-and-empty-states.spec.ts` | parse-error, transport-error, empty, and config-error each have a visible banner/fallback today (no silent blank-on-error in the component). The gap is that no real-browser test asserts these are distinguishable from a blank/broken canvas. |

## The observable "it painted" assertion (decided)

The hollow check this feature must NOT settle for is "a DOM node with
`role=figure` exists". That passes today against a blank canvas. The
genuine "it painted" observable, chosen here as the requirement (DESIGN
owns the exact technique, see D1/D2), is the conjunction of:

1. **A real paint signal.** A doc-hidden attribute on the chart container
   (the specs already name `data-prism-chart-painted="true"`) that toggles
   to `true` ONLY after ECharts reports a completed render of a non-empty
   series, wired to the real ECharts paint lifecycle (the `finished`
   render event), parallel to the existing `data-tick-count`. It is absent
   / `false` before the first real paint and is reset on each new query
   until the next paint completes. A test that waits on this signal cannot
   pass on a mounted-but-unpainted div.
2. **A non-blank canvas probe.** The test asserts the ECharts canvas
   carries real ink, not uniform background: read the rendered canvas
   (e.g. `chart.getDataURL()` / canvas pixel sampling) and assert it is not
   a single uniform colour. This is the assertion that defeats the
   "blank-that-looks-broken" case the brief calls out.
3. **A rendered-series assertion.** `page.evaluate(() => chart.getOption().series)`
   reports at least one series with at least one point for a query (`up`)
   known to return data against the fixture container.

The primary lock is (1) + (3); the pixel probe (2) is the strongest
"actually drew ink" guard and is required for the slice-1 walking-skeleton
test. For the empty-result and error states (slice 2), the observable is
the VISIBLE message text in the browser (not the absence of a series),
because an honest empty render must be distinguishable from a failed paint.

## Decisions FLAGGED for DESIGN / DEVOPS

DISCUSS encodes the REQUIREMENT (the headline chart is genuinely proven to
paint in a real browser; failure modes render visibly, not as a swallowed
blank; the jsdom skip stays narrow; the CI job runs the un-MARKed spec).
DESIGN (`nw-solution-architect`) and DEVOPS (`nw-platform-architect`) own
the mechanism.

### D1 - The paint-signal mechanism (DESIGN)

- **What**: how the component exposes the "genuinely painted" signal the
  test waits on. The specs already reference `[data-prism-chart-painted="true"]`.
- **Requirement**: the signal toggles `true` only after a real ECharts
  render of a non-empty series completes, and resets on each new query.
  It must NOT be set on mere mount.
- **Luna's lean** (non-binding): subscribe to the ECharts `finished` event
  on the instance; set `data-prism-chart-painted="true"` on the chart
  container when `finished` fires and `getOption().series` is non-empty;
  clear it when a new query begins. Parallel to the existing
  `data-tick-count` doc-hidden attribute (`EChart.tsx:104`). DESIGN
  confirms the event and the reset semantics.

### D2 - The non-blank-canvas assertion technique (DESIGN / DISTILL)

- **What**: how the test proves the canvas carries real ink, not just that
  a series object exists in the option.
- **Requirement**: the slice-1 walking-skeleton test asserts the canvas is
  not a uniform blank. DESIGN / DISTILL pick the technique
  (`chart.getDataURL()` pixel non-uniformity, canvas `getImageData`
  sampling, or an equivalent), kept deterministic and non-flaky.
- **Luna's lean** (non-binding): `chart.getDataURL({pixelRatio:1})` and
  assert more than one distinct pixel value over a sampled region; pair it
  with the paint signal (D1) and the rendered-series assertion so the test
  fails on a blank canvas AND on an empty option.

### D3 - The error-swallow remediation, keeping the jsdom skip narrow (DESIGN)

- **What**: the `try { } catch { }` at `EChart.tsx:91-97` currently
  swallows every `setOption` failure. In a real browser this would also
  hide a genuine paint failure from the e2e test.
- **Requirement**: in a real browser a paint failure must SURFACE (the
  paint signal never flips, and/or a console error is emitted) so the
  zero-uncaught-error e2e invariant catches it; the jsdom-only skip stays
  narrow enough that it does not also mask real-browser failures. The
  Vitest jsdom suite must stay green (the narrow skip is preserved).
- **Luna's lean** (non-binding): only swallow when the canvas-2D probe is
  `null` (the genuine jsdom condition already used at `EChart.tsx:71`);
  in a real browser, let a paint failure propagate to a visible error
  state and/or fail to set the paint signal, so the e2e reds. DESIGN picks
  the exact shape; the chosen behaviour becomes a locked, honestly-worded
  AC.

### D4 - The CI-browser job (DEVOPS) - THE LOAD-BEARING ITEM, why this was deferred

- **What**: running the un-MARKed Playwright spec needs a real headless
  Chromium and the pinned Prometheus container in CI. This is the reason
  `claims-honesty-pass-2` MARKed the gate scaffold rather than implementing
  it; it is the heaviest, most environment-dependent part of the feature.
- **Requirement**: a CI job installs / provisions headless Chromium (the
  Playwright browser, the pinned image already SSOT'd in
  `playwright.config.ts`), provides docker for the Prometheus fixture
  (`global-setup.ts`), and runs the un-MARKed spec headless. DEVOPS owns
  the browser install, the headless run, the docker availability, and
  CRUCIALLY whether the job GATES or is `continue-on-error` initially.
- **Luna's lean** (non-binding): start `continue-on-error` (feedback, not a
  gate) consistent with the project's pure trunk-based, no-required-checks
  posture (MEMORY `project_kaleidoscope_pure_trunk_based`), then tighten to
  gating once the job is observed green and stable. DEVOPS decides; this is
  NOT resolved in DISCUSS.

### D5 - The testMatch un-MARK scope, preserving the SSOT and the roadmap (DESIGN / DEVOPS)

- **What**: how `testMatch` graduates from `['__no-spec-matches-yet__.spec.ts']`
  to matching the specs this feature implements, without inheriting the
  unimplemented specs or destroying the preserved scaffold.
- **Requirement**: un-MARK `testMatch` to include ONLY the slice-1 spec
  (slice 1) and the slice-3 error/empty spec (slice 2); the remaining 4
  specs stay UNIMPLEMENTED and OUT of the matcher (Scope boundary). The
  `PROMETHEUS_IMAGE_DIGEST` SSOT and the per-slice re-add roadmap comment
  (`playwright.config.ts:35-56`) are PRESERVED and corrected truthfully as
  each slice graduates (the un-MARK is the honest inverse of the
  claims-honesty-pass-2 MARK, not a deletion).
- **Luna's lean** (non-binding): also split or `test.fixme` the two p95
  perf-KPI blocks and the operator-time guardrail inside
  `slice-01-*.spec.ts` with a disclosed reason, so un-MARKing slice 01 does
  not silently rope in known-flaky p95 wall-clock tests (MEMORY
  `p95_wallclock_flakes_overnight`). DESIGN decides whether to split the
  file or fixme the blocks; either way the perf KPIs are NOT this feature.

### D6 - Empty-state vs paint-signal semantics (DESIGN)

- **What**: an empty result genuinely renders (a calm "No data" chart area)
  but has no series, so the paint signal (D1, which requires a non-empty
  series) would not flip true. The test must not mistake an honest empty
  render for a paint failure, nor mistake a paint failure for an empty
  render.
- **Requirement**: the empty state is asserted by its VISIBLE message
  ("No data for {range}..."), distinct from both a painted-with-data chart
  and a blank/failed canvas. DESIGN reconciles the paint-signal semantics
  with the empty state (e.g. a distinct `data-prism-chart-empty` marker, or
  asserting the message text directly).

## Honest limit to record (for DESIGN / DEVOPS to word truthfully)

- The genuine paint assertion can run LOCALLY today (Playwright Chromium on
  a developer machine via `pnpm playwright`, with docker for the fixture).
  The full claim "the chart is verified to paint in CI" depends on the D4
  CI-browser job actually executing and going green.
- **Until the D4 job is observed green, no wave (DESIGN, DEVOPS, DELIVER,
  or the narrative/slides) may claim the chart is "CI-verified".** The
  honest interim claim is "verified locally under headless Chromium; CI
  verification pending the browser job". This is the exact discipline
  `claims-honesty-pass-2` exists to enforce; do not re-create an
  advertised-but-vacuous gate (the very thing that feature MARKed).
- The local run itself depends on docker being available for the Prometheus
  fixture container (`global-setup.ts`). A machine without docker cannot
  run the spec; this is a stated precondition, not a silent assumption.

## Scope boundary (what this feature is NOT, to keep the carpaccio honest)

IN SCOPE (this feature):

- Slice 1: the walking-skeleton paint proof (US-PE-01) - one query, the
  genuine paint signal + non-blank-canvas assertion, the slice-01
  walking-skeleton paint test running locally, testMatch un-MARKed for it,
  and the D4 CI-browser job existing.
- Slice 2: honest failure-mode rendering (US-PE-02 empty state, US-PE-03
  surfaced paint/query error) and removing the real-browser paint-error
  swallow, with the slice-03 error/empty tests un-MARKed.

OUT OF SCOPE (named future work, the scaffold marks they would retire stay
in place, per `claims-honesty-pass-2` follow-up item 12):

- The 4 remaining specs: `slice-02` (time range / relative presets),
  `slice-04` (auto-refresh), `slice-05` (absolute range / permalink),
  `slice-06` (accessibility). These are largely non-paint behaviours (URL
  codec, picker, reducer, a11y) and are a much larger body; graduating them
  is the full-e2e completion, a separate feature.
- The two p95 perf-KPI blocks and the operator-time guardrail inside
  `slice-01-*.spec.ts` (latency p95 < 2s / < 800ms). These are perf KPIs
  subject to the known overnight p95 wall-clock flake (MEMORY
  `p95_wallclock_flakes_overnight`); they need the CI-gating treatment, not
  a paint feature.
- The Firefox / WebKit browser-matrix breadth. This feature targets
  headless Chromium (the brief). The full matrix is the future graduation.
- Building prism's missing dashboarding / multi-panel / logs-traces-profiles
  scope (4Q report Q3 finding 1). Unrelated to the paint-proof gap.

## Risks

| Risk | Probability | Impact | Mitigation |
|---|---|---|---|
| **No DIVERGE artifacts** - job not validated through a DIVERGE wave | Medium | Low | The job is grounded in the four-quadrants Q3 finding, the `claims-honesty-pass-2` MARK it follows, and the Earned-Trust posture, and is verified directly in code. Recorded here; does not block. |
| **A paint test that passes on the hollow DOM-only check** (the false-confidence trap) | High (the easy assertion is the hollow one) | High | The test MUST assert the genuine paint signal (D1) AND a non-blank canvas (D2) AND a rendered series (D3). It must FAIL against today's behaviour (no paint signal, swallowed errors, jsdom skip) and pass only on a real painted canvas. DISTILL must not inherit a test that asserts only `role=figure` presence. This is the Earned-Trust crux of the feature. |
| **The CI-browser job never goes green but the claim is made anyway** (re-creating the vacuous gate) | Medium | High | The honest-limit note forbids any "CI-verified" claim until D4 is observed green. Interim claim is "verified locally". The un-MARK preserves the digest SSOT and the roadmap so the gate is real apparatus, not advertising. D4 is DEVOPS-owned and explicitly unresolved here. |
| **The real-browser paint swallow keeps hiding failures** (D3) | Medium | High | The `EChart.tsx:91-97` catch must not swallow real-browser paint failures; D3 requires a paint failure to surface (signal never flips / console error) so the e2e reds. The jsdom skip stays narrow; the Vitest suite stays green (guardrail KPI-5). |
| **p95 perf flake roped into the un-MARK** | Medium | Medium | D5 / Scope boundary: the perf-KPI blocks and operator-time guardrail are split or `test.fixme`d with a disclosed reason; un-MARking slice 01 must not silently include them. |
| **Empty render mistaken for a paint failure (or vice versa)** (D6) | Medium | Medium | The empty state is asserted by its visible message, distinct from both a painted chart and a blank canvas; DESIGN reconciles the paint-signal semantics with the empty state (D6). |
| **Flaky pixel/canvas assertion** | Medium | Medium | D2 keeps the canvas assertion deterministic (sample a region against the fixture's known `up` series; avoid timing-dependent thresholds). The fixture container is pinned by digest (SSOT). |
| **prism is the headline UI, real reader impact** | Medium | Medium | The change is test + a doc-hidden paint signal + narrowing the swallow; it must not alter the operator-visible chart, the fidelity flags (`buildOption.ts`), or the existing error/empty banners. The existing Vitest suite staying green is the regression guard (KPI-5). |

## Constraints established

- **C1 - prism is the headline UI.** The change must not alter the
  operator-visible chart, the fidelity flags (`buildOption.ts`:
  `smooth:false`, `connectNulls:false`, `sampling:'none'`), the palette
  default, or the existing parse-error / transport-error / empty / config
  banners. The paint signal is a doc-hidden attribute, not a visible
  change.
- **C2 - The jsdom skip stays narrow; the Vitest suite stays green.** The
  canvas-2D probe skip (`EChart.tsx:69-84`) is preserved for jsdom; the
  genuine paint assertion lives only in the real-browser Playwright run.
  The existing 125 Vitest blocks must stay green (regression guard).
- **C3 - Genuine paint, not a hollow check.** The assertion is the
  paint-signal + non-blank-canvas + rendered-series conjunction (see
  "The observable" section), not the presence of a DOM node.
- **C4 - Falsifiable against today.** Every paint AC must FAIL against
  today's behaviour (no paint signal, swallowed errors, no spec runs) and
  pass only when the chart genuinely paints. DISTILL must not inherit a
  test green-by-vacuum.
- **C5 - Preserve the SSOT and the roadmap.** The `PROMETHEUS_IMAGE_DIGEST`
  pin and the per-slice re-add roadmap (`playwright.config.ts:35-56`) are
  preserved and corrected truthfully on graduation; the un-MARK is the
  honest inverse of the `claims-honesty-pass-2` MARK, not a deletion.
- **C6 - No "CI-verified" claim before the CI job is green.** Honest
  interim claim is "verified locally under headless Chromium"; the CI
  claim waits on D4 (see Honest limit).
- **C7 - Headless Chromium only.** This feature targets Chromium; the
  Firefox / WebKit matrix is future (Scope boundary).
- **C8 - Pure trunk-based, no CI gates** (MEMORY
  `project_kaleidoscope_pure_trunk_based`). The D4 job is feedback first;
  whether it gates is DEVOPS's call (D4), defaulting to `continue-on-error`.
- **C9 - Only the crafter writes `apps/prism/src`.** Per CLAUDE.md, DELIVER
  (`nw-software-crafter`) writes the component change (the paint signal,
  the narrowed swallow) and the spec bodies; DISCUSS / DESIGN / DEVOPS /
  DISTILL produce specifications, ADRs, platform design, and the executable
  acceptance specs respectively.
- **C10 - Frontend mutation testing (Gate 10, StrykerJS)** applies if the
  component logic changes (`scripts/run-stryker.sh`). The paint-signal
  branch (painted vs not-painted) and the narrowed-swallow branch (jsdom vs
  real-browser) should be pinned; DESIGN / DELIVER confirm the scope.

## Notes for downstream waves

- **DESIGN** (`nw-solution-architect`): own D1 (paint-signal mechanism), D2
  (canvas assertion technique), D3 (swallow remediation keeping jsdom
  narrow), D5 (testMatch un-MARK scope + perf-block handling), D6 (empty vs
  paint-signal). Produce an ADR if the paint-signal contract or the swallow
  policy warrants one (the existing prism ADRs are 0026-0032; ADR-0030 owns
  the ECharts wrapper). Confirm no operator-visible change (C1) and the
  Vitest suite stays green (C2).
- **DEVOPS** (`nw-platform-architect`): own D4 - the CI-browser job (headless
  Chromium install, the pinned Prometheus container, docker availability,
  gate vs continue-on-error). This is the load-bearing item and the reason
  the gate was deferred. Word the CI claim honestly (C6): no "CI-verified"
  before the job is observed green.
- **DISTILL** (`nw-acceptance-designer`): the UAT scenarios in
  `user-stories.md` are the source. The slice-01 walking-skeleton paint
  test and the slice-03 empty / error tests become executable; each must
  FAIL against today's behaviour and pass only on genuine paint (C4). Do
  NOT inherit a test that asserts only DOM presence or that passes
  green-by-vacuum. Preserve the digest SSOT and the roadmap (C5).
- **DELIVER** (`nw-software-crafter`): only the crafter writes
  `apps/prism/src` and the spec bodies. Wire the paint signal (D1), narrow
  the swallow (D3), implement the slice-01 + slice-03 spec bodies, un-MARK
  `testMatch` for them (C5). Keep the existing Vitest suite green (C2) and
  the operator-visible chart unchanged (C1). Run Gate 10 (StrykerJS) on the
  changed component logic if applicable (C10). Trunk-based fix-forward
  (MEMORY). NEVER bump any crate / package to 1.0.0.
</content>
</invoke>
