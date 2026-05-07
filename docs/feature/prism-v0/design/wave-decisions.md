# Prism v0 — DESIGN wave decisions

- **Date**: 2026-05-07
- **Architect**: `@nw-solution-architect` (Morgan, dispatched by Bea)
- **Wave**: DESIGN
- **Inputs**: 13 DISCUSS files (`docs/feature/prism-v0/discuss/` + 6
  slice briefs); SSOT (`docs/product/journeys/incident-response.yaml`,
  `docs/product/journeys/incident-response-visual.md`,
  `docs/product/jobs.yaml`); Eclipse's iteration-1 DISCUSS approval; Bea's
  pre-locked stack decisions (14 items).
- **Outputs**: this file; seven ADRs (ADR-0026 through ADR-0032);
  `component-design.md`; `workspace-layout.md`; four Mermaid
  architecture diagrams embedded across the design docs.

---

## Mode

**Propose**. The DISCUSS wave locked the contract surface (7 stories,
30 ACs, 6 slices, 5 KPIs). Bea pre-locked 14 stack decisions covering
everything from the package manager to the licence headers. Morgan's
DESIGN scope was therefore the substantive component design (module
split, error types, URL codec, auto-refresh state machine, ECharts
shape, workspace layout, licence enforcement) — each landing in an ADR
with two-or-more considered alternatives.

No further user dialogue is required to lock the architecture; if any
DISCUSS contract needs revision, a back-propagation note routes back
to Bea.

---

## Multi-architect context

This is a single-architect feature (Morgan). Prism is the project's
first **frontend** feature; no prior application-architecture
sections exist for it.

Prior ADRs (ADR-0001 through ADR-0025) cover Rust crates only. The
seven new ADRs (0026-0032) are the first set scoped to a TypeScript
SPA. They follow the same Nygard template (Status / Date / Author /
Context / Decision / Alternatives / Consequences / Verification) as
the Rust ADRs for cross-feature consistency.

The ADR cluster does not amend any existing ADR.

---

## DESIGN decisions, summarised

The DISCUSS wave locked the contract; DESIGN locks the shape.

| ID | DISCUSS-flagged decision | DESIGN resolution | ADR |
|----|--------------------------|-------------------|-----|
| D1 | Component layout / module split | Eight-folder layout under `apps/prism/src/`: `app/`, `panels/query/`, `lib/{promql,url-state,auto-refresh,config,echarts}/`, `components/`. Driving / driven / pure dependency direction. Slice tests live in `tests/slice-NN-*.test.ts` and `e2e/slice-NN-*.spec.ts`. | ADR-0026 |
| D2 | Backend HTTP client + error mapping | One total function `queryRange(req, ctx) => Promise<QueryOutcome>` returning a five-arm discriminated union (success / empty / parse-error / transport-error / aborted). CORS posture: same-origin reverse-proxy in production, Vite proxy in dev. Header redaction is structural (test asserts no header value appears in any error string). External-integration handoff annotation for contract testing of Prometheus `/api/v1/query_range`. | ADR-0027 |
| D3 | URL state schema + codec | Pure-function codec: `decode(URLSearchParams) => Result<UrlState, UrlParseError>` and `encode(UrlState) => URLSearchParams`. Vocabulary: `q`, `from`, `to`, `refresh`. Forgiving-input strict-output. `history.replaceState` (never `pushState`). Malformed URL renders calm banner; never blanks the page. | ADR-0028 |
| D4 | Auto-refresh state machine | Pure reducer over states `idle | running | backoff(0..2) | hidden`. Five-event vocabulary; effects discriminated union for the React `useEffect` runner. AbortController integration; Page Visibility integration; absolute-range disable enforced at picker AND at reducer (double lock). Backoff curve 5/10/20/30 s capped, transport-error only. | ADR-0029 |
| D5 | ECharts integration shape | Direct import; tree-shaken modular API (LineChart, GridComponent, Tooltip, Legend, Aria, Title, Canvas). Pure option builder (`buildOption(outcome, ctx) => EChartsOption`) plus thin `<EChart>` wrapper holding instance via `useRef`. `setOption({notMerge:true})`. Palette swap via CSS custom properties (Okabe-Ito + Tableau 10). `prefers-reduced-motion` honoured. SR-only `<table>` fallback for screen readers. | ADR-0030 |
| D6 | Workspace layout and tooling | Two coexistent workspaces in one git repo: Cargo workspace (existing) + pnpm workspace (NEW: `apps/*`). Top-level `package.json` with `pnpm -r` recursive scripts. Exact-version pinning across both ecosystems (matches Codex/Spark `=0.27` style). ESLint with type-checked profile + `eslint-plugin-boundaries` (module-boundary enforcement, principle 11) + `eslint-plugin-license-header`. Pre-commit hook extended with TS gates conditionally on `apps/prism/package.json` existing. CI gains parallel `prism` job. | ADR-0031 |
| D7 | Licence headers in TS source | AGPL-3.0-or-later file-level header on every `.ts`/`.tsx` source file under `apps/prism/src/`, `tests/`, `e2e/`, plus configuration files (`vite.config.ts`, `eslint.config.ts`). Single source of truth at `scripts/licence-header-agpl.txt`. ESLint plugin enforces; auto-fix prepends. Mirrors the Rust workspace's per-file licence discipline. | ADR-0032 |

---

## Architectural style

**Single modular SPA** (one bundle, one URL, one chart, one panel) with
**ports-and-adapters** internal structure:

- **Driving**: `panels/query/QueryPanel` (the only operator-facing surface
  at v0).
- **Driven adapters**: `lib/promql/` (HTTP), `lib/config/` (HTTP, single
  call), `lib/echarts/EChart` (rendering library wrapper).
- **Ports / pure**: `lib/url-state/` (codec), `lib/echarts/buildOption`
  (option builder), `lib/auto-refresh/reducer` (state machine).

**Why this shape, not microservices / not SSR / not micro-frontends**:

- **Microservices**: rejected. Prism v0 is a single panel served by
  one operator-deployed reverse proxy. A microservice split has no
  team boundary to respect (one operator, one Andrea, one designer);
  Conway's law prefers a monolith bundle.
- **SSR (Next.js, Remix)**: rejected. The operator runs Prism behind
  their reverse proxy; SSR adds a Node runtime to the operator's
  deployment surface for no incident-time benefit. The KPI 1 budget
  (first-chart < 2 s p95) is met by client-side rendering with
  tree-shaken ECharts.
- **Micro-frontends**: rejected. The future Loom v0 / Aegis frontend
  may share components with Prism, but the lift to a `packages/ui/`
  shared package is a v1+ refactor, not a v0 architectural primitive.

**Enforcement**: TypeScript's module system enforces the `pub` boundary
(unexported symbols cannot leak); `eslint-plugin-boundaries` enforces
the `panels/` → `lib/` → `lib/` (siblings only) → `components/`
direction (principle 11, language-appropriate to TS); a Vitest test
asserts `lib/url-state/` has zero React imports.

---

## Earned-Trust posture

Prism v0 has three driven adapters in the principle-12 sense:

1. **`lib/promql/` — backend HTTP client**.
   - Lie surface: "the backend honours the `/api/v1/query_range` API
     exactly as we expect."
   - Probe shape: Slice 01's walking-skeleton demo against a real local
     Prometheus is the runtime probe. No Slice 01 acceptance passes
     without a real round-trip succeeding. The CI extension is the
     contract-test recommendation in ADR-0027 § External-integration
     handoff.
   - Wire then probe then use: Slice 01's `panels/query/QueryPanel`
     does NOT issue a fetch until `lib/config/` has produced a typed
     `RuntimeConfig`; the Promise chain in `main.tsx` is the structural
     enforcement.

2. **`lib/config/` — `/config.json` fetcher**.
   - Lie surface: "the operator-deployed `/config.json` has the keys
     and shapes we expect."
   - Probe shape: the loader's parse function rejects any config that
     fails the schema; the composition root refuses to mount the SPA on
     `ConfigError` and renders a calm error UI instead. This is the
     "wire then probe then use" invariant — without a healthy config,
     the SPA never tries to fetch the backend.

3. **`lib/echarts/EChart` — ECharts wrapper**.
   - Lie surface: "ECharts honours `setOption({notMerge:true})` without
     merging stale series", and "ECharts' `AriaComponent` produces the
     announced text we expect for screen readers."
   - Probe shape: Slice 04's Playwright test asserts DOM-node identity
     preservation across ticks (no flicker = no re-mount). KPI 3's
     Vitest test asserts byte-equality for `buildOption` against the
     five-point fixture. Slice 06's a11y audit (Lighthouse + axe-core)
     is the screen-reader probe.

The principle-12 self-application — "the probe must verify that probes
exist" — is covered by:

- ESLint rule failing if a new `lib/<adapter>/` is added without a
  typed total-function return type (no throws). The `boundaries` plugin
  catches imports of `throw` from `lib/`; the type-checked profile catches
  unhandled exceptions.
- A Vitest meta-test asserting `queryRange` is total (catches every
  `fetch` rejection) — the test mocks `fetch` to throw and asserts
  the result is a `QueryOutcome` value, never a thrown exception.

The composition root invariant ("wire then probe then use") is
honoured: `main.tsx` loads `/config.json` first, refuses to mount
`<App>` on config error, and only then issues backend fetches.

---

## Quality attributes (ISO 25010)

The strategy table is in `component-design.md` § 10. Highlights:

- **KPI 1** (first-chart < 2s p95): tree-shaken ECharts, no
  state-management library, native fetch (no `axios`/`ky`); pure
  option builder means the chart-render path is one synchronous call.
- **KPI 2** (iterate < 800 ms p95): `setOption({notMerge:true})` without
  re-mount; no chart instance churn.
- **KPI 3** (data fidelity 100%): the option builder's invariants
  (`smooth: false`, `connectNulls: false`, no auto-downsampling) are
  pure-function tested; mutation testing covers each invariant.
- **KPI 4** (URL roundtrip 100%): the codec's `encode(decode(URL)) ===
  URL` property test plus Playwright's byte-equality on rendered
  series.
- **KPI 5** (page-stays-usable 100%): total-function `QueryOutcome`
  shape (no throws); React error boundary at `<App>` root for
  catastrophic JSX errors; calm-banner fallback on malformed URL.

---

## Existing-system reuse analysis

Prism v0 is the first frontend feature; the codebase has no prior TS
to reuse. Examined for transferable patterns:

- **Rust crate ADR style** (Nygard template): adopted directly for
  ADRs 0026-0032. Cross-feature consistency for reviewers.
- **Per-crate exact-pinning posture** (`=0.27` style, ADR-0024 § 3):
  adopted for npm dependencies via `save-exact=true` + manual exact
  versions in `package.json`. Same MSRV-creep posture for Node version
  bumps.
- **Pre-commit hook discipline** (Rust gates in seconds): extended
  with TS gates conditionally; the contributor experience matches the
  Rust workspace's posture.
- **Composition root + driven adapters pattern** (Aperture's
  `bin/aperture.rs`, Sieve's `lib.rs`): adopted for `main.tsx` as the
  one place that wires `lib/config/`, `lib/promql/`, `lib/echarts/`,
  and the QueryPanel.
- **Earned-Trust principle-12 application** (Aperture's TLS / SPIFFE
  probes, Spark's catalogue probe): adapted to the SPA shape — config
  load is the startup probe; Slice 01's real round-trip is the runtime
  probe; CI contract tests are the regression probe.

No existing TS code was discoverable for reuse; no integration into
existing modules was bypassed. The integration surface to existing
project tooling (pre-commit hook, CI workflow, `LICENSING.md`) is
documented in ADR-0031.

---

## Constraint and priority analysis

DISCUSS quantified the constraints:

- **Bundle gate**: 300 KB gzipped at v0. ECharts is ~200 KB. The
  remaining 100 KB has to fit React 19 + react-router-dom + the SPA
  source. **Quantified impact**: bundle composition is 67% ECharts,
  ~20% React + Router, ~13% Prism source. Bundle-size optimisation
  has its largest leverage on ECharts (lazy-import escape hatch in
  ADR-0030 § 7); Prism source optimisation has the smallest leverage.
- **First-chart latency p95 < 2 s on developer laptop**: data path
  from page load to first paint involves config fetch (~50 ms),
  router decode + UrlState (~5 ms), backend fetch (~50-200 ms typical
  for local Prom), JSON parse (~5-20 ms), buildOption (~10 ms),
  ECharts paint (~30-100 ms). **Quantified breakdown**: backend fetch
  + JSON parse is 35-50% of the budget; ECharts paint is 15-30%;
  everything else is rounding error. The constraint-free opportunity
  is the first-paint latency; the constraint-bound is the backend
  round-trip.
- **No client-side state persistence** (cookie/localStorage/IDB
  forbidden): the URL is the only state container. **Quantified
  impact**: this constrains the codec design (URL must encode all
  view state) but is not a performance constraint. It does forbid
  some operationally-tempting features (remember last query, remember
  refresh interval).

Primary focus: data fidelity (KPI 3) and page-stays-usable (KPI 5),
because both are 100% targets with zero flakiness budget. Secondary
focus: latency (KPI 1, 2) at p95 with measured tolerance. Tertiary
focus: URL roundtrip (KPI 4) which the codec design makes structurally
guaranteed.

The ADR set reflects this prioritisation: ADR-0027 (HTTP client) and
ADR-0030 (ECharts) dominate, since they own the fidelity invariant;
ADR-0028 (URL codec) is structurally simple but worth its own ADR
because it owns KPI 4; ADR-0029 (auto-refresh) is the most complex
state machine but only material at Slice 04.

---

## Slice → ADR → module mapping

In `component-design.md` § 12. Summary: Slice 01 lights up ADRs 0026,
0027, 0028, 0030 across most of the modules; Slices 02-05 deepen the
URL codec and auto-refresh; Slice 06 deepens the ECharts palette and
a11y; Slice 07 (extension) does not exist at v0.

---

## C4 diagrams

Four diagrams across the design docs:

1. **System Context (L1)**: in `component-design.md` § 1.
2. **Container (L2)**: in `component-design.md` § 2.
3. **Component (L3)**: in `component-design.md` § 3.
4. **Init sequence**: in `component-design.md` § 5.
5. **Auto-refresh state machine**: in `component-design.md` § 6 (and
   the full version in ADR-0029 § 1).
6. **Error mapping flow**: in `component-design.md` § 7.

The L1 + L2 minimum is satisfied. L3 is included because Prism's
internal-component count (8 modules + UI sub-components) crosses the
"5+ components" threshold the C4 model recommends as the trigger for
L3. The `panels/query/` module's internal component-level decomposition
is in the catalogue (`component-design.md` § 4).

---

## Open items NOT decided in DESIGN

Routed to crafter for DELIVER:

- Vite plugin set (visualiser plugin choice, dev-mode middleware shape).
- CSS custom property scoping inside `theme.module.css`.
- `useReducer` vs custom-hook shape inside `lib/auto-refresh/`.
- The exact TypeScript util shape for the `Result` type
  (one local definition vs a shared `lib/util/result.ts`).

Routed to platform-architect (Apex) for DEVOPS:

- The full CI workflow YAML (parallel `rust` + `prism` jobs).
- The bundle-size assertion script (which Vite plugin, what tolerance).
- The Playwright `globalSetup` for the local-Prometheus container
  (image version, port mapping, fixture metric population).
- The browser-matrix runner config (which engines per CI job, sharding).
- KPI 1, 2, 3 metric emission via Aperture (the SPA-emitted metric
  pipeline; lib/promql captures `queryMs` per outcome and the QueryPanel
  passes it to a metrics module the DEVOPS handoff specifies).
- Contract testing for Prometheus `/api/v1/query_range` (Pact-JS or
  container-fixture posture; ADR-0027 § External-integration handoff).

Routed to acceptance-designer (Scholar) for DISTILL:

- The Gherkin scenarios derived from the 30 ACs across the 7 stories.
- The slice-by-slice `*.feature` file layout matching the `tests/slice-
  NN-*.test.ts` and `e2e/slice-NN-*.spec.ts` conventions.
- The KPI 3 byte-equality test fixture (the five-point NaN-bearing
  series).
- The KPI 4 URL roundtrip Playwright fixture.

---

## Recovery posture

This is the sixth occurrence of the agent-stall recovery pattern in
this project (Morgan twice on Codex, Scholar twice on Codex/Spark,
Luna once on Prism DISCUSS). Morgan completed this DESIGN wave end-to-
end without stalling; the artefact set is the seven ADRs plus three
DESIGN-wave documents.

If a future agent stalls at any DELIVER slice, Bea finalises and
flags in the relevant `slice-decisions.md`; the methodology absorbs
the partial output. The ADR cluster is stable enough to support that
absorption — every architectural decision has a documented rationale
and an alternatives table.

---

## Next-wave handoffs

### To DEVOPS (`@nw-platform-architect` Apex)

Receives:

- The DESIGN ADR cluster (0026-0032).
- `component-design.md` § 10 (quality attributes table).
- `workspace-layout.md` § 5 (CI workflow extension).
- ADR-0027's External-integration handoff annotation:
  *"Prometheus / Mimir HTTP API is an external integration. Recommended:
  consumer-driven contract tests via Pact-JS or container-fixture
  posture in CI."*

Designs:

- The CI workflow YAML.
- The bundle-size script.
- The Playwright `globalSetup` for local Prometheus.
- The browser-matrix sharding.
- The KPI metric emission pipeline.
- The contract-testing posture.

### To DISTILL (`@nw-acceptance-designer` Scholar)

Receives:

- The full DISCUSS artefact set (7 stories, 30 ACs, 6 slices).
- The DESIGN ADR cluster.
- `component-design.md`'s slice-to-module mapping (§ 12).

Designs:

- Slice-by-slice Gherkin `.feature` files.
- The unit-test fixture catalogue (KPI 3, codec roundtrip).
- The E2E Playwright spec catalogue (KPI 4, KPI 5).

### To Reviewer (`@nw-solution-architect-reviewer` Atlas)

Bea dispatches Atlas to review this wave before DEVOPS and DISTILL
proceed. Per the orchestrator's parallel-handoff design, the reviewer
gate is the precondition for the downstream waves; this DESIGN wave is
ready for review.

---

## Quality gates (self-attested)

- [x] Requirements traced to components — every AC routes to a module.
- [x] Component boundaries with clear responsibilities — ADR-0026 § 2.
- [x] Technology choices in ADRs with alternatives — every ADR has a
      "Alternatives considered" section with two-or-more options.
- [x] Quality attributes addressed — `component-design.md` § 10.
- [x] Dependency-inversion compliance — driving / driven / pure split
      enforced by `eslint-plugin-boundaries`.
- [x] C4 diagrams (L1 + L2 minimum) — L1, L2, L3, plus init sequence,
      state machine, error flow.
- [x] Integration patterns specified — ADR-0027 (HTTP client), ADR-0028
      (URL codec), ADR-0029 (auto-refresh effects).
- [x] OSS preference validated — every dependency is OSS; AGPL Prism
      source per project policy.
- [x] AC behavioural, not implementation-coupled — DISTILL inherits
      the 30 ACs unmodified; ADRs do not constrain DISTILL's scenario
      shapes.
- [x] External integrations annotated for contract testing — ADR-0027
      § External-integration handoff.
- [x] Architectural enforcement tooling recommended —
      `eslint-plugin-boundaries` (ADR-0031 § 7) + ESLint type-checked
      profile + `eslint-plugin-license-header`.
- [x] Earned-Trust probes specified per driven adapter — `wave-decisions.md`
      § Earned-Trust posture (above).
- [ ] Peer review completed and approved — pending Atlas dispatch.
