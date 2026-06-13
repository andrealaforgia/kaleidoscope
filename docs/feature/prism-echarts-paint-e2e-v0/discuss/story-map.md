# Story Map: prism-echarts-paint-e2e-v0

Author: Luna (nw-product-owner). Wave: DISCUSS. Date: 2026-06-13.
British English. No em dashes in body.

Companion to `user-stories.md` (US-PE-01, US-PE-02, US-PE-03) and
`wave-decisions.md` (D1-D6; Scope boundary; the CI-browser load-bearing
flag). This map fixes the backbone, marks the walking-skeleton posture,
and slices the three stories into two releases. It does not restate the AC
(they live per story in `user-stories.md`); it sequences them.

## Persona and emotional arc (single persona, lightweight UX)

Priya, the on-call operator, opens prism at 03:14, types a query, and reads
the chart to judge the signal. The secondary beneficiary is the maintainer
who must trust the headline feature ships verified. The arc is Confidence /
Problem Relief: today the chart is proven only by eyeballing a built bundle
(an unverified seam the platform sells hardest); after this feature, an
automated real-browser test genuinely paints the chart and fails loudly on
a blank, so Priya trusts the shape she reads and the maintainer trusts the
ship.

## Backbone (the read-path activity spine, left to right)

The horizontal sequence is the operator's read path through prism, not a
feature grouping. Each activity is a step the operator moves through.

```text
  OPEN PRISM          RUN A QUERY          CHART PAINTS          READ THE RESULT
  in a browser   ->   type PromQL,    ->   ECharts renders  ->   operator acts on
  (the SPA at         press Enter          the returned          the shape (or the
   the prism URL)     (the query           series on the         honest empty /
        |             input + Run)          canvas               error message)
        |                  |                    |                      |
   out of scope       happy path           IN SCOPE              IN SCOPE
   (the SPA loads,    (existing query      US-PE-01 (the gap:    US-PE-02 (empty
    unchanged)         pipeline,            no test ever          is honest, not
                       unchanged)           proves it paints)     blank); US-PE-03
                                                                  (failure is
                                                                  visible, not
                                                                  swallowed)
```

The SPA load and the existing query pipeline (fetch / parse / codec) are
unchanged. The feature owns the "chart paints" cell (which no automated
test exercises today) and the "read the result" cell (the empty and error
states, proven distinct from a blank / failed canvas).

### Backbone-to-story mapping

| Backbone activity | Owning story | Operator outcome |
|---|---|---|
| Chart paints | US-PE-01 | the headline chart genuinely paints a non-blank canvas with a rendered series, proven by a real-browser test that fails on a blank |
| Read the result (empty) | US-PE-02 | a no-data query shows an honest "no data" message, provably distinct from a blank that looks broken |
| Read the result (failure) | US-PE-03 | a query / paint failure renders a visible message and the page stays usable; a real-browser paint failure is surfaced, not swallowed |

## Walking skeleton: No (brownfield)

`F-Skeleton = No` (recorded in `wave-decisions.md`). prism exists, is
built, and ships a real ECharts integration, a real Prometheus client, a
URL codec, and an auto-refresh state machine (4Q report Q1). The 6 e2e
specs and the pinned Prometheus container fixture already exist. There is
no greenfield thread to stand up. Per the run brief, the FIRST slice is
nonetheless a thin end-to-end proof, defined below.

### Thinnest end-to-end slice (the brownfield equivalent of a walking skeleton)

The thinnest slice that delivers a verifiable operator-visible behaviour on
its own is:

> A genuine paint signal (D1) is wired into the ECharts wrapper; the
> slice-01 walking-skeleton spec body navigates to the prism URL against
> the pinned Prometheus fixture, types `up`, presses Enter, and asserts the
> canvas paints non-blank (D2) with at least one rendered series; the test
> is un-MARKed in `testMatch` and runs locally under headless Chromium; and
> the CI-browser job to run it headless is stood up (D4, DEVOPS). It is
> proven by ONE test that FAILS against today's behaviour (no paint signal,
> swallowed errors, no spec running) and passes only on a genuine paint.

That slice exercises the full vertical thread (open URL -> run query ->
real ECharts paint -> non-blank-canvas + rendered-series assertion ->
testMatch un-MARK -> CI job). The slice is NOT "add the paint attribute"
or "un-rig the matcher" alone; those are technical layers, not operator
outcomes, and neither is demonstrable to Priya by itself.

## Carpaccio slicing into releases

Two releases, sliced by operator outcome (chart-paints, then honest
failure-mode rendering), not by technical layer. Each is an end-to-end,
demonstrable real-browser behaviour.

### Release R1 (slice 1): the headline chart is proven to paint

| Field | Value |
|---|---|
| Scope | US-PE-01: the genuine paint signal (D1) + non-blank-canvas assertion (D2) + the slice-01 walking-skeleton paint test running locally + testMatch un-MARKed for it (D5, SSOT + roadmap preserved) + the CI-browser job stood up (D4, DEVOPS) |
| Operator outcome | the headline ECharts chart genuinely paints a non-blank canvas with a rendered series, proven by a real-browser test that fails loudly on a blank |
| Learning hypothesis | "A paint signal wired to the real ECharts render lifecycle, plus a non-blank-canvas probe, is enough to prove the headline chart paints in a real browser, falsifiable against today's blank / hollow state." Falsified if the paint signal cannot be made deterministic against the ECharts `finished` event, or if the canvas probe flakes; on falsification, narrow the assertion to the paint-signal + rendered-series lock and record the pixel-probe as a follow-on. |
| Demonstrable in one session | yes (one query, one real-browser paint assertion, run locally; the CI job's green is the D4 follow-through) |

### Release R2 (slice 2): failure modes render honestly, nothing is swallowed

| Field | Value |
|---|---|
| Scope | US-PE-02 (empty result is an honest, visibly distinct message) + US-PE-03 (parse / transport errors render visibly; a real-browser paint failure is surfaced, not swallowed - D3); the slice-03 empty / error tests un-MARKed |
| Operator outcome | a no-data query shows an honest "no data" message distinct from a blank; a query / paint failure renders a visible message and the page stays usable; a genuine paint failure reds the e2e |
| Learning hypothesis | "Narrowing the EChart swallow to the jsdom-only condition (D3) surfaces real-browser paint failures to the zero-uncaught-error invariant without regressing the Vitest jsdom suite, and the empty state is assertable by its visible message distinct from a blank canvas (D6)." Falsified if narrowing the swallow regresses the Vitest suite, or if the empty state cannot be told apart from a failed paint; on falsification, split US-PE-03's swallow remediation into its own slice ahead of the error-banner assertions. |
| Demonstrable in one session | yes (empty-result test + parse-error + transport-error tests, run locally) |

### Priority Rationale

Priority is by operator-outcome impact and dependency, not by feature
grouping.

1. **US-PE-01 (the chart paints) is the spine.** Without a genuine paint
   proof there is no coverage of the headline feature, and US-PE-02 /
   US-PE-03 both depend on the paint signal it introduces (the empty and
   failure states are defined relative to a genuine paint). It is also the
   highest-value outcome: it closes the exact Q3 gap and un-MARKs the
   claims-honesty scaffold. Highest priority, slice 1.
2. **US-PE-03 (failures are visible, not swallowed) protects against the
   worst incident-time outcome** (a blank chart Priya cannot interpret) and
   removes the real-browser swallow that would mask paint regressions. It
   rides in slice 2 with US-PE-02 because both are failure-mode rendering
   over the same slice-03 spec and the same paint-signal mechanism.
3. **US-PE-02 (honest empty state) makes the empty case trustworthy.** It
   is the lower-stakes of the two slice-2 stories (an empty render is less
   urgent than a swallowed failure) but shares the slice-03 spec, so it
   ships together. Within slice 2, US-PE-03's swallow remediation is the
   riskier change (it touches the component) and is the one to validate
   first if the slice is split.
4. **The CI-browser job (D4) follows R1 immediately.** It is the
   load-bearing DEVOPS item and the reason the gate was deferred; R1's
   local-green is real value, but the honest "CI-verified" claim waits on
   D4 going green (C6). DEVOPS owns gate-vs-continue-on-error.

## Carpaccio taste tests

| Taste test | Verdict | Evidence |
|---|---|---|
| Is each release an end-to-end, demonstrable operator outcome (not a technical layer)? | PASS | R1 paints a real chart and asserts non-blank canvas; R2 renders honest empty / error states. Both are demonstrable in a real browser in one session. The thinnest slice explicitly rejects "add the attribute" / "un-rig the matcher" as non-outcomes. |
| Does each release deliver verifiable value on its own? | PASS | R1 proves the headline chart paints (locally now, CI on D4); R2 proves failures are visible and unswallowed. Each is independently shippable and independently testable. |
| Is the highest-value work first? | PASS | US-PE-01 (the actual paint proof, closing the Q3 gap) leads; the failure-mode honesty rides in slice 2. |
| Right-sized (1-3 days, 3-7 scenarios each)? | PASS | Three stories, 3 scenarios each (10 total counting the shared falsifiability edges), one app (`apps/prism`), one persona, no new UI. See Scope Assessment. |
| Does the change avoid altering operator-visible behaviour? | PASS | The paint signal is a doc-hidden attribute (C1); the empty / error banners are asserted, not redesigned. The only behaviour change is narrowing the real-browser paint swallow (US-PE-03), which surfaces failures that were silently eaten. |
| Could a slice ship a half-truth (a paint test that passes on a blank)? | GUARDED | C3 / C4: every paint AC must fail against today's blank / hollow state and pass only on a genuine paint. The honest-limit note (C6) forbids any "CI-verified" claim before the D4 job is green, so R1 cannot oversell a green-by-vacuum gate. |

## Scope Assessment: PASS -- 3 stories, 1 bounded context (apps/prism), estimated 2-3 days

Oversized signals checked (none tripped):

- User stories: 3 (threshold > 10). PASS.
- Bounded contexts / modules: 1 app, `apps/prism`; the touched surfaces
  (`src/lib/echarts/EChart.tsx`, the `e2e/slice-01` and `e2e/slice-03`
  spec bodies, `playwright.config.ts` testMatch, and the DEVOPS CI job) are
  all within prism + its CI (threshold > 3 bounded contexts). PASS.
- Walking-skeleton integration points: WS = No (brownfield); the thinnest
  slice has one vertical thread (open URL -> query -> paint -> assert),
  against one external fixture (the pinned Prometheus container, already
  wired) (threshold > 5). PASS.
- Estimated effort: 2-3 days; one paint-signal wiring + one swallow
  narrowing + two spec-body implementations (slice-01 paint, slice-03
  empty / error) + one testMatch un-MARK + the DEVOPS CI job (threshold >
  2 weeks). PASS.
- Independent shippable outcomes: R1 (paint proof) and R2 (failure-mode
  honesty) could ship separately and are sliced as such; they share the
  paint-signal mechanism and one persona. This is a controlled carpaccio
  split, not an oversized feature. PASS.

No split required for size. The two-slice plan is the outcome slicing, not
a size remedy. Proceeding without restructuring.

Explicitly OUT OF SCOPE to keep the carpaccio honest (full rationale in
`wave-decisions.md` > Scope boundary): the 4 remaining specs (slice-02
time range, slice-04 auto-refresh, slice-05 absolute / permalink, slice-06
accessibility); the two p95 perf-KPI blocks and the operator-time guardrail
inside slice-01 (known overnight p95 flake); the Firefox / WebKit
browser-matrix breadth; and prism's missing dashboarding scope. These are
named future work; the scaffold marks they would retire stay in place
(`claims-honesty-pass-2` follow-up item 12).

## Verified loci note (feeds D1/D3/D5, confirmed on this branch)

- Paint skip + swallow: `apps/prism/src/lib/echarts/EChart.tsx:69-84` (the
  canvas-2D probe jsdom skip), `:88-98` (the swallowed `setOption` catch).
- The paint signal does NOT exist: `grep data-prism-chart-painted
  apps/prism/src` -> no matches; only `data-tick-count` exists
  (`EChart.tsx:104`). The specs wait on `[data-prism-chart-painted="true"]`
  (`e2e/slice-01-walking-skeleton.spec.ts:63`).
- The matcher is rigged: `apps/prism/playwright.config.ts:57`
  (`testMatch: ['__no-spec-matches-yet__.spec.ts']`); the per-slice re-add
  roadmap is at `:48-56`; the `PROMETHEUS_IMAGE_DIGEST` SSOT is at
  `:35-41`.
- The specs exist as UNIMPLEMENTED pseudocode: `e2e/slice-0{1..6}-*.spec.ts`
  (6 files, 44 blocks, every body throws `UNIMPLEMENTED`); the fixture
  container is started in `e2e/global-setup.ts:42-53`.
- The zero-uncaught-error invariant is scaffolded:
  `e2e/slice-03-error-and-empty-states.spec.ts:33-49`.
</content>
