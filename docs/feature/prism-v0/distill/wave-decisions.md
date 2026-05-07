# Prism v0 — DISTILL wave decisions

- **Wave**: DISTILL
- **Author**: `@nw-acceptance-designer` (Scholar, dispatched by Bea)
- **Date**: 2026-05-08
- **Inputs**: 13 DISCUSS files, 7 DESIGN ADRs (0026–0032), `component-design.md`,
  `environments.yaml`, `ci-cd-pipeline.md`, `kpi-instrumentation.md`.
- **Companion**: `test-mapping.md`, `test-strategy.md`, the six slice
  Vitest files in `apps/prism/tests/`, the six slice Playwright files
  in `apps/prism/e2e/`, the three cross-cutting invariant files, the
  four KPI fixtures.

---

## D1 — Walking skeleton strategy: Strategy C "real local"

**Decision**: Slice 01's walking skeleton uses a real Prometheus
container — not an in-memory double, not a JSON fixture — for the
Playwright E2E and the contract-shape Vitest test. Vitest unit tests
that exercise pure functions (`buildOption`, `decode`/`encode`,
`reduce`) inject a `fetchFn` mock at the architectural seam.

**Why**: the project already runs Strategy C for Aperture; reusing the
posture keeps the CI vocabulary stable, and ADR-0027's contract-test
recommendation explicitly routes to a container fixture. The walking
skeleton's lie surface is "Prometheus' actual JSON wire shape" — a
JSON-fixture-only skeleton would not catch a wire-shape regression
(the `transport-error: shape` arm of `QueryOutcome`). The container
fixture catches it before it reaches a Mac-using operator.

**Consequence**: Slice 01's Playwright spec depends on
`playwright.config.ts > globalSetup` having started a real
`prom/prometheus@<digest>` container with the seeded `up`,
`prism_test_high_cardinality`, and `prism_test_nan_bearing` metrics
described in `environments.yaml`. The Vitest contract test in Gate 11
uses the same container started as a `services:` block.

**Reviewer hook**: dimension 9 (walking skeleton boundary proof) —
the WS strategy is declared here; the implementation in
`apps/prism/e2e/slice-01-walking-skeleton.spec.ts` matches the
declaration; every driven adapter in `lib/promql`, `lib/config`,
`lib/echarts` has at least one real-I/O integration test.

---

## D2 — Mocking discipline: mock at the architectural seam, never deeper

**Decision**: tests mock the `fetchFn` parameter of `queryRange` and
the `Scheduler` parameter of the auto-refresh effect runner. Tests
NEVER mock React, NEVER mock the ECharts library, NEVER mock
`history.replaceState`, NEVER mock `URLSearchParams`.

**Why**: the seams are explicit injection points the architecture
locked in (ADR-0027 § 7, ADR-0029 § 7). Mocking deeper (e.g.
`vi.mock('react')`) would test the test, not the SPA, and would not
catch the kind of integration bugs that DISTILL exists to surface.

**Consequence**: every Vitest unit test that exercises QueryPanel
mounts the real React component tree under `@testing-library/react`'s
JSdom, calls `queryRange` (or its mocked seam) for real, and asserts
on observable DOM. The ECharts wrapper is exercised against the real
ECharts library in headless JSdom (ECharts works in JSdom via
`CanvasRenderer` plus `node-canvas`; if `node-canvas` is unavailable
in CI, the test that asserts canvas pixels migrates to Playwright).

**Reviewer hook**: dimension 7 (observable behaviour) — every Then
step asserts a return value from `queryRange`, an observable DOM
node, or a captured fetch URL. Zero `mock.calls` assertions.

---

## D3 — Test pyramid ratios: ~70% Vitest unit, ~20% Vitest integration, ~10% Playwright E2E

**Decision**: across the six slices and the three cross-cutting
invariants, target the test-pyramid ratios above by test count.

**Why**: pure functions (codec, reducer, option builder) carry the
load-bearing invariants (KPI 3, KPI 4 property, fidelity); they
test fastest and pin the most behaviour per second. Integration
tests at the QueryPanel ↔ adapter seam catch wiring bugs the pure
tests cannot see. E2E catches what only a real browser can see
(URL roundtrip across reload, KPI 1 latency, focus management).

**Consequence**: a feature that needed E2E framing for a pure-function
concern (e.g. KPI 4 byte-equality on `decode(encode(state))`) is split:
the pure-function property test goes to Vitest; the cross-tab
byte-equality on rendered series JSON goes to Playwright. KPI 4 thus
appears in BOTH layers; the layer split is locked in `test-strategy.md`.

**Reviewer hook**: dimension 6 (priority validation) — every layer
shift is justified by what only that layer can prove.

---

## D4 — KPI 3 (data fidelity) lives in Vitest, not Playwright

**Decision**: KPI 3's byte-equality test against `buildOption` runs
under Vitest with a hand-crafted five-point NaN-bearing fixture. No
Playwright dependency, no real Prometheus.

**Why**: KPI 3 is a **structural** invariant on a pure function. The
fixture is `[[t1, 1], [t2, NaN], [t3, 3], [t4, NaN], [t5, 5]]`; the
assertion is `option.series[0].data` byte-equal to that array, plus
`smooth === false`, `connectNulls === false`, `sampling === undefined`.
A Playwright framing would force the assertion through ECharts' canvas,
bringing JSdom or browser noise into a test that should be deterministic.

**Consequence**: `apps/prism/tests/invariant-fidelity.test.ts` is the
load-bearing KPI 3 test. The slice-01 Playwright spec covers KPI 1 / 2
(latency) but does NOT carry KPI 3.

**Reviewer hook**: dimension 4 (coverage completeness) — KPI 3 is
explicitly traced to `invariant-fidelity.test.ts`.

---

## D5 — KPI 4 (URL roundtrip) appears in two layers

**Decision**: KPI 4 has TWO test homes:

- `apps/prism/tests/slice-01-walking-skeleton.test.ts` (Vitest):
  property test `decode(encode(state)) === state` for every canonical
  `UrlState`.
- `apps/prism/e2e/slice-02-time-range-and-relative-presets.spec.ts` and
  `apps/prism/e2e/slice-05-absolute-time-range-and-permalink.spec.ts`
  (Playwright): cross-tab byte-equality on rendered series JSON.

**Why**: the property test pins the codec (no information lost between
encode and decode). The cross-tab Playwright test pins the full
roundtrip (URL is read on fresh load, the same Prometheus is queried,
the same chart renders). Both are load-bearing; neither alone is
sufficient.

**Consequence**: a regression in the codec breaks the Vitest property.
A regression in the QueryPanel's "read URL on mount" path breaks the
Playwright. The two together pin the contract.

**Reviewer hook**: dimension 4 — KPI 4 is covered in both layers.

---

## D6 — Stryker mutation evidence — slice-by-slice baseline failure pattern

**Decision**: at the boundary of slice N's commit, slice N+1's RED
tests are present in the workspace and fail Stryker's baseline run.
This is the same pattern Aperture, Spark, Sieve, and Codex hit with
cargo-mutants. The pattern resolves at the final slice (Slice 06)
landing.

**Why**: DISTILL writes ALL slice tests now; DELIVER turns RED into
GREEN one slice at a time. Between Slice 01 and Slice 02 lands,
`slice-02-relative-presets.test.ts` references functions that
`slice-01` did not implement, so it throws `'UNIMPLEMENTED — Slice 02
DELIVER'` from the test body. Stryker treats every RED test as a
non-killing test (the test body throws before reaching the assertion);
mutants in slice-02 territory therefore survive at slice-01-landing
time. The 100% kill-rate gate (ADR-0005 Gate 5) is therefore set to
`report-only` on Prism for v0; the gate flips to `enforce` at Slice 06
when every test has GREEN status.

**Consequence**: `wave-decisions.md > D6` is the audit trail. The
Stryker `--in-diff` posture (Gate 10) means each slice's own diff is
the surface that must hit 100% kill rate before merging that slice; a
neighbouring slice's RED tests are out-of-diff and do not fail the
in-diff gate. A FULL Stryker run would fail until Slice 06; the
in-diff posture matches the cargo-mutants in-diff posture and avoids
that.

**Reviewer hook**: dimension 6 — this matches the project's existing
mutation testing strategy and is not novel.

---

## D7 — `'UNIMPLEMENTED — Slice NN DELIVER'` is the RED idiom

**Decision**: at DISTILL, every test body that exercises a not-yet-
implemented module throws:

```ts
throw new Error('UNIMPLEMENTED — Slice 03 DELIVER');
```

The string is greppable, slice-numbered, and human-readable. Tests are
otherwise complete (Given setup, When call, Then assertion all
written); only the production module body is missing.

**Why**: this is the TS analogue of Rust's `unimplemented!()`. Vitest
treats a thrown error as a test failure with a useful message; Bea
or the crafter can `grep -n 'UNIMPLEMENTED — Slice'` to find the next
piece of work.

**Consequence**: the DISTILL hand-off is a workspace where:

- Every test file compiles (TS strict mode satisfied via the stub
  exports the crafter writes at first DELIVER step — see D8).
- Every test file's first scenario throws with a slice-numbered
  marker.
- Subsequent scenarios in the same file throw on the same marker,
  via Vitest's `it.skip` (so only one assertion fails per slice at a
  time, matching the one-at-a-time methodology).

---

## D8 — Stub-export contract for the first DELIVER step

**Decision**: at the first DELIVER step (Slice 01), the crafter writes
a tiny stub-export file for every module the tests import, so that the
RED tests COMPILE but FAIL. The stubs are themselves throwing functions:

```ts
// apps/prism/src/lib/promql/client.ts (Slice 01 stub)
export async function queryRange(/* … */): Promise<QueryOutcome> {
  throw new Error('UNIMPLEMENTED — Slice 01 DELIVER');
}
```

**Why**: TypeScript's strict mode rejects imports of non-existent
exports at type-check time. The stub file gives the test the type
surface to import while keeping the body unimplemented. The crafter
deletes each stub when the real implementation lands.

**Consequence**: DISTILL produces test files that REFERENCE module
exports (the public surface). The crafter's first commit at DELIVER
adds the stub files; subsequent commits replace stubs with real code,
slice-by-slice.

**Reviewer hook**: dimension 8 (traceability) — the stub-export
contract bridges DISTILL's "tests fail meaningfully" to DELIVER's
"first commit makes the workspace compile-clean".

---

## D9 — Property tests use Vitest's native `expect.soft` plus a hand-rolled
generator

**Decision**: property tests do NOT depend on a third-party generator
library (no `fast-check`, no `jsverify`). The two property tests in
the suite (`decode(encode(state)) === state` and "every tick is a
fresh fetch") use a hand-rolled `forEach` over a small but exhaustive
`UrlState` enumeration plus a small randomised companion.

**Why**: bundle-size discipline (300 KB gzipped) extends to dev
dependencies indirectly via the Vitest test bundle; the workspace
keeps the dev-deps tight (ADR-0031 § 4). `fast-check` is excellent for
pure-function generators but the `UrlState` shape is small enough to
enumerate by hand: 5 relative offsets × 5 refresh intervals × {q="",
q="up"} × {refresh applies, refresh ignored on absolute} = under 50
canonical states. The hand-rolled enumeration is more legible and
mutation-test-friendlier (every state is a named line, not an opaque
generator output).

**Consequence**: `slice-01-walking-skeleton.test.ts` has a
`canonicalUrlStates` array; the property test loops over it. If a
v0.x slice grows the URL vocabulary, the array grows; if it grows
beyond what enumeration handles, swap to `fast-check`.

---

## D10 — KPI 5 (page-stays-usable) split across Slice 03 and Slice 06

**Decision**: KPI 5's four primary failure modes (parse error, empty
result, transport network failure, transport HTTP 500) live in
`apps/prism/e2e/slice-03-error-and-empty-states.spec.ts`. The
malformed-URL recovery and accessibility-keyboard-recoverability tests
live in `apps/prism/e2e/slice-06-accessibility.spec.ts`.

**Why**: `kpi-instrumentation.md > 6.3` lists six failure modes. Four
are pure error-rendering (Slice 03 owns); one is malformed URL (Slice
03 owns the URL banner; Slice 06 owns "keyboard user can recover"); one
is `/config.json` unreachable (Slice 03 owns the composition-root
calm error UI).

**Consequence**: dimension 4 — KPI 5 has scenarios in both slice-03
and slice-06. The split tracks `kpi-instrumentation.md > 6.5` (slices
03 and 06 light up KPI 5).

---

## D11 — Tests in business language for the operator's role

**Decision**: every test description and every Given / When / Then
clause is written in the first person of the operator (Priya) per the
JTBD-aligned house style. No "the system shall", no
"$function returns $value". Test names read like incident-time
operator narratives.

**Why**: Mandate 2 (business language abstraction) — the test is
documentation; it must read to a non-technical stakeholder. The
DISCUSS user-stories' Elevator Pitches use Priya's voice already;
DISTILL inherits the voice.

**Consequence**: `it('renders a chart when I type "up" and press
Run')` not `it('queryRange returns success and ChartArea mounts')`.

**Reviewer hook**: dimension 3 (business language purity) — grep
should find "operator", "Priya", "I", "my", "see", "type" in test
names. The grep should NOT find "fetch", "mock", "stub", "spy",
"renders 200", "DOM node", "ref", "useEffect" in test titles.

---

## D12 — AGPL header on every TypeScript test file

**Decision**: every `.ts` and `.tsx` file under `apps/prism/tests/`
and `apps/prism/e2e/` opens with the 13-line AGPL header per
ADR-0032 § 2 (file scope: tests are first-class source).

**Why**: `eslint-plugin-license-header` enforces this on every file
in scope; Gate 9 fails if a file is missing the header. The DISTILL
output is the first wave to write tests in TS, so it sets the
discipline.

**Consequence**: every file Scholar writes opens with the header.
Future file additions during DELIVER auto-fix via `pnpm lint --fix`.

---

## D13 — Back-propagation to DESIGN: none

**Decision**: no design-level questions surfaced during DISTILL.

**Why**: ADRs 0026-0032 and `component-design.md` cover every type
the tests need to import. The two seams (`fetchFn` and `Scheduler`)
are explicit. The `QueryOutcome` discriminated union covers every
rendering arm. The `UrlState` codec is fully typed.

**Consequence**: DISTILL hands off cleanly to DELIVER without a
DESIGN-revisit cycle.

---

## D14 — Cross-engine matrix asserts via Playwright projects, not duplicated specs

**Decision**: every Playwright spec is written ONCE; the engine matrix
runs via `playwright.config.ts > projects: [chromium, firefox,
webkit]`. Tests do not duplicate by engine.

**Why**: `environments.yaml > runtime-matrix` configures three engines
on one runner with `workers: 3`; Playwright's project mechanism is the
intended path. Duplicating specs by engine would multiply maintenance
without adding signal.

**Consequence**: a single spec file produces three test runs in CI;
`playwright-report/` tags each by engine.

---

## D15 — KPI 3 fidelity-anchor fixture is hand-authored; not regenerated

**Decision**: `apps/prism/tests/fixtures/promql-fidelity-anchor.json`
is the single hand-authored fixture for the KPI 3 byte-equality test.
It is NOT regenerated from a real Prometheus.

**Why**: KPI 3 asserts that `buildOption` produces byte-identical
output for a SPECIFIC input shape (NaNs at positions 2 and 4, boundary
values at 0 and 5). Regenerating from real Prometheus would replace
the controlled NaN distribution with whatever Prometheus happened to
return, blunting the test's discriminator. The other three fixtures
(`promql-success.json`, `promql-parse-error.json`, `promql-empty.json`)
ARE periodically regenerable (operator-side ritual; not gated).

**Consequence**: `promql-fidelity-anchor.json` carries a comment
inside referencing this decision so a future contributor does not
"refresh" it.

---

## D16 — Pre-flight smoke test in `invariant-public-api.test.ts`

**Decision**: `apps/prism/tests/invariant-public-api.test.ts` is a
compile-time smoke test that imports the public TS types from each
`lib/` module. No assertions on behaviour; the test exists to fail
the suite when a public type is renamed or deleted.

**Why**: Codex's `invariant_public_api_smoke.rs` is the model. The TS
analogue is a file that imports `QueryOutcome`, `UrlState`,
`AutoRefreshState`, `RuntimeConfig`, etc., and then has a single
`it('the public API surface is intact')` that does nothing. If the
file does not COMPILE, CI fails at type-check; if it compiles, the
test passes.

**Consequence**: `pnpm typecheck` is the gate; the test body is a
no-op assertion. This is the same pattern the Rust workspace uses for
public API smoke tests.

**Reviewer hook**: dimension 8 (traceability) — every public surface
is referenced.

---

## D17 — `invariant-licence-headers.test.ts` is belt-and-braces

**Decision**: a runtime Vitest test scans every `.ts` / `.tsx` file
under `apps/prism/src/` and asserts the AGPL header is present.
This is REDUNDANT with Gate 9's `eslint-plugin-license-header`; the
redundancy is intentional.

**Why**: Gate 9 is feedback, not a required-status-check (project
posture). If a contributor disables the ESLint rule with a
`// eslint-disable-next-line` comment, the runtime test catches it.
Two tripwires for one invariant; cheap belt-and-braces.

**Consequence**: the test reads files via `fs.promises.readdir` and
runs in Node mode (not JSdom). It is fast (~50 ms for the v0 codebase).

---

## Decision summary table

| ID | Topic | Effect on test design |
|---|---|---|
| D1 | Strategy C "real local" | Slice 01 walking skeleton uses real Prometheus container |
| D2 | Mock at the seam, never deeper | `fetchFn`, `Scheduler` only |
| D3 | Pyramid 70/20/10 | Bulk Vitest unit; sparing Playwright |
| D4 | KPI 3 in Vitest, not Playwright | `invariant-fidelity.test.ts` |
| D5 | KPI 4 in two layers | Codec property + cross-tab byte-equality |
| D6 | Stryker baseline cascade | `--in-diff` posture; matches cargo-mutants |
| D7 | `'UNIMPLEMENTED — Slice NN DELIVER'` | RED idiom |
| D8 | Stub-export contract | First DELIVER commit makes the workspace compile |
| D9 | Hand-rolled property tests | No `fast-check` dep at v0 |
| D10 | KPI 5 split across Slice 03 + 06 | Tracks `kpi-instrumentation.md` |
| D11 | Tests in operator's voice | Business language Mandate |
| D12 | AGPL header on test files | ADR-0032 § 2 file scope |
| D13 | No back-propagation to DESIGN | Clean DISTILL hand-off |
| D14 | Engine matrix via Playwright projects | One spec, three engines |
| D15 | Fidelity-anchor fixture is hand-authored | Controlled NaN distribution |
| D16 | Public-API smoke compiles, asserts nothing | Codex pattern |
| D17 | Licence-header test is belt-and-braces | Catches ESLint disablement |

---

## Completion note — Bea finalisation (2026-05-08)

Scholar (`@nw-acceptance-designer`) was dispatched at 18:39 UTC and
produced through these artefacts before being interrupted by Andrea:

- The three markdown specs (`test-mapping.md`, `test-strategy.md`,
  `wave-decisions.md`) — all locked
- Slice 01-04 Vitest test files (`tests/slice-01..04-*.test.ts`)
- Slice 01-03 Playwright spec files (`e2e/slice-01..03-*.spec.ts`)
- All four JSON fixtures (`tests/fixtures/promql-*.json`)

Andrea interrupted the dispatch at the next agent-step boundary
because the periodic-check signal was queueing up — the same stuck-
process pattern flagged on the Sieve cycle. Bea finalised the
remaining seven files directly, mirroring Scholar's style:

- `apps/prism/tests/slice-05-absolute-time-range-and-permalink.test.ts`
- `apps/prism/e2e/slice-04-auto-refresh.spec.ts`
- `apps/prism/e2e/slice-05-absolute-time-range-and-permalink.spec.ts`
- `apps/prism/e2e/slice-06-accessibility.spec.ts`
- `apps/prism/tests/invariant-public-api.test.ts`
- `apps/prism/tests/invariant-licence-headers.test.ts`
- `apps/prism/tests/invariant-fidelity.test.ts`

This is the seventh occurrence of the agent-stall recovery pattern
in the project (Morgan twice on Codex, Scholar twice on
Codex/Spark, Luna once on Prism DISCUSS, Scholar once here, plus
two clean runs by Morgan and Apex on Prism DESIGN/DEVOPS). The
methodology absorbs partial-output stalls without re-doing complete
work; the reviewer (`@nw-acceptance-designer-reviewer`) treats
Scholar's halves and Bea's halves equivalently per the established
recovery posture.

Bea's halves follow Scholar's conventions verbatim: AGPL header,
operator-voice persona narrative, story/KPI/ADR map, imports from
modules ADR-0026 names, `'UNIMPLEMENTED — Slice NN DELIVER'`
throw idiom, Given/When/Then comments, no `expect.fail` calls
(`throw` is the canonical RED-state in TypeScript per D7).
