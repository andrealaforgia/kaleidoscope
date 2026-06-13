# Outcome KPIs: prism-echarts-paint-e2e-v0

Author: Luna (nw-product-owner). Wave: DISCUSS. Date: 2026-06-13.
British English. No em dashes in body.

Companion to `user-stories.md` (US-PE-01/02/03 each carry a per-story
Outcome KPIs block) and `wave-decisions.md` (D1-D6). This file
consolidates the feature-level KPIs with targets, baselines, and
measurement methods, so DEVOPS (`platform-architect`) can design the
CI-browser tracking and DISTILL/DELIVER can assert them. Every KPI is
falsifiable: each names a test that MUST FAIL against today's behaviour (no
paint signal, swallowed paint errors, the rigged matcher) and PASS only
once the chart genuinely paints.

All baselines are verified on this branch (see `wave-decisions.md` >
Verified code findings).

## Objective

Give prism's headline chart-rendering path the automated real-browser
verification it has never had: prove the ECharts chart genuinely paints a
query result (not a blank canvas, not a hollow DOM node), and prove that
empty and failure states render honestly and visibly, so the operator
trusts the chart and the maintainer ships it on automated proof rather than
human inspection.

## Feature-level KPIs

| # | Who | Does What | By How Much | Baseline | Measured By | Type |
|---|-----|-----------|-------------|----------|-------------|------|
| KPI-1 | the maintainer shipping prism + Priya reading the chart | gains an automated real-browser test that genuinely paints the ECharts chart | tests that genuinely paint the chart: 0 -> at least 1 (the walking-skeleton paint test), passing only on a non-blank canvas | 0 today (`EChart.tsx:69-84` jsdom-skips ECharts; `playwright.config.ts:57` matches no spec; 44 blocks throw `UNIMPLEMENTED`) | the slice-01 walking-skeleton paint test asserting paint signal + non-blank canvas + rendered series, included in `testMatch` | Leading |
| KPI-2 | the maintainer's verification of the headline chart | moves from human inspection of a built bundle to an automated real-browser run | matched specs: 0 (`__no-spec-matches-yet__`) -> the slice-01 + slice-03 paint / failure specs; run locally under headless Chromium | the chart is verified only by the hand-built `dist/` bundle + eyeballing (4Q Q3) | `testMatch` includes the implemented specs; `pnpm playwright` runs them green locally; the D4 CI job runs them headless | Leading |
| KPI-3 | the maintainer relying on the e2e + Priya facing a blank chart | gets a real-browser paint failure surfaced instead of silently swallowed | swallowed real-browser paint failures: "always eaten" (`EChart.tsx:91-97`) -> 0 (surfaced; the zero-uncaught-error invariant reds) | the `setOption` catch swallows every paint error with an empty body | a forced-paint-failure case proving the e2e reds; the slice-03 zero-uncaught-error invariant | Leading |
| KPI-4 | Priya reading an empty or failed chart | tells an honest "no data" / a visible error apart from a blank that looks broken | real-browser tests asserting failure / empty states are visibly distinct from a blank canvas: 0 -> at least 3 (empty, parse error, transport error) | the component renders these states (4Q Q2 INFO) but no real-browser test asserts they are distinct from a blank / failed canvas | the slice-03 empty / parse-error / transport-error tests asserting visible message text + the zero-uncaught-error invariant | Leading |
| KPI-5 | the existing prism test surface | experiences zero regression from the change | Vitest jsdom suite stays green; the jsdom skip stays narrow | 125 Vitest blocks green today; the jsdom canvas-2D skip is correct for jsdom | the existing Vitest suite staying green after the paint signal + narrowed swallow land | Guardrail |
| KPI-6 | the project's honesty posture (Earned-Trust) | makes no "CI-verified" claim before the CI-browser job is green, and preserves the scaffold SSOT | "CI-verified" claims before D4 is green: 0; `PROMETHEUS_IMAGE_DIGEST` SSOT + per-slice roadmap: preserved | `claims-honesty-pass-2` MARKed the gate scaffold and preserved the digest + roadmap deliberately | the wave narrative / slides / READMEs claiming only "verified locally" until D4 is observed green; the SSOT + roadmap intact in `playwright.config.ts` | Guardrail |

## KPI detail and falsifiability

### KPI-1 -- The headline chart is genuinely painted by at least one test (US-PE-01)

- **Baseline locus**: `EChart.tsx:69-84` (jsdom skip, `echarts.init`
  never called under Vitest); `playwright.config.ts:57` (no spec matched);
  no `data-prism-chart-painted` in `src`.
- **By how much**: 0 -> at least 1 test that paints the chart and asserts a
  non-blank canvas with a rendered series.
- **Falsifiability**: the test MUST FAIL against today's blank / hollow
  state (no paint signal, the matcher matching nothing) and PASS only on a
  genuine paint. A test asserting only `role=figure` presence is rejected
  (`wave-decisions.md` risk "a paint test that passes on the hollow
  DOM-only check").

### KPI-2 -- Verification moves from human inspection to an automated real-browser run (US-PE-01)

- **Baseline**: the chart is verified only by the hand-built `dist/`
  bundle and human inspection (4Q Q3).
- **By how much**: matched specs 0 -> the slice-01 + slice-03 specs; run
  green locally under headless Chromium.
- **Honest limit (C6)**: local-green is real value; the "CI-verified"
  claim waits on the D4 CI-browser job going green (KPI-6).

### KPI-3 -- Real-browser paint failures are surfaced, not swallowed (US-PE-03)

- **Baseline locus**: `EChart.tsx:91-97` (empty-body `catch` swallowing
  every `setOption` failure).
- **By how much**: swallowed real-browser paint failures move from "always
  eaten" to 0 (a paint failure surfaces: the signal never flips and / or a
  console error is emitted, so the zero-uncaught-error invariant reds).
- **Falsifiability**: a forced paint fault in a real browser MUST red the
  e2e; the jsdom-only swallow stays narrow so the Vitest suite is
  unaffected (KPI-5).

### KPI-4 -- Empty and failure states are visibly distinct from a blank canvas (US-PE-02, US-PE-03)

- **By how much**: 0 -> at least 3 real-browser tests (empty, parse error,
  transport error) asserting visible message text, not series-absence.
- **Falsifiability**: each asserts the VISIBLE message / banner in the
  browser; an honest empty render is provably distinct from a blank that
  painted nothing (D6).

### KPI-5 -- Zero regression to the Vitest jsdom suite (guardrail)

- **Baseline**: 125 Vitest blocks green; the jsdom canvas-2D skip is
  correct for jsdom.
- **By how much**: 0 regression. The paint signal and the narrowed swallow
  must not break the jsdom suite; the jsdom skip stays narrow (C2).
- **Measured by**: the existing Vitest suite staying green after the change.

### KPI-6 -- No "CI-verified" claim before the CI-browser job is green; scaffold preserved (guardrail / honesty)

- **Baseline**: `claims-honesty-pass-2` MARKed the gate scaffold and
  preserved the `PROMETHEUS_IMAGE_DIGEST` SSOT + the per-slice roadmap.
- **By how much**: 0 premature "CI-verified" claims; the SSOT + roadmap
  remain intact and are corrected truthfully only as each slice graduates.
- **Measured by**: the narrative / slides / READMEs claiming only "verified
  locally under headless Chromium" until the D4 job is observed green; the
  digest SSOT and the roadmap comment intact in `playwright.config.ts`.

## Metric hierarchy

- **North Star**: the headline ECharts chart is genuinely proven to paint a
  query result in a real browser, by a test that fails on a blank canvas
  (KPI-1). This is the verification the feature exists to deliver.
- **Leading indicators**: automated-over-human verification (KPI-2),
  surfaced-not-swallowed failures (KPI-3), visibly-distinct empty / error
  states (KPI-4).
- **Guardrail metrics**: zero Vitest regression (KPI-5), no premature
  "CI-verified" claim + preserved scaffold (KPI-6). These must NOT
  degrade: a paint proof that regresses the jsdom suite, or a gate that is
  advertised before it runs green, is a regression even if the paint
  assertion works.

## KPI-to-story trace

| KPI | Primary story | Decision dependency | Baseline locus |
|---|---|---|---|
| KPI-1 chart genuinely painted | US-PE-01 | D1, D2, D5 | `EChart.tsx:69-84`; `playwright.config.ts:57` |
| KPI-2 automated over human inspection | US-PE-01 | D4, D5 | hand-built `dist/` + eyeballing (4Q Q3) |
| KPI-3 failures surfaced not swallowed | US-PE-03 | D3 | `EChart.tsx:91-97` |
| KPI-4 empty / error visibly distinct | US-PE-02, US-PE-03 | D6 | states render today; no real-browser test |
| KPI-5 zero Vitest regression | all (guardrail) | C2 | 125 Vitest blocks green |
| KPI-6 no premature CI claim; scaffold kept | all (guardrail) | C5, C6, D4 | claims-honesty-pass-2 MARK |

## Measurement timing

- **DISTILL** (`acceptance-designer`): KPI-1, KPI-3, KPI-4 become
  executable real-browser assertions; each must fail on today's behaviour
  before DELIVER makes it pass (the EDD failing-test-first discipline).
  KPI-5's "Vitest suite green" is the regression control.
- **DELIVER** (`software-crafter`): KPI-1 / KPI-4 (the un-MARKed specs run
  green locally under headless Chromium) and KPI-5 (Vitest green) are the
  closing checks; Gate 10 (StrykerJS) pins the paint-signal and
  narrowed-swallow branches if the component logic changes (C10).
- **DEVOPS** (`platform-architect`): KPI-2 / KPI-6 are the CI-browser job's
  remit (D4): provision headless Chromium + docker for the fixture, run the
  un-MARKed specs headless, and word the CI claim honestly (no
  "CI-verified" before the job is observed green). This file is the
  tracking-design input.

## Handoff to DEVOPS (instrumentation / CI requirements)

1. **CI-browser job (D4, load-bearing)**: a job that installs / provisions
   headless Chromium (the Playwright browser, the pinned image already
   SSOT'd in `playwright.config.ts`), provides docker for the Prometheus
   fixture (`e2e/global-setup.ts`), and runs the un-MARKed slice-01 +
   slice-03 specs headless. DEVOPS owns whether it gates or is
   `continue-on-error` initially (Luna's lean: continue-on-error first,
   tighten once green and stable; pure trunk-based, no required checks).
2. **Honest CI claim (C6 / KPI-6)**: no "CI-verified" claim in any wave
   artefact, README, narrative, or slide until the job is observed green.
   The interim claim is "verified locally under headless Chromium".
3. **Baseline collection**: none required pre-release; the baseline is "0
   tests paint the chart, the matcher matches nothing, paint errors are
   swallowed" and is verified in code.
4. **Flake watch**: the local p95 perf-KPI blocks are OUT OF SCOPE (known
   overnight flake, MEMORY); the CI-browser job should not inherit them
   (D5 splits / fixmes them). Watch the canvas / paint-signal assertion for
   determinism (D2 keeps it non-timing-dependent against the pinned
   fixture).
</content>
