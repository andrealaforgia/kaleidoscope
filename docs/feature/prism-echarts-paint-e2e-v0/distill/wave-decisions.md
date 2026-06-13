<!-- markdownlint-disable MD013 MD024 -->

# Wave Decisions — prism-echarts-paint-e2e-v0 (DISTILL)

- **Wave**: DISTILL (nWave) — the acceptance-spec wave. I write the e2e
  SPECS and prove them RED; I do NOT write the production React wiring
  (DELIVER owns that).
- **Designer**: Quinn (nw-acceptance-designer)
- **Date**: 2026-06-13
- **Mode**: autonomous overnight; no questions returned to the operator.
- **British English; no em dashes; no emoji.**

## Inputs grounded (read-first checklist)

- [x] DESIGN `design/wave-decisions.md` "## Handoff summary" + the DD1-DD6
  table — the driving entry, the paint-signal contract, the swallow
  narrowing, the testMatch un-MARK scope, the empty-vs-paint reconciliation.
- [x] `docs/product/architecture/adr-0075-prism-echarts-paint-verification.md`
  — D1 (the `data-prism-chart-painted` lifecycle), D2 (the canvas
  `getImageData` non-uniformity probe), D3 (catch-and-surface), D5 (the
  in/fixme block table), D6 (empty by placement, no second marker), the
  alternatives, the Verification section, the honest CI limit (C6).
- [x] DEVOPS `devops/wave-decisions.md` — gate-7-prism-playwright is
  chromium-only + `--pass-with-no-tests` + `continue-on-error`; the docker
  Prometheus fixture via `e2e/global-setup.ts`; the digest SSOT; the honest
  CI-verification limit (0-spec trivial pass until DELIVER un-MARKs).
- [x] `apps/prism/e2e/slice-01-walking-skeleton.spec.ts` +
  `slice-03-error-and-empty-states.spec.ts` — the UNIMPLEMENTED bodies and
  the Given/When/Then contract I implement.
- [x] `apps/prism/playwright.config.ts` — the MARKed
  `testMatch: ['__no-spec-matches-yet__.spec.ts']`, the pinned
  `PROMETHEUS_IMAGE_DIGEST` SSOT, the chromium/firefox/webkit projects.
- [x] `apps/prism/e2e/global-setup.ts` — the digest-pinned docker fixture.
- [x] `apps/prism/src/lib/echarts/EChart.tsx` — confirmed: NO
  `data-prism-chart-painted` attribute exists anywhere in `src`; only
  `data-tick-count` (`:104`). The swallow (`:91-97`) is live only in a real
  browser (the `instance === null` early-return at `:90` is the jsdom guard).
- [x] `apps/prism/src/panels/query/QueryPanel.tsx`, `app/App.tsx`,
  `lib/echarts/buildOption.ts`, `lib/promql/queryRange.ts`,
  `lib/url-state/codec.ts`, `lib/auto-refresh/reducer.ts`, `vite.config.ts`,
  `public/config.json`, `e2e/fixtures/prometheus.yml` — the real
  query->render flow, the real backend label, the real URL encoding, and the
  mount-time fetch behaviour (see upstream-issues.md).

## What I implemented (the two in-scope spec bodies)

Scope is bounded by DISCUSS + ADR-0075 D5: slice-01 (paint) and slice-03
(empty + error states), in-scope blocks only. slice-02/04/05/06 stay
UNIMPLEMENTED and OUT of `testMatch`, untouched.

### slice-01-walking-skeleton.spec.ts (4 in-scope blocks implemented)

| Block | AC / story | Disposition | What it asserts |
|---|---|---|---|
| walking skeleton "type `up` -> chart" | AC-1.4, US-PR-01 | **IMPLEMENTED** | the three-part paint conjunction (below); the embedded `< 1000 ms` line dropped per the DESIGN back-prop note |
| chrome backend label | AC-6.1, US-PR-06 | **IMPLEMENTED** | the chrome names the real backend label from `/config.json` |
| chrome after paint | AC-6.3, US-PR-06 | **IMPLEMENTED** | the backend label survives a successful paint (reuses the paint signal) |
| URL roundtrip | AC-4.2, US-PR-04 | **IMPLEMENTED** | a fresh tab on the same URL repaints the same series count (drives Run explicitly; see upstream-issues.md) |
| KPI-1 p95, KPI-2 p95, operator-time | perf | **UNTOUCHED (throwing)** | latency KPIs; out of scope; DELIVER `test.fixme`s them on graduation |

### slice-03-error-and-empty-states.spec.ts (4 in-scope blocks implemented)

| Block | AC / story | Disposition | What it asserts |
|---|---|---|---|
| FM1 PromQL parse error | AC-3.2, US-PE-03 | **IMPLEMENTED** | VISIBLE parse-error banner + "Backend rejected this query." fallback; no painted chart; input interactive; URL preserved |
| FM3 HTTP 500 transport | AC-3.3, US-PE-03 | **IMPLEMENTED** | `route.fulfill(500)`; VISIBLE transport banner naming the backend + `http-status`; no painted chart; page interactive |
| FM4 empty result | AC-3.4, US-PE-02 | **IMPLEMENTED** | VISIBLE "No data for ..." empty-state (distinct from a blank canvas); no banner; no painted chart; URL preserved |
| parse -> empty -> success sequence | US-PE-02/03 | **IMPLEMENTED** | exercises the paint-signal RESET across queries; ends on a painted chart with no stale banner/empty-state |
| FM2 backend-unreachable, FM5 `/config.json` 404, FM6 malformed URL | error | **UNTOUCHED (throwing)** | out of US-PE-02/03 scope; DELIVER `test.fixme`s them on graduation |

The existing zero-uncaught-error `afterEach` invariant (`pageErrors` and
`consoleErrors` must be empty) is preserved and applies to every slice-03
block. See the DISTILL hardening note below.

## The three-part paint assertion — mapping per AC (ADR-0075 D1 ∧ D2 ∧ D3)

The headline observable is a falsifiable conjunction, NOT a hollow
DOM-exists check (ADR-0075 rejects alternative A). It reduces to "a real
browser drew real ink for a real non-empty series":

1. **Part 1 — paint signal (D1).** `await page.waitForSelector(
   '[data-prism-chart-painted="true"]')`. The attribute is `"true"` ONLY
   after the ECharts `finished` event fired with a non-empty rendered
   series; it is `"false"` initially, reset to `"false"` before each new
   query, and never set under jsdom. This is the **rendered-series** half of
   the observable (the signal is gated on `getOption().series` carrying
   data), so no separate `getOption()` reach-through is needed.
2. **Part 2 — canvas ink (D2).** `page.evaluate` over
   `[data-testid="chart-canvas"] canvas`, `getImageData`, sample every 64th
   pixel into a Set, assert `> 1` distinct value. Defeats the
   blank-canvas-that-looks-broken case on a same-origin, non-tainted canvas.
3. **Part 3 — corroborating count.** The accessible fallback `<table>`
   caption is parsed for `>= 1 series` and `>= 1 points` — a deterministic
   confirmation the data reached React.

| AC / story | Block | Part 1 (signal) | Part 2 (canvas ink) | Part 3 (caption count) |
|---|---|---|---|---|
| AC-1.4 / US-PR-01 | slice-01 walking skeleton | yes | yes | yes |
| AC-6.3 / US-PR-06 | slice-01 chrome-after-paint | yes | (label assertion follows the signal) | — |
| AC-4.2 / US-PR-04 | slice-01 URL roundtrip | yes (per tab) | — | series-count equality across tabs |
| US-PE-02/03 | slice-03 sequence | yes (final `up`) | — | — |

The empty/error observables are asserted by **VISIBLE text** distinct from a
blank canvas (D6): FM1 parse banner + fallback text, FM3 transport banner,
FM4 empty-state text — and each asserts the paint signal is **absent from
the DOM** (`[data-prism-chart-painted]` count 0), because the error/empty
states never mount `<EChart>`.

## RED-not-BROKEN proof — RUN (not merely reasoned)

Chromium and docker were both provisionable in this run, so the proof is an
actual execution, not an argument from source.

**Substrate.** macOS; docker 29.4.2 with the digest-pinned
`prom/prometheus@sha256:378f4e0...1000fe` image cached; the Vite dev server
(`pnpm dev`) on :5173; the project's own Playwright 1.49.1 with chromium
build v1148 (had to install the matching build — a stale global Playwright
had only build 1223). I ran ONLY the two specs via a temporary,
NON-COMMITTED `playwright.redproof.config.ts` that overrode only `testMatch`
(then deleted it); the tracked `testMatch` stayed MARKed throughout.

**Result (8 in-scope blocks, `--project=chromium`, the 6 out-of-scope
throwing blocks excluded via `--grep-invert`):**

```
4 failed
  slice-01 ... I see a genuinely painted chart (AC-1.4, US-PR-01)
  slice-01 ... the chrome still names the backend after the chart paints (AC-6.3)
  slice-01 ... a fresh tab on the same URL repaints the same chart (AC-4.2)
  slice-03 ... parse-error -> empty -> success sequence ends on a painted chart
4 passed (49.1s)
```

**The 4 failures are RED-on-assertion, not BROKEN-on-setup.** Every one
failed with exactly:

```
TimeoutError: page.waitForSelector: Timeout 15000ms exceeded.
  - waiting for locator('[data-prism-chart-painted="true"]') to be visible
```

The fixture came up, the dev server served, chromium launched, the SPA
rendered, the query ran against the real Prometheus — and the wait timed out
because `data-prism-chart-painted` does not exist in `EChart.tsx` (confirmed
by source read). This is the falsifiable RED the feature turns on (ADR-0075
C4).

**The 4 passes are existing-honesty regression guards, green against HEAD by
design** (US-PE-02/03 assert the EXISTING honest rendering per DISCUSS C1,
they do not introduce the feature): slice-01 AC-6.1 (the backend label),
slice-03 FM1 (the parse banner), FM3 (the HTTP-500 banner), FM4 (the
empty-state text). They must STAY green through DELIVER. They are NOT
Fixture Theater: they cover already-honest behaviour that is a different
concern from the paint signal; no fixture supplies the feature's output.

**Verified absence of BROKEN failures:** zero compile/import/setup errors
across the run (the two specs also `tsc --noEmit` clean inside the project
tsconfig program). The only non-paint failures in the full-file run were the
6 deliberately-untouched `throw UNIMPLEMENTED` blocks, which DELIVER converts
to `test.fixme` atomically on graduation.

## DISTILL hardening note — the zero-error invariant vs browser network noise

The first real-browser run surfaced a flaw in the prior scaffold's
`afterEach`: chromium logs failed HTTP responses (4xx/5xx) as resource-load
**console errors**, so the `consoleErrors).toEqual([])` invariant tripped on
FM1 (400) and FM3 (500) — making them RED for a reason that would persist
even after DELIVER's wiring (the 400/500 messages are emitted by the browser,
not the app). That is wrong: FM1/FM3 assert existing honest banners and must
go green. I narrowed the console capture to **exclude** messages starting
with `"Failed to load resource"` (browser network noise), while preserving
the ADR-0075 D3 surface: the app-emitted `console.error('[prism] ECharts
setOption failed', ...)` and genuine uncaught exceptions (`pageerror`) are
still captured, so a swallowed-then-surfaced paint failure still reds the
invariant. Post-fix, FM1/FM3 pass against HEAD; the sequence test still reds
cleanly on the paint timeout.

## testMatch stays MARKed (DELIVER un-MARKs atomically)

`playwright.config.ts` `testMatch` is UNCHANGED at
`['__no-spec-matches-yet__.spec.ts']`. I did NOT un-MARK it. DELIVER un-MARKs
it to exactly the two graduated specs ATOMICALLY with the production wiring,
so trunk's gate-7-prism-playwright stays at the honest 0-specs-trivial-pass
posture until the code that makes the paint tests green lands. The
`PROMETHEUS_IMAGE_DIGEST` SSOT is preserved byte-for-byte (1 occurrence,
unchanged).

## The precise production seam DELIVER owes (Mandate-7 equivalent)

No scaffold is needed: the specs are TypeScript that compiles and runs
against the app; they are RED by **timeout on a missing attribute**, not
BROKEN by a missing symbol. DELIVER (Crafty) must add, in `apps/prism/src`
ONLY (C9), the following seam so the 4 RED tests go green:

1. **The paint signal** on the `<EChart>` container `<div>` (the
   `role="figure"` element, parallel to `data-tick-count`):
   `data-prism-chart-painted`, literal `"false"` on mount/initial render;
   set `"true"` on the ECharts **`finished`** event IFF
   `instance.getOption().series` has at least one series with at least one
   data point; reset to `"false"` in the update effect **before**
   `setOption`. Never set under jsdom (the `instance === null` early-return
   guarantees no subscription).
2. **The catch-and-surface** (D3): on a real-browser `setOption` throw, leave
   the signal `"false"` AND `console.error('[prism] ECharts setOption
   failed', err)` — do NOT swallow, do NOT re-throw (a throw inside the
   `useEffect` would unmount the subtree and blank the page). Preserve the
   narrow jsdom canvas-probe skip verbatim.
3. **`test.fixme`** the 6 out-of-scope blocks (slice-01 KPI-1/KPI-2/
   operator-time; slice-03 FM2/FM5/FM6) with the disclosed reasons from
   ADR-0075 D5, so the graduated suite reports them as deferred rather than
   throwing.
4. **Un-MARK `testMatch`** to
   `['slice-01-walking-skeleton.spec.ts', 'slice-03-error-and-empty-states.spec.ts']`
   and correct the roadmap comment truthfully; preserve the digest SSOT.

Out of scope for DELIVER here (do NOT add): auto-run-on-mount (see
upstream-issues.md), a `window`-instance test seam (ADR-0075 alt B rejected),
a `data-prism-chart-empty` marker (D6: empty asserted by placement + text).

## Adapter / driving-entry coverage (hexagonal boundary)

prism is a browser SPA; the single **driving port** is the rendered SPA
itself, driven through the real user surface (navigate, type, press
Run/Enter) — every in-scope spec enters there, never through an internal
React component or a unit function. The **driven adapters** exercised with
REAL I/O by these e2e specs:

- **Prometheus query adapter** (`queryRange` -> `/api/v1/query_range` via the
  Vite dev proxy to the digest-pinned container): real HTTP, real series, in
  the walking skeleton, AC-6.3, URL roundtrip, FM4, the sequence.
- **The ECharts canvas renderer** (real chromium canvas): real `getImageData`
  pixel sampling in the walking skeleton (Part 2).
- **The config loader** (`/config.json`): real fetch on every `goto('/')`.
- **The transport-error path**: real `route.fulfill(500)` interception (FM3)
  and the real backend 400 (FM1).

These are `@real-io` by construction (real chromium + real Prometheus +
real dev server); there is no InMemory double in the e2e layer. The
component/unit InMemory coverage (jsdom Vitest) is a separate, pre-existing
suite (gate-6) and is out of this wave's scope.

## Self-review (no nested reviewer available in this autonomous run)

Reviewer dispatch: not nested-invocable here. SELF-REVIEW against the
nw-ad-critique-dimensions; verdict below.

| Dimension | Check | Verdict |
|---|---|---|
| 1 Happy-path bias | slice-03 is 4 in-scope error/empty blocks; across the two specs the error/empty share is ~50% (4 of 8 in-scope) | **PASS** |
| 2 GWT compliance | each block is Given (fixture + navigate) / When (single action: Run/Enter) / Then (observable); the sequence is a deliberate cumulative-state scenario with staged Then checks | **PASS** |
| 3 Business-language purity | titles/comments speak operator language (type `up`, see a chart, backend rejected, no data); technical selectors live in step code, not in the scenario intent | **PASS** |
| 4 Coverage completeness | US-PR-01, US-PR-04, US-PR-06, US-PE-02, US-PE-03 each have >= 1 implemented block; perf KPIs + FM2/5/6 disclosed as out-of-scope/fixme | **PASS** |
| 5 WS user-centricity | the walking skeleton title is a user goal ("I type `up` ... and I see a genuinely painted chart"); Then steps are user observations (a painted chart, a shareable URL, a named backend), not internal side effects | **PASS** |
| 6 Priority validation | the largest gap (the headline chart proven by NO test) is the one addressed; perf is correctly deferred (the wrong-problem trap avoided) | **PASS** |
| 7 Observable-behaviour assertions | every Then checks an observable: a visible attribute flip, canvas pixel non-uniformity, a visible caption/banner/empty-state, a URL string — no private state, no mock call-counts | **PASS** |
| 8 Traceability | each block tags its AC/story in the title; environment = the digest-pinned docker fixture referenced via global-setup (the WS Given) | **PASS** |
| 9 WS boundary proof (real I/O) | the WS drives the real SPA against the real Prometheus container and samples the real chromium canvas; deleting the real adapter would break the WS (it cannot pass on a double) — Strategy C honoured | **PASS** |
| Paint assertion genuinely falsifiable (not hollow) | PROVEN RED by run: the conjunction reduces to a real-browser canvas-drew-ink check and times out against HEAD | **PASS** |
| Empty/error observables are VISIBLE-text not blank | FM1/FM3/FM4 assert visible banner/empty text + signal-absent | **PASS** |
| RED-not-BROKEN proven | run output above: 4 RED on the paint timeout, 0 BROKEN, 4 existing-honesty passes | **PASS** |
| testMatch stays MARKed | unchanged `__no-spec-matches-yet__`; verified post-cleanup | **PASS** |
| chromium-only | the proof ran `--project=chromium`; firefox/webkit untouched | **PASS** |
| digest SSOT preserved | byte-for-byte; 1 occurrence, unchanged | **PASS** |

**Self-review verdict: APPROVED.** 0 critical, 0 high open. The two spec
bodies are implemented, the paint assertion is falsifiable and PROVEN RED in
a real browser, the empty/error states assert visible text, testMatch stays
MARKed, and the production seam is named precisely for DELIVER.

## Handoff summary

- **DELIVER** (`nw-software-crafter`): implement the production seam above
  (paint signal + catch-and-surface in `EChart.tsx`), `test.fixme` the 6
  out-of-scope blocks, un-MARK `testMatch` to the two graduated specs
  ATOMICALLY, keep Vitest green (C2) and the chart operator-invisible (C1),
  Gate 10 (StrykerJS) on the changed component logic (C10). Honour the honest
  CI limit (C6): no "CI-verified" claim until gate-7 is observed green with
  the un-MARKed specs. NEVER bump to 1.0.0.
- See `distill/upstream-issues.md` for two grounded observations (the backend
  label naming and the auto-run-on-mount gap) — neither blocks DELIVER.
</content>
