<!-- markdownlint-disable MD024 -->

# User Stories: prism-echarts-paint-e2e-v0

## Origin and Job Grounding

No DIVERGE artifacts exist for this feature
(`docs/feature/prism-echarts-paint-e2e-v0/diverge/` is absent). Origin is
the four-quadrants prism report
(`kaleidoscope-4-quadrants-theory/reports/prism.md`, Q3 finding 2): the
ECharts chart, prism's headline feature, has never been verified to paint
by any automated test. `EChart.tsx:69-84` skips the entire ECharts
lifecycle under jsdom, `EChart.tsx:91-97` swallows paint errors, the 6
Playwright specs all throw `UNIMPLEMENTED`, and
`playwright.config.ts:57` rigs `testMatch` to match nothing. This feature
is the honest completion of the e2e that `claims-honesty-pass-2-v0` MARKed
"NOT YET IMPLEMENTED", building the genuine coverage behind that MARK while
preserving the `PROMETHEUS_IMAGE_DIGEST` SSOT and the per-slice roadmap.
Full framing in `wave-decisions.md`. Absence of DIVERGE is recorded there
as a risk; it does not block, because the gap and the fix direction are
verified directly in code.

## The Job (JTBD, verification-of-the-headline-feature framing)

> **When** an on-call operator opens prism at 03:14, types a query, and
> presses Enter, **I want** the headline ECharts chart to genuinely paint
> the returned series in the browser (not a blank canvas, not a hollow DOM
> node), **so that** the operator can trust the shape they read is the real
> signal, and the maintainer can trust the product's headline feature is
> verified by an automated real-browser test before it ships, instead of by
> human inspection of a hand-built bundle.

## System Constraints

(Full text in `wave-decisions.md` > Constraints established. Pinned here
for the crafter and the reviewer.)

- **C1 - prism is the headline UI.** No change to the operator-visible
  chart, the fidelity flags (`buildOption.ts`), the palette default, or the
  existing error / empty banners. The paint signal is a doc-hidden
  attribute.
- **C2 - The jsdom skip stays narrow; the Vitest suite stays green.** The
  canvas-2D probe skip (`EChart.tsx:69-84`) is preserved for jsdom; the
  genuine paint assertion lives only in the real-browser Playwright run;
  the existing 125 Vitest blocks stay green.
- **C3 - Genuine paint, not a hollow check.** The assertion is the
  paint-signal + non-blank-canvas + rendered-series conjunction, not the
  presence of a DOM node.
- **C4 - Falsifiable against today.** Every paint AC must FAIL against
  today's behaviour and pass only when the chart genuinely paints.
- **C5 - Preserve the SSOT and the roadmap.** The `PROMETHEUS_IMAGE_DIGEST`
  pin and the per-slice re-add roadmap are preserved and corrected
  truthfully on graduation; the un-MARK is the honest inverse of the
  `claims-honesty-pass-2` MARK, not a deletion.
- **C6 - No "CI-verified" claim before the CI job is green.** Honest
  interim claim is "verified locally under headless Chromium".
- **C7 - Headless Chromium only**; the Firefox / WebKit matrix is future.
- **C8 - Pure trunk-based, no CI gates**; the CI-browser job is feedback
  first (D4, DEVOPS).
- **C9 - Only the crafter writes `apps/prism/src`** (CLAUDE.md).
- **C10 - Gate 10 (StrykerJS)** applies if the component logic changes; pin
  the paint-signal branch and the narrowed-swallow branch.

The DESIGN-owned mechanism decisions (D1 paint signal, D2 canvas-assertion
technique, D3 swallow remediation, D4 CI-browser job, D5 testMatch
un-MARK scope, D6 empty-vs-paint semantics) are stated in
`wave-decisions.md`. The stories below encode the REQUIREMENT; they do not
prescribe the mechanism.

---

## US-PE-01: The headline chart is proven to genuinely paint in a real browser

### Problem

Priya is the on-call SRE for a multi-tenant Kaleidoscope deployment. At
03:14 a payments alert fires; she opens prism, types `up`, presses Enter,
and reads the shape of the line off the chart to decide whether the signal
is degrading. The chart is the product's headline feature and the only
thing she acts on. Yet today no automated test ever instantiates that
chart and asserts it paints: under jsdom the ECharts lifecycle is skipped
entirely (`EChart.tsx:69-84`, a canvas-2D probe returns `null` so
`echarts.init` is never called), and in Playwright no spec runs at all
(`playwright.config.ts:57` matches `__no-spec-matches-yet__.spec.ts`,
every one of the 44 spec blocks throws `UNIMPLEMENTED`). The chart is
verified only by a hand-built `dist/` bundle and someone eyeballing it.
Priya, and the maintainer who ships prism, find it impossible to trust the
headline feature actually works, because the one thing the product exists
to do, paint a query result, is proven by no automated test, and a blank
canvas that rendered nothing would pass every check there is today.

### Elevator Pitch

- **Before**: a developer opens prism, types `up`, and a chart appears,
  but no automated test ever confirms it; the ECharts lifecycle is skipped
  under jsdom, paint errors are swallowed (`EChart.tsx:91-97`), and the
  Playwright matcher matches nothing. A blank canvas would pass every
  existing check.
- **After**: on the running prism SPA (the operator opens the prism URL
  `http://localhost:5173/`, types `up` into the PromQL query input, and
  presses Enter), the headline ECharts chart genuinely paints the returned
  series: the canvas carries real ink (a non-blank canvas, not a uniform
  background), the chart reports at least one rendered series with at least
  one point, and a real-browser Playwright test running under headless
  Chromium asserts all of this and fails loudly on a blank or hollow
  render. The slice-01 walking-skeleton paint test is un-MARKed in
  `testMatch` and runs locally; the CI-browser job to run it headless is
  stood up (DEVOPS owns whether it gates, D4).
- **Decision enabled**: Priya reads the chart shape and trusts it is the
  real signal, not an empty render masquerading as "no data"; and the
  maintainer decides prism's headline feature is fit to ship on the
  strength of an automated real-browser paint proof, not human inspection.

### Who

- Priya the on-call operator | opens prism in a browser at incident time,
  types a PromQL query, reads the chart to judge the signal | motivated to
  trust the painted chart is the real returned series, not a blank canvas.
- The Kaleidoscope maintainer / release engineer | ships prism | motivated
  to have the headline chart-rendering path verified by an automated
  real-browser test before release, replacing eyeballing a built bundle.

### Solution

Wire a genuine paint signal into the ECharts wrapper (D1), toggled `true`
only after a real ECharts render of a non-empty series completes, parallel
to the existing `data-tick-count`. Implement the slice-01 walking-skeleton
spec body: navigate to the prism URL against the pinned Prometheus fixture
container, type `up`, press Enter, wait on the paint signal, and assert
(a) the canvas is not a uniform blank (D2), and (b) the chart reports at
least one series with at least one point (D3 of the observable). Un-MARK
`testMatch` to include this spec (D5), preserving the digest SSOT and the
roadmap (C5). Stand up the CI-browser job to run it headless (D4, DEVOPS).
The jsdom skip stays narrow and the Vitest suite stays green (C2). DESIGN
owns the paint-signal mechanism, the canvas-assertion technique, and the
testMatch scope; this story encodes the requirement that the headline
chart is genuinely proven to paint.

### Domain Examples

#### 1: Happy Path - `up` paints a real line against the fixture Prometheus

Priya opens `http://localhost:5173/`, types `up`, presses Enter. The
fixture Prometheus (self-scraping, pinned by `PROMETHEUS_IMAGE_DIGEST`)
returns the `up` series. Within about a second the ECharts canvas paints a
line: the rendered canvas carries non-background ink, the chart reports one
series (`up`) with multiple points, and the doc-hidden paint signal reads
`true`. The walking-skeleton test asserts all three and passes.

#### 2: Edge - the paint signal is false before the real render completes

On the same flow, immediately after the page loads but before the first
query renders, the paint signal is absent / `false`: a test that waited on
`[data-prism-chart-painted="true"]` cannot pass on the mounted-but-unpainted
chart container. The signal flips `true` only after the real ECharts
`finished` render event fires with a non-empty series. This guards against
the hollow "a `role=figure` div exists" check passing prematurely.

#### 3: Error / falsifiability - a blank canvas fails the paint proof

If the chart mounted but ECharts never painted (the canvas is a uniform
blank, exactly today's jsdom-skipped state surfaced in a browser), the
non-blank-canvas probe fails and the rendered-series assertion finds no
series, so the test FAILS. This is the falsifiability guard: the test must
fail against today's behaviour (no paint signal, swallowed errors, no spec
running) and pass only on a genuinely painted canvas.

### UAT Scenarios (BDD)

#### Scenario: Typing a query paints the headline chart in a real browser

```gherkin
Given prism is open in a real browser against the pinned Prometheus fixture
And the fixture returns the "up" series
When Priya types "up" into the query input and presses Enter
Then the chart canvas paints a non-blank image with real ink
And the chart reports at least one series with at least one point
And the doc-hidden paint signal reads painted
```

#### Scenario: The chart is not reported as painted until it genuinely renders

```gherkin
Given prism has just loaded in a real browser with no query run yet
When the page is inspected before the first render completes
Then the doc-hidden paint signal does not read painted
And it flips to painted only after the real ECharts render completes with a non-empty series
```

#### Scenario: A blank canvas fails the paint proof (falsifiability)

```gherkin
Given a chart container is mounted but the canvas was never painted
When the walking-skeleton paint test runs against it
Then the non-blank-canvas assertion fails
And the rendered-series assertion finds no series
And the test fails loudly rather than passing on the hollow DOM node
```

### Acceptance Criteria

- [ ] In a real browser, typing `up` and pressing Enter paints a non-blank
  ECharts canvas reporting at least one series with at least one point, and
  the doc-hidden paint signal reads painted (from scenario 1).
- [ ] The paint signal does not read painted on mere mount; it flips only
  after a real ECharts render of a non-empty series completes (from
  scenario 2).
- [ ] The paint test fails against a blank / hollow canvas and against
  today's behaviour (no paint signal, swallowed paint errors, no spec
  running); it passes only on a genuine paint (from scenario 3, C4).
- [ ] `testMatch` is un-MARKed to include the slice-01 walking-skeleton
  paint test, the test runs locally under headless Chromium, and the
  `PROMETHEUS_IMAGE_DIGEST` SSOT and the per-slice roadmap are preserved
  (C5).
- [ ] The jsdom skip stays narrow and the existing Vitest suite stays green
  (C2).

### Outcome KPIs

- **Who**: the Kaleidoscope maintainer shipping prism, and Priya reading
  the chart at incident time.
- **Does what**: gains an automated real-browser test that genuinely
  instantiates and paints the ECharts chart, instead of relying on human
  inspection of a built bundle.
- **By how much**: automated tests that genuinely paint the chart move from
  0 (today the lifecycle is jsdom-skipped and no spec runs) to at least 1
  (the walking-skeleton paint test); the paint test passes only on a
  non-blank canvas and fails against today's behaviour.
- **Measured by**: the slice-01 walking-skeleton paint test asserting the
  paint signal + non-blank canvas + rendered series, included in
  `testMatch` (was `__no-spec-matches-yet__`), run under headless Chromium.
- **Baseline**: 0 tests paint the chart; `EChart.tsx:69-84` skips ECharts
  under jsdom; `playwright.config.ts:57` matches no spec; all 44 spec
  blocks throw `UNIMPLEMENTED`.

### Technical Notes

- Depends on D1 (paint-signal mechanism), D2 (canvas-assertion technique),
  D5 (testMatch un-MARK scope), and D4 (the CI-browser job - DEVOPS, the
  load-bearing item). All flagged in `wave-decisions.md`.
- The paint signal is a doc-hidden attribute parallel to `data-tick-count`
  (`EChart.tsx:104`); the specs already reference
  `[data-prism-chart-painted="true"]` (`slice-01-*.spec.ts:63`).
- Honest limit (C6): runs locally now; no "CI-verified" claim until the D4
  job is observed green. Local run needs docker for the fixture container
  (`e2e/global-setup.ts`).
- The two p95 perf-KPI blocks and the operator-time guardrail in
  `slice-01-*.spec.ts` are OUT OF SCOPE (Scope boundary); D5 splits or
  `test.fixme`s them so they are not roped into the un-MARK (MEMORY p95
  flake).

---

## US-PE-02: An empty result renders an honest empty state, not a blank that looks broken

### Problem

Priya types `up{job="nonexistent"}` during an incident; the query is valid
but matches no series. The honest answer is "no data for this range";
today the chart area shows a calm "No data" message and no warning banner
(verified in the component, 4Q report Q2 INFO). But there is no automated
real-browser test that this empty render is DISTINGUISHABLE from a blank
canvas that painted nothing because the chart silently failed. To Priya at
03:14, an empty-but-honest chart and a broken-blank chart look identical if
both are just an empty rectangle. She needs to know, the instant she sees
it, whether the metric genuinely has no data in this window (widen the
range, check the metric name) or whether prism failed to paint (a bug, not
a signal). Priya finds it impossible to act confidently on an empty chart,
because nothing proves the empty state is an honest "no data" message and
not a silent paint failure wearing the same blank face.

### Elevator Pitch

- **Before**: a no-data query shows an empty chart area; the component does
  render a "No data" message, but no automated real-browser test asserts
  the empty state is visibly distinct from a blank canvas that failed to
  paint. The two look the same to the operator and to the (non-existent)
  test.
- **After**: on the running prism SPA, when Priya runs a valid query that
  returns no series (`up{job="nonexistent"}`), the chart area shows a calm,
  legible "No data for {range}. Check the metric name or widen the range."
  message, no warning banner appears, the URL still encodes the query, and
  a real-browser test asserts the visible message text (not merely the
  absence of a series), so an honest empty render is provably distinct from
  a blank-that-looks-broken.
- **Decision enabled**: Priya sees an empty chart, reads the explicit "no
  data" message, and decides to widen the range or fix the metric name,
  rather than wondering whether prism is broken.

### Who

- Priya the on-call operator | runs valid queries that sometimes match no
  series at incident time | motivated to tell an honest "no data" apart
  from a silent paint failure at a glance, so she chases the right problem.

### Solution

Implement the slice-03 empty-result spec body: open prism with a valid
query that returns no series against the fixture, and assert the VISIBLE
"No data..." message text is present, no warning banner is shown, and the
URL still encodes the (empty-yielding) query. Reconcile the empty state
with the paint signal (D6) so the empty render is asserted by its message,
distinct from both a painted-with-data chart and a blank / failed canvas. A
negative control asserts a successful `up` query does NOT show the empty
message. DESIGN owns the empty-vs-paint-signal semantics (D6); this story
encodes the requirement that an empty result is an honest, visibly distinct
state.

### Domain Examples

#### 1: Empty result - the calm "no data" message renders

Priya runs `up{job="nonexistent"}`. The fixture returns a valid empty
result. The chart area shows "No data for the last 15 min. Check the metric
name or widen the range." No warning banner appears (this is not an error).
The URL still reads the query. The test asserts the visible message text.

#### 2: Edge - the empty state is visibly distinct from a blank canvas

The empty render shows legible message text in the chart area, not an empty
rectangle. The real-browser test reads the message string, so an honest
empty state cannot be confused with a canvas that painted nothing because
the chart silently failed (which would show no message).

#### 3: Negative control - a good query does not show the empty message

Priya runs `up` (which returns data). The chart paints a line and the "No
data" message is absent. This guards against the empty message leaking onto
a successful render.

### UAT Scenarios (BDD)

#### Scenario: A valid query with no matching series shows an honest empty message

```gherkin
Given prism is open in a real browser against the pinned Prometheus fixture
When Priya runs a valid query that matches no series
Then the chart area shows a legible "no data" message naming the range
And no warning banner is shown
And the URL still encodes the query
```

#### Scenario: The empty state is visibly distinct from a blank canvas

```gherkin
Given a valid query returned no series
When the empty state is inspected in the browser
Then the visible "no data" message text is present in the chart area
And the empty state is asserted by that message, not by the mere absence of a series
```

#### Scenario: A successful query does not show the empty message

```gherkin
Given prism is open in a real browser against the pinned Prometheus fixture
When Priya runs "up" which returns data
Then the chart paints a line
And the "no data" message is not shown
```

### Acceptance Criteria

- [ ] A valid query returning no series shows a legible "no data" message
  naming the range, with no warning banner, and the URL still encodes the
  query (from scenario 1).
- [ ] The empty state is asserted by its visible message text, making it
  provably distinct from a blank canvas that failed to paint (from scenario
  2, D6).
- [ ] A successful query does not show the empty message (negative control,
  from scenario 3).
- [ ] The slice-03 empty-result test is un-MARKed in `testMatch` and runs
  locally under headless Chromium (C5); the Vitest suite stays green (C2).

### Outcome KPIs

- **Who**: Priya reading an empty chart at incident time.
- **Does what**: tells an honest "no data" apart from a silent paint
  failure by reading explicit message text, instead of guessing from a
  blank rectangle.
- **By how much**: real-browser tests asserting the empty state is visibly
  distinct from a blank canvas move from 0 to 1; the empty message is
  asserted by visible text, not series-absence.
- **Measured by**: the slice-03 empty-result test asserting the visible
  "no data" message, no banner, URL preserved, plus the successful-query
  negative control.
- **Baseline**: the component renders an empty message today (4Q Q2 INFO),
  but no automated real-browser test asserts it is distinct from a blank /
  failed canvas.

### Technical Notes

- Depends on US-PE-01 (the paint signal and the slice-1 mechanism) and on
  D6 (empty-vs-paint-signal semantics).
- Part of slice 2 (honest failure-mode rendering); un-MARKed together with
  US-PE-03 in the slice-03 spec.
- No change to the operator-visible empty banner copy (C1); this story
  asserts the existing honest behaviour, it does not redesign it.

---

## US-PE-03: A paint or query failure is surfaced visibly, not swallowed into a blank

### Problem

When Priya types an invalid PromQL expression, or the backend returns a
500, or the chart genuinely fails to paint, the worst outcome at 03:14 is a
blank chart with no explanation: she cannot tell a failure from "no data",
and she loses time. Today the component does render a warning banner for
parse and transport errors (4Q report Q2 INFO), but the ECharts update path
swallows paint errors entirely (`EChart.tsx:91-97`: `try { setOption() }
catch { }` with an empty body), and no real-browser test asserts that a
failure is visible rather than a silent blank. In a real browser that
swallow would hide a genuine paint failure from any e2e check. Priya needs
every failure mode to render a visible message and the page to stay
interactive, and the maintainer needs the automated test to FAIL LOUDLY on
a real-browser paint failure instead of the error being quietly eaten.
Priya finds it impossible to trust a blank chart, because a swallowed paint
error and an honest "no data" are indistinguishable, and nothing proves a
failure surfaces at all.

### Elevator Pitch

- **Before**: parse and transport errors render a banner, but paint errors
  on the chart update path are swallowed silently (`EChart.tsx:91-97`), and
  no real-browser test asserts failures are visible; in a browser the
  swallow would hide a genuine paint failure from every check.
- **After**: on the running prism SPA, when Priya enters an invalid PromQL
  query or the backend rejects it (a 500), a warning banner appears with the
  backend's error text, the page stays interactive, the URL is preserved,
  and no uncaught console error escapes; and crucially a genuine paint
  failure in a real browser is no longer swallowed: it surfaces (the paint
  signal never flips and/or a console error is emitted) so the real-browser
  test fails loudly. The jsdom-only swallow stays narrow so the Vitest suite
  is unaffected.
- **Decision enabled**: Priya sees a visible error, reads the cause, and
  decides to fix her query or escalate the backend outage, instead of
  staring at a blank chart; and the maintainer is alerted by a red test the
  moment the chart stops painting, instead of the failure being eaten
  silently.

### Who

- Priya the on-call operator | makes query typos and hits backend outages at
  incident time | motivated to see a visible, legible failure rather than a
  blank chart she cannot interpret.
- The Kaleidoscope maintainer | relies on the e2e to catch regressions |
  motivated to have a genuine paint failure surface as a red test, not be
  swallowed.

### Solution

Implement the slice-03 parse-error and transport-error spec bodies in a
real browser: assert a warning banner with the backend's verbatim error
text, the page stays interactive, the URL is preserved, and no uncaught
console error escapes (the zero-uncaught-error invariant the spec already
sets up at `slice-03-*.spec.ts:37-49`). Narrow the `EChart.tsx:91-97`
swallow (D3) so it only applies under the genuine jsdom condition (the
canvas-2D probe being `null`), and in a real browser a paint failure
surfaces (the paint signal never flips and/or a console error is emitted),
so the e2e reds. The jsdom skip stays narrow and the Vitest suite stays
green (C2). DESIGN owns the exact swallow remediation (D3); this story
encodes the requirement that failures are visible and a real-browser paint
failure is not swallowed.

### Domain Examples

#### 1: Error - an invalid PromQL query shows a visible banner

Priya opens prism at `/?q=invalid syntax)(`. The backend rejects the
parse; a warning banner appears with the backend's verbatim error text, the
chart area shows "Backend rejected this query.", the query input stays
focusable, the URL still encodes the broken query, and no uncaught console
error escapes. The real-browser test asserts the visible banner and the
zero-error invariant.

#### 2: Error - a backend 500 renders inline and the page stays usable

Priya runs a query while the backend returns HTTP 500. A warning banner
appears mentioning the failure, the page stays interactive, and no uncaught
console error escapes. Priya reads the error and escalates the backend
outage rather than staring at a blank chart.

#### 3: Edge / falsifiability - a genuine paint failure is not swallowed

If the chart genuinely fails to paint in a real browser (a forced paint
fault), the `EChart.tsx` catch no longer swallows it silently: the paint
signal never flips to painted and / or a console error is emitted, so the
zero-uncaught-error e2e invariant trips and the test FAILS. The jsdom-only
swallow stays narrow, so the Vitest jsdom suite is unaffected. This is the
falsifiability guard against the swallow masking real-browser failures.

### UAT Scenarios (BDD)

#### Scenario: An invalid query renders a visible warning, page stays usable

```gherkin
Given prism is open in a real browser against the pinned Prometheus fixture
When Priya runs an invalid PromQL query
Then a warning banner appears with the backend's error text
And the query input stays focusable
And the URL still encodes the query
And no uncaught console error escapes
```

#### Scenario: A backend error renders inline without blanking the page

```gherkin
Given the backend returns an HTTP 500 for the next query
When Priya runs a query
Then a warning banner appears naming the failure
And the page stays interactive
And no uncaught console error escapes
```

#### Scenario: A real-browser paint failure is surfaced, not swallowed

```gherkin
Given the chart genuinely fails to paint in a real browser
When the page renders
Then the failure surfaces rather than being silently swallowed
And the paint signal does not read painted and or a console error is emitted
And the real-browser test fails loudly
And the narrow jsdom-only skip leaves the Vitest suite unaffected
```

### Acceptance Criteria

- [ ] An invalid PromQL query renders a visible warning banner with the
  backend's error text, the input stays focusable, the URL is preserved,
  and no uncaught console error escapes (from scenario 1).
- [ ] A backend HTTP 500 renders an inline warning, the page stays
  interactive, and no uncaught console error escapes (from scenario 2).
- [ ] A genuine paint failure in a real browser is surfaced, not swallowed:
  the paint signal does not flip and / or a console error is emitted, so the
  e2e fails loudly; the jsdom-only swallow stays narrow and the Vitest suite
  stays green (from scenario 3, D3, C2).
- [ ] The slice-03 parse-error and transport-error tests are un-MARKed in
  `testMatch` and run locally under headless Chromium (C5).

### Outcome KPIs

- **Who**: Priya hitting query typos and backend outages, and the
  maintainer relying on the e2e to catch paint regressions.
- **Does what**: sees every failure mode render a visible message instead
  of a blank chart, and gets a red test the moment a genuine paint failure
  occurs instead of a swallowed error.
- **By how much**: swallowed real-browser paint failures move from "always
  silently eaten" (`EChart.tsx:91-97`) to "surfaced and caught by the
  zero-uncaught-error invariant"; real-browser tests asserting failures are
  visible move from 0 to at least 2 (parse error, transport error).
- **Measured by**: the slice-03 parse-error and transport-error tests
  asserting the visible banner + the zero-uncaught-error invariant; a
  forced-paint-failure case proving the swallow no longer masks it.
- **Baseline**: parse / transport errors render banners today, but paint
  errors are swallowed (`EChart.tsx:91-97`) and no real-browser test
  asserts any of it.

### Technical Notes

- Depends on US-PE-01 (the paint signal) and on D3 (the swallow
  remediation keeping the jsdom skip narrow).
- Part of slice 2; un-MARKed together with US-PE-02 in the slice-03 spec.
- The zero-uncaught-error invariant is already scaffolded in the spec
  (`slice-03-*.spec.ts:33-49`); the bodies need un-throwing and the swallow
  needs narrowing.
- No change to the operator-visible banner copy (C1); this story asserts
  the existing honest banners and removes the real-browser swallow, it does
  not redesign the error UI.
</content>
