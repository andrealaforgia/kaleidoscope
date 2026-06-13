# ADR-0075 — Prism ECharts paint verification: the paint signal, the narrowed swallow, and the real-browser test architecture

- **Status**: Accepted
- **Date**: 2026-06-13
- **Author**: `nw-solution-architect` (Morgan)
- **Feature**: `prism-echarts-paint-e2e-v0`
- **Mode**: PROPOSE (autonomous)
- **Related**: ADR-0030 (prism ECharts integration shape — the `<EChart>`
  wrapper, the `setOption({notMerge:true})` update path, and the pure
  `buildOption` this ADR extends), ADR-0026 (prism component layout —
  `lib/echarts/` boundary), ADR-0027 (`QueryOutcome` and the error mapping
  that feeds the banners), ADR-0058 / ADR-0070 (perf-KPI CI gating posture
  this ADR defers to for the latency blocks).
- **Earned-Trust note (Principle 12)**: this ADR is the read-path-UI sibling
  of ADR-0049/0060 (honour-fsync) and ADR-0073 (body-size cap): the
  four-quadrants prism report (Q3 finding 2) flagged prism's **headline
  feature** — the ECharts chart that *is* the visual query result — as
  verified by **no automated test at all**. Under jsdom the entire ECharts
  lifecycle is skipped (`EChart.tsx:71-72`, the canvas-2D probe returns
  `null`), the update path swallows every paint failure
  (`EChart.tsx:91-97`), and `playwright.config.ts:57` rigs `testMatch` to
  match no spec. The chart is "verified" only by a human eyeballing a
  hand-built `dist/` bundle. The load-bearing honesty requirement here:
  the paint assertion MUST be the empirical probe that the chart honours
  its contract (it genuinely drew the returned series) **in the real
  browser substrate where it runs**, and it MUST fail against today's
  behaviour. A test that passes on a blank canvas is the dishonest gate
  this feature exists to retire (it is the inverse of the `claims-honesty-
  pass-2-v0` MARK).
- **Supersedes**: none.
- **Superseded by**: none.

## Context

`prism` v0 ships a single PromQL query panel. The operator (Priya) opens
the SPA at incident time, types `up`, presses Enter, and reads the shape of
the returned series off an Apache ECharts line chart. The chart is the only
thing she acts on and the product's headline feature.

The `<EChart>` wrapper (ADR-0030) holds the ECharts instance in a ref and
drives updates via `setOption({notMerge:true})`. Two seams make the chart
**unprovable** today, both verified on this branch:

1. **The jsdom lifecycle skip** (`apps/prism/src/lib/echarts/EChart.tsx:69-84`).
   The mount effect probes `document.createElement('canvas').getContext('2d')`;
   in jsdom (Vitest) that returns `null`, so `echarts.init` is never called
   and `instanceRef` stays `null`. No Vitest test ever instantiates the
   chart. This skip is **legitimate and must be preserved** (jsdom has no
   working canvas-2D); the problem is that nothing covers the real-browser
   path it deliberately excludes.
2. **The swallowed paint error** (`EChart.tsx:88-98`). The update effect
   wraps `instance.setOption(option, {notMerge:true})` in `try { } catch { }`
   with an empty body and the comment "jsdom: canvas paint unavailable".
   The comment is **wrong about its own reach**: the update effect early-
   returns at `if (instance === null) return;` (line 90), and `instance` is
   non-`null` **only** in a real browser (jsdom never inits). So the catch
   can fire **only** in a real browser — it swallows exactly the genuine
   paint failures an e2e test must catch, and never anything in jsdom.

The signal the specs already wait on — `[data-prism-chart-painted="true"]`
(`e2e/slice-01-walking-skeleton.spec.ts:64`) — **does not exist** in the
component (`grep data-prism-chart-painted apps/prism/src` → no matches; only
`data-tick-count` exists at `EChart.tsx:104`). The six Playwright specs
exist as detailed pseudocode but every `test()` body throws `UNIMPLEMENTED`,
and `testMatch` matches none.

DISCUSS (`docs/feature/prism-echarts-paint-e2e-v0/discuss/`) fixed the
**requirement** (a genuine, falsifiable paint proof; honest failure-mode
rendering; the narrow jsdom skip preserved; the SSOT and roadmap kept) and
flagged six DESIGN/DEVOPS decisions (D1-D6). This ADR resolves the five
DESIGN-owned decisions (D1, D2, D3, D5, D6) and states the D4 CI-browser
dependency and the honest limit for DEVOPS.

### Where the chart sits in the data flow (verified)

`QueryPanel` mounts `<EChart>` **only on a successful outcome** with a
non-empty series (`QueryPanel.tsx:270,381-385`: `showChart = outcome !==
null && outcome.kind === 'success'`). Empty results, parse errors,
transport errors, and config errors each render their own visible
element instead of the chart (`QueryPanel.tsx:331-379`). `queryRange` maps
a zero-series response to `outcome.kind === 'empty'`, never to a
zero-series success, so when `<EChart>` is mounted the option always
carries at least one series with at least one point (`buildOption.ts:104-118,137-139`).
This is the lever that makes D6 (empty-vs-paint reconciliation) fall out
for free: the paint signal can require a non-empty series without ever
contradicting the honest empty render, because the two states never share
the DOM.

## Decision

### D1 — The paint-signal contract (the falsifiable observable)

A doc-hidden boolean attribute, **`data-prism-chart-painted`**, on the
`<EChart>` container `<div>` (the `role="figure"` element, parallel to the
existing `data-tick-count` at `EChart.tsx:104`). Its lifecycle is the
contract; the crafter implements it, DISTILL asserts it, Scholar tests it.

| Transition | When | Mechanism |
|---|---|---|
| **initial = `"false"`** | first render, before any real paint | rendered into the JSX attribute as a literal `"false"`; never absent, never `"true"` on mount |
| **`false` → `true`** | the real ECharts render of a non-empty series completes | subscribe to the ECharts instance **`finished`** event in the mount effect (real-browser path only, after `echarts.init`); on `finished`, read `instance.getOption().series` and set `"true"` **iff** at least one series carries at least one data point |
| **`true` → `false`** (reset) | a new option arrives (new query result) | in the update effect, set the attribute to `"false"` **before** calling `setOption`, so a stale `"true"` from the prior render is never observable across queries; the next `finished` re-flips it |
| **stays `"false"`** | jsdom (Vitest) | `instance === null` ⇒ no `finished` subscription is ever made ⇒ the attribute is never set `"true"`; the narrow jsdom skip (`EChart.tsx:71-72`) is preserved |

Sketch (illustrative, **not** the implementation — the crafter owns the
code under `apps/prism/src`):

```ts
// mount effect, real-browser path only (after echarts.init):
const onFinished = (): void => {
  const series = instance.getOption().series as ReadonlyArray<{ data?: unknown[] }> | undefined;
  const hasInk = Array.isArray(series)
    && series.some((s) => Array.isArray(s.data) && s.data.length > 0);
  if (hasInk) containerRef.current?.setAttribute('data-prism-chart-painted', 'true');
};
instance.on('finished', onFinished);
// cleanup: instance.off('finished', onFinished) before dispose

// update effect, before setOption:
containerRef.current?.setAttribute('data-prism-chart-painted', 'false');
```

**Why `finished` and not `rendered`.** ECharts emits `rendered` on every
frame of an animation; `finished` fires once the render (including any
animation) has settled and the canvas is stable — the correct moment to
declare "painted". This also means the signal is robust to the
`prefers-reduced-motion` animation toggle (ADR-0030 §6): with animation off
`finished` fires immediately, with animation on it fires once.

**Why this is falsifiable** (C3, C4): a Playwright `waitForSelector('[data-
prism-chart-painted="true"]')` **cannot** pass on a mounted-but-unpainted
`<div>` (the attribute is `"false"`), **cannot** pass on the jsdom skip
(never subscribed), and **cannot** pass on an empty option (the `hasInk`
guard). It must therefore fail against today's behaviour (no attribute at
all) and pass only on a genuine non-empty paint.

### D2 — The non-blank-canvas assertion technique (DOM canvas pixel sampling)

The slice-01 walking-skeleton test, **after** the paint signal reads
`true`, proves the canvas carries real ink by sampling the rendered
`<canvas>` pixels in the browser and asserting non-uniformity:

```ts
// in-spec, page.evaluate (illustrative):
const distinct = await page.evaluate(() => {
  const canvas = document.querySelector('[data-testid="chart-canvas"] canvas') as HTMLCanvasElement;
  const ctx = canvas.getContext('2d')!;
  const { data } = ctx.getImageData(0, 0, canvas.width, canvas.height);
  const seen = new Set<string>();
  for (let i = 0; i < data.length; i += 4 * 64 /* stride */) {
    seen.add(`${data[i]},${data[i + 1]},${data[i + 2]},${data[i + 3]}`);
  }
  return seen.size;
});
expect(distinct).toBeGreaterThan(1); // not a single uniform colour
```

The `<canvas>` is a same-origin element ECharts' `CanvasRenderer` draws
into; `getImageData` on it is **not** tainted, so no `getDataURL` round-trip
is needed. The conjunction the test asserts is therefore:

1. `data-prism-chart-painted="true"` — ECharts `finished` fired with a
   non-empty series (D1; this is also the **rendered-series** half of the
   observable, since the attribute is gated on `getOption().series` having
   data, so a separate `getOption()` reach-through is unnecessary), AND
2. canvas pixels are non-uniform — ECharts drew ink (D2), AND
3. (supplementary, human-readable) the accessible fallback `<table>`
   caption reads "≥1 series · ≥1 points" (`QueryPanel.tsx:393-396`) — a
   deterministic series/point count that confirms the data reached React.

(1) ∧ (2) is the primary lock; (3) is corroborating and free (already
rendered). This defeats the "blank-that-looks-broken" case the DISCUSS
brief calls out.

### D3 — The swallow narrowing (surface, do not swallow; jsdom skip stays narrow)

The genuine jsdom guard is **already** the `if (instance === null) return;`
early-return at `EChart.tsx:90`: in jsdom the canvas probe is `null`, init
is skipped, `instanceRef` stays `null`, and `setOption` is therefore never
reached in jsdom. The surrounding `try { } catch { }` (lines 91-97) is dead
for jsdom and live **only** in a real browser, where it silently eats the
paint failures the e2e must catch.

The remediation: **on the real-browser path, surface a `setOption` failure
instead of swallowing it.** On `catch`, the component (a) leaves the paint
signal at `"false"` (it never flips, so the slice-01 `waitForSelector`
times out → red), and (b) emits a `console.error` naming the failure (so
the slice-03 zero-uncaught-error invariant — `consoleErrors` must be empty,
`slice-03-*.spec.ts:46-49` — trips → red). The page stays interactive
because the error is caught (not re-thrown) — a thrown error inside a React
`useEffect` would propagate and unmount the subtree, blanking the page and
violating C1 / US-PE-03 "the page stays interactive". So the shape is
**catch-and-surface**, not catch-and-rethrow and not catch-and-swallow.

Sketch (illustrative):

```ts
// update effect, real-browser path:
containerRef.current?.setAttribute('data-prism-chart-painted', 'false');
try {
  instance.setOption(option, { notMerge: true });
} catch (err) {
  // Real-browser paint failure: surface it. Signal never flips → the
  // walking-skeleton wait reds; console.error → the zero-error invariant reds.
  console.error('[prism] ECharts setOption failed', err);
}
```

The Vitest suite stays green (C2): jsdom never reaches `setOption`, so the
`console.error` is never emitted there; the existing ~125 blocks are
unaffected. The narrow canvas-probe skip is preserved verbatim.

### D5 — The `testMatch` un-MARK scope (graduate two specs, fence the perf and out-of-story blocks)

`testMatch` graduates from `['__no-spec-matches-yet__.spec.ts']` to:

```ts
testMatch: [
  'slice-01-walking-skeleton.spec.ts',
  'slice-03-error-and-empty-states.spec.ts',
],
```

`PROMETHEUS_IMAGE_DIGEST` (`playwright.config.ts:35-41`) is preserved
**byte-for-byte** as the SSOT shared with CI `gate-11`. The per-slice re-add
roadmap comment (`:43-57`) is **corrected truthfully**, not deleted: slices
01 and 03 are marked GREEN/graduated; slices 02, 04, 05, 06 remain
scaffold/UNIMPLEMENTED and out of the matcher. The un-MARK is the honest
inverse of the `claims-honesty-pass-2-v0` MARK (C5).

Within the two graduated files, the crafter implements only the in-scope
blocks and `test.fixme()`s the rest with a disclosed reason in the title:

| Spec file | Block | Disposition | Reason |
|---|---|---|---|
| slice-01 | walking-skeleton "type `up` → chart" | **IN** (US-PE-01) | the paint proof; assert the D1∧D2 conjunction, **drop the embedded `< 1000 ms` wall-clock line** (that is the latency KPI, out of scope — see back-prop note) |
| slice-01 | chrome backend-label tests (US-PR-06) | **IN** | deterministic, part of "type `up`, see chart, read the backend"; no wall-clock |
| slice-01 | URL-roundtrip (US-PR-04) | **IN** | deterministic; reuses the paint signal to confirm a fresh tab repaints |
| slice-01 | KPI-1 p95 (< 2 s) | **fixme** | perf KPI; MEMORY `p95_wallclock_flakes_overnight`; out of scope |
| slice-01 | KPI-2 p95 (< 800 ms) | **fixme** | perf KPI; same |
| slice-01 | operator-time guardrail (< 5 s median) | **fixme** | perf KPI; same |
| slice-03 | FM1 PromQL parse error | **IN** (US-PE-03) | visible banner + zero-error invariant |
| slice-03 | FM3 HTTP 500 transport | **IN** (US-PE-03) | `route.fulfill(500)`; visible banner + page interactive |
| slice-03 | FM4 empty result | **IN** (US-PE-02) | visible "No data" message text |
| slice-03 | parse → empty → success sequence | **IN** | exercises the paint-signal **reset** across queries and the no-stale-banner invariant |
| slice-03 | FM2 backend-unreachable (stop container) | **fixme** | stopping the **shared** global-setup container mid-suite conflicts with the shared-fixture model; the route-fulfilled 500 (FM3) is the in-scope transport proof |
| slice-03 | FM5 `/config.json` 404 | **fixme** | config-error; not in US-PE-02/03; graduates with the broader slice-03 feature |
| slice-03 | FM6 malformed URL | **fixme** | URL-codec behaviour (slice-05 territory); not paint/banner |

`test.fixme` (not deletion, not a silent skip) keeps the pseudocode visible
and the disclosed reason in the test title, so the suite reports them as
deferred rather than green-by-vacuum.

### D6 — Empty-state vs paint-signal semantics (no new marker needed)

An empty result does **not** mount `<EChart>` (`QueryPanel.tsx:270,381`
gate `showChart` on `outcome.kind === 'success'`); it renders
`[data-testid="empty-state"]` with the visible text "No data for {range}.
Check the metric name or widen the range." (`QueryPanel.tsx:369-373`).
Therefore the `data-prism-chart-painted` attribute **does not exist in the
DOM at all** in the empty state — it cannot be confused with a failed paint
(a mounted `<EChart>` whose signal is stuck `"false"`) nor with a painted
chart (signal `"true"` + canvas ink). The empty state is asserted by its
**visible message text**, exactly as DISCUSS requires.

No new `data-prism-chart-empty` marker is introduced: it would be redundant
surface area against the already-honest `empty-state` element and would
widen the component change beyond the paint signal + the narrowed swallow
(C1, minimal change). DESIGN reconciles D6 by **placement** (the two states
never share the DOM), not by a second attribute.

### D4 — The CI-browser dependency (FLAGGED for DEVOPS) and the honest limit

DESIGN states **what must run in CI**; DEVOPS (`nw-platform-architect`)
owns the **job mechanics**. What must run: the two un-MARKed specs under
**headless Chromium only** (C7 — the `chromium` Playwright project;
firefox/webkit projects stay defined but are not part of this feature),
with the pinned Prometheus fixture container available via docker
(`e2e/global-setup.ts`) and the Playwright Chromium browser installed
(`playwright install chromium`).

DEVOPS owns: the browser-install step, docker availability in the runner,
the `--project=chromium` scoping, and **crucially whether the job gates or
runs `continue-on-error`**. Consistent with the project's pure trunk-based,
no-required-checks posture (MEMORY `project_kaleidoscope_pure_trunk_based`,
C8), the lean is to start `continue-on-error` (feedback, not a gate) and
tighten to gating once the job is observed green and stable — but this is
DEVOPS's call, explicitly **not** resolved here.

**The honest limit (C6, load-bearing).** The paint assertion runs
**locally** today under headless Chromium (`pnpm playwright`, with docker
for the fixture). **Until the D4 CI-browser job is observed green, no wave
— DESIGN, DEVOPS, DELIVER, or the narrative/slides — may claim the chart is
"CI-verified".** The honest interim claim is **"verified locally under
headless Chromium; CI verification pending the browser job"**. This is the
exact discipline `claims-honesty-pass-2-v0` exists to enforce: do not
re-create an advertised-but-vacuous gate. The local run itself depends on
docker for the fixture container — a stated precondition, not a silent
assumption.

## Alternatives considered

### A (rejected) — DOM-existence check (`role=figure` present) as the paint proof

The cheapest assertion: wait for the `role="figure"` `<div>` to exist. It
**passes today against a blank canvas** and against the jsdom skip — it is
precisely the hollow, green-by-vacuum check this feature exists to retire
(DISCUSS C3, the high-probability false-confidence trap). Rejected because
it is not falsifiable against today's behaviour.

### B (rejected) — expose the ECharts instance on `window` and assert via `chart.getDataURL()` / `getOption()`

DISCUSS's non-binding lean. It works, but it requires a **production test
seam** (publishing the instance on `window`) purely for the test, widening
the component's surface (against C1's minimal-change intent). DOM `<canvas>`
`getImageData` sampling (D2) achieves the same non-uniformity proof on a
same-origin, non-tainted canvas **without** the seam, and the paint signal
(D1) already encodes the `getOption().series` non-emptiness, so the
reach-through buys nothing. Rejected on minimal-surface grounds; recorded
as a fallback if a future need (e.g. asserting exact series JSON across
tabs, slice-04/05) makes instance access worthwhile.

### C (rejected) — bare removal of the `try/catch` (let `setOption` throw)

Narrows the swallow by deletion. Rejected: a throw inside a React
`useEffect` propagates to React's error handling and, with no error
boundary around `<EChart>`, unmounts the subtree — **blanking the page**,
which violates "the page stays interactive" (US-PE-03) and C1. The
catch-and-surface shape (D3) reds the test **and** keeps the page alive.

### D (rejected) — `rendered` event instead of `finished` for the paint signal

`rendered` fires on every animation frame, so the signal would flap
`true`/`false` mid-animation and the test could observe a transient `true`
before the chart settles. `finished` fires once the render (and any
animation) settles. Rejected for `finished`'s settle-once semantics.

### E (rejected) — graduate all six specs (full e2e completion) in this feature

Maximal coverage in one feature. Rejected: slices 02/04/05/06 are largely
non-paint behaviours (URL codec, picker, auto-refresh reducer, a11y), a much
larger body, and the perf-KPI blocks carry the known overnight p95
wall-clock flake (MEMORY `p95_wallclock_flakes_overnight`). The carpaccio
keeps this feature to the paint proof (slice 01) + honest failure modes
(slice 03); the rest is named future work with its scaffold marks intact
(C5, DISCUSS Scope boundary).

## Consequences

### Positive

- **The headline feature is genuinely proven to paint.** The D1∧D2
  conjunction fails against today's behaviour (no signal, swallowed errors,
  no spec runs) and passes only on a real painted canvas (C4, the
  Earned-Trust crux).
- **The swallow can no longer hide a real-browser paint failure.** A genuine
  paint fault reds the test two independent ways (signal never flips;
  `console.error` trips the zero-error invariant).
- **The change is minimal and operator-invisible** (C1): a doc-hidden
  attribute + the `finished` subscription + the narrowed catch. No change to
  the visible chart, the fidelity flags (`buildOption.ts`), the palette, or
  the existing banners.
- **The jsdom skip and the Vitest suite are untouched** (C2): jsdom never
  reaches `setOption` or the `finished` subscription.
- **The SSOT and the roadmap survive** (C5): the digest is byte-identical;
  the roadmap comment is corrected, not deleted; the perf and out-of-story
  blocks are `test.fixme`d, not roped in.

### Negative

- **The paint signal is browser-only coverage.** It is dead under jsdom by
  construction, so the Vitest suite cannot exercise the `false → true`
  transition; only the Playwright run can. Mitigation: this is inherent to
  "paint" (jsdom has no canvas); the signal's *initial-false* and the
  narrowed-swallow's *jsdom-no-op* are both Vitest-observable.
- **Pixel-sampling assertions can be brittle.** Mitigation: assert mere
  non-uniformity (> 1 distinct sampled value) against the pinned fixture's
  known `up` series, not an exact image or a colour threshold; the fixture
  container is digest-pinned (SSOT).
- **The CI claim is gated on D4.** Until the browser job is green the honest
  claim is "verified locally" (C6). This is a deliberate honesty cost, not a
  defect.

### Earned-Trust framing (Principle 12)

The paint signal + canvas probe is the empirical proof that the chart
honours its contract — *it drew the returned series* — in the **real
browser substrate** where the operator runs it, not in a jsdom stand-in that
skips the draw. The narrowed swallow is the refusal to lie about a failure.
This is RED→GREEN applied to the product's headline visual contract: the
test must be able to fail (it reds against today), or it proves nothing.

## Verification

- **Falsifiability (C4)**: against HEAD (no `data-prism-chart-painted`,
  swallowed errors, `testMatch` matches none) the slice-01 walking-skeleton
  test cannot pass — the attribute never reaches `"true"`. After the wiring
  it passes only on a non-blank, non-empty paint.
- **D1**: Playwright asserts the attribute is `"false"`/absent before the
  first render and `"true"` only after `finished` with a non-empty series;
  the parse→empty→success sequence asserts the reset.
- **D2**: Playwright `getImageData` non-uniformity (> 1 distinct sampled
  value) on `[data-testid="chart-canvas"] canvas`.
- **D3**: a forced paint fault in a real browser leaves the signal `"false"`
  and emits a `console.error`, reddening the test; the Vitest suite stays
  green (jsdom never reaches `setOption`).
- **D5**: `testMatch` includes exactly the two graduated specs; the digest
  is byte-identical; the perf and out-of-story blocks are `test.fixme`d with
  disclosed reasons.
- **D6**: an empty query asserts the visible "No data" message; the
  `data-prism-chart-painted` attribute is absent from the DOM.
- **Gate 10 (StrykerJS, C10)**: the paint-signal branch (painted vs not)
  and the narrowed-swallow branch (jsdom-no-op vs real-browser-surface) are
  the mutation surface; pin them if the component logic is in the changed
  set.
</content>
</invoke>
