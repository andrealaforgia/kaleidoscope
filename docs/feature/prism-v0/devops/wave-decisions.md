# Prism v0 — DEVOPS wave decisions

- **Date**: 2026-05-08
- **Architect**: `@nw-platform-architect` (Apex, dispatched by Bea)
- **Wave**: DEVOPS
- **Inputs**: `outcome-kpis.md` (DISCUSS, parallel-handoff); ADR-0026
  through ADR-0032 (DESIGN); `component-design.md`,
  `workspace-layout.md`, DESIGN `wave-decisions.md`; existing CI
  workflow (9 Rust gates), pre-commit / pre-push hooks; Bea's
  pre-resolved decisions (deployment target, container
  orchestration, CI/CD platform, observability, deployment strategy,
  branching, mutation testing).
- **Outputs**: this file plus 7 companion files under
  `docs/feature/prism-v0/devops/`.

---

## Mode

**Propose-and-execute under pre-resolved decisions.** The orchestrator's
brief locks 8 decisions before this wave (deployment target,
container orchestration at v0, CI/CD platform, existing infra reuse,
observability technology, deployment strategy, continuous-learning
out of scope, branching). Apex executes within those constraints
plus delivers the substantive DEVOPS work: gates 6-11, KPI
instrumentation, contract-testing posture, pre-commit and CI
contracts.

The DESIGN ADR cluster (0026-0032) is stable; no back-propagation
to DESIGN was needed during this wave.

---

## Multi-architect context

Single-architect wave (Apex). The DESIGN wave's seven ADRs are the
load-bearing inputs; the DISCUSS wave's KPI document is the only
DISCUSS file routed to DEVOPS per the orchestrator's parallel-
handoff design (DESIGN gets the full DISCUSS artefact set; DEVOPS
gets only the KPI file).

This is the first DEVOPS wave for a TypeScript SPA in Kaleidoscope.
Prior DEVOPS waves covered the Rust crates (harness, aperture,
spark, sieve, codex). The patterns transfer cleanly:

- Per-feature mutation testing (ADR-0005 Gate 5) → Gate 10
  (StrykerJS mirroring cargo-mutants).
- Real-local fixture pattern (Aperture's Strategy C) → Gate 7
  Playwright + Gate 11 contract test against `prom/prometheus`
  container.
- Pre-commit gate discipline → extension of `scripts/hooks/pre-commit`
  with TS gates.
- CI-is-feedback-not-a-gate posture → no required-status-checks for
  Prism gates (mirrors the Rust posture).

---

## DEVOPS decisions, summarised

| ID | Decision | Rationale | KPI traceability | ADR / file |
|---|---|---|---|---|
| **D1** | **Deployment shape: static SPA behind operator's reverse proxy.** No Docker image, no Kubernetes manifest, no Helm chart at v0. | Pre-resolved by orchestrator; mirrors Grafana / Prometheus UI posture. Smallest operator surprise. | n/a (deployment shape doesn't affect KPIs directly) | `platform-architecture.md` § 1 |
| **D2** | **Dev-mode posture: Vite dev server + local Prometheus container, with Vite `server.proxy` forwarding `/api/v1/*`.** Mirrors the production same-origin posture (ADR-0027 § 5). | Same shape dev-and-production keeps the SPA's `lib/promql` code path identical. | KPI 1 / 2 fixture runs in this shape | `platform-architecture.md` § 2 |
| **D3** | **CI gate set: 6 new Prism gates (6-11) added as parallel jobs alongside 9 existing Rust gates.** Gate 9 first (lint, cheapest); Gate 6 (Vitest) prerequisite for Gates 7, 8, 10, 11. | Mirror of the Rust gate fail-fast ordering (Gate 4 deny first). Cheap gates fail fast; expensive gates run last. | Each gate maps to one or more KPIs; see `kpi-instrumentation.md` § 10 | `ci-cd-pipeline.md` §§ 3.1-3.6 |
| **D4** | **Browser matrix: Playwright runs Chromium + Firefox + WebKit per CI run.** No sharding at v0; three engines on one runner with `workers: 3`. | DISCUSS cross-KPI guardrail. WebKit covers Safari for cross-browser purposes. | All KPIs sensitive to browser variance (1, 2, 4, 5) | `environments.yaml > runtime-matrix` |
| **D5** | **Browser-emitted KPI metrics path: same-origin POST to `/v1/metrics` (Path B).** Path A (`console.warn` only) is the dev-mode complement, not a substitute. Path C (cross-origin to Aperture) is rejected. | Same-origin posture matches ADR-0027 § 5. Reuses operator's reverse proxy for routing. No CORS preflight overhead on operator's incident path. | KPI 1 (`prism.first_chart_latency_ms`), KPI 2 (`prism.iterate_latency_ms`) | `observability-design.md` § 3 |
| **D6** | **Custom JSON emitter (50 lines), not OTel-JS browser SDK.** Aperture translates JSON to OTLP at ingestion. Migration to OTel-JS is the v0.x graduation. | Bundle gate (300 KB gzipped, ECharts ~200 KB) has no headroom for the OTel-JS browser SDK (~30-40 KB). v0 has only two metrics with one numeric field each. | All metric-emitting KPIs (1, 2) | `observability-design.md` § 3.4 |
| **D7** | **No K8s / no Docker / no Helm at v0.** Static bundle is the artefact; operator's reverse proxy is the orchestration. | Pre-resolved by orchestrator (mirrored from DISCUSS). v0 is a v0 — Phase 2 Loom owns dashboards-as-code where K8s manifests live. | n/a | `platform-architecture.md` § 1.3 |
| **D8** | **Mutation testing: StrykerJS, `--in-diff` against origin/main, baseline cascade origin/main → HEAD~1 → full, 30-min timeout, 100% kill rate per ADR-0005 Gate 5.** | Mirror of the existing gate-5-mutants-* jobs (Aperture / Spark / Sieve / Codex). StrykerJS is the JS-ecosystem analogue of cargo-mutants. CLAUDE.md's mutation-testing-strategy section names per-feature mutation testing as the project posture; Prism v0 is per-feature. | Indirectly all KPIs; directly KPI 3 (option-builder invariant flips) | `ci-cd-pipeline.md` § 3.5 |
| **D9** | **Contract testing: container-fixture (real `prom/prometheus@<digest>` in CI), NOT Pact-JS.** | Lighter setup; matches Aperture's Strategy C pattern. Pact-JS is overkill at v0 (one consumer, one externally-maintained provider). Migration path documented if a second backend or a second consumer joins. | KPI 5 indirectly (transport-error and shape-error arms in the QueryOutcome union) | `ci-cd-pipeline.md` § 3.6 |
| **D10** | **Bundle-size gate: 300 KB gzipped main JS bundle, enforced by `gate-8-prism-bundle-size`.** Implementation: `apps/prism/scripts/check-bundle-size.js` (DESIGN routed the script to Apex; the crafter writes the body at Slice 01). | DISCUSS cross-KPI guardrail. Coupled to KPI 1 (large bundle = slower first paint). | KPI 1 (and structurally protects KPIs 1, 2 by keeping the bundle within the latency-sensitive ceiling) | `ci-cd-pipeline.md` § 3.3 |
| **D11** | **Pact-JS migration trigger conditions: a second backend (Mimir-specific shape divergence, VictoriaMetrics, Grafana Cloud) OR a second consumer of the same backend (e.g. Loom v0).** Until then, container-fixture is the lighter shape. | Records the Pact-JS deferral rationale so the v0.x decision has a clear gate condition. | n/a | `ci-cd-pipeline.md` § 3.6 |
| **D12** | **Pre-commit hook contract: TS section conditional on `apps/prism/package.json` presence; runs `pnpm --filter prism lint && format:check && typecheck && vitest`.** | Mirror of the existing Rust hook discipline (Rust contributors do not pay the TS gate cost). Fast subset locally; slow gates in CI. | All KPIs that have CI-gated tests (1, 2, 3, 4, 5) | `ci-cd-pipeline.md` § 4 |
| **D13** | **Pre-push hook: NO Prism additions at v0.** Pre-push runs nightly-toolchain-bound Rust gates (`cargo public-api`, `cargo semver-checks`); the TS ecosystem has no analogue for an SPA (no published library API surface). | Honest about ecosystem differences; revisit if `packages/ui/` emerges as a published TS library. | n/a | `ci-cd-pipeline.md` § 4.4 |
| **D14** | **CI workflow YAML: extension shape, NOT a new workflow file.** Six new jobs added to existing `.github/workflows/ci.yml` as parallel additions; existing 9 Rust jobs unchanged; existing concurrency / triggers / permissions unchanged. | Single CI ground-truth surface. One cancel-in-progress group. | All KPIs | `ci-cd-pipeline.md` § 5 |
| **D15** | **Branching: pure trunk-based, no required-status-checks, no enforce_admins.** CI runs on every push to main and every PR; PR builds run gates but do NOT block merge. Discipline: local pre-commit + pre-push + fix-forward + post-merge correction. | Project memory: Kaleidoscope is pure trunk-based, no CI gates. CI is feedback, not a gate. | n/a (branching shape doesn't affect KPIs directly) | `branching-strategy.md` |
| **D16** | **Monitoring: minimal at v0; defer dashboards to Loom Phase 2 and alerting to Aegis Phase 3.** Browser-emitted metrics (D5) populate the operator's Prometheus / Mimir; Kaleidoscope-side dashboards arrive with Loom. | No production deployment we own; Andrea is solo on-call; alerting infrastructure not yet built. | KPI 1 / 2 graduate to dashboards in Phase 2 | `monitoring-alerting.md` |
| **D17** | **KPI 1 / KPI 2 capture: both CI fixture (Gate 7 Playwright) AND browser-emitted (Aperture path).** CI is the structural enforcement; browser emission is the production observation surface. | Belt-and-braces. CI gates regression at PR time; production emission lets operators tune their workload. | KPI 1, KPI 2 | `kpi-instrumentation.md` §§ 2, 3 |
| **D18** | **KPI 3 / KPI 4 / KPI 5 capture: CI only at v0.** KPI 3 is a structural invariant (Vitest unit + Stryker mutation gate); KPI 4 / 5 are Playwright assertions. v0.x adds `prism.uncaught_error_count` for KPI 5 production observation. | 100%-or-fail invariants do not need a production dashboard at v0; CI gate is the ground truth. | KPI 3, 4, 5 | `kpi-instrumentation.md` §§ 4-6 |
| **D19** | **Bundle ceiling: 300 KB gzipped main JS.** Coupled gate to KPI 1 (large bundle = slower first paint on developer laptop). | DISCUSS cross-KPI guardrail; ECharts already takes ~200 KB so the budget pressure is on Prism source + React. | KPI 1, KPI 2 (indirectly) | `ci-cd-pipeline.md` § 3.3 |
| **D20** | **Reverse-proxy contract**: operator's existing reverse proxy serves `/` from the Prism `dist/` and forwards `/api/v1/*` and `/v1/metrics` to backends. Same-origin posture from ADR-0027 § 5 extended to the metric emission endpoint. | Reuses existing operator infrastructure; no new TLS cert; no new origin to manage. | KPI 1 / 2 emit path | `platform-architecture.md` § 1 |
| **D21** | **Aperture's `/v1/metrics` JSON ingestion path is a v0.x dependency.** v0 ships with the SPA emitting; if Aperture has not yet implemented `/v1/metrics`, the operator's reverse proxy returns `204 No Content` for the endpoint and emits silently no-op. | Decoupled rollout: Prism v0 ships even if Aperture's ingestion path lands later. | KPI 1, KPI 2 (graceful degradation to no-emit) | `observability-design.md` § 3.5 |

---

## Earned-Trust posture (mirror of DESIGN's discipline)

Per the orchestrator's brief, every load-bearing element is named at
each enforcement layer. Summary table:

| Element | Subtype | Structural | Behavioural |
|---|---|---|---|
| Same-origin posture | n/a | grep CI step asserts no `mode: 'no-cors'` / `credentials: 'include'` in `lib/promql` or emitter | Slice 01 Playwright observes no CORS preflight |
| Browser-emitted metric payload | TS type for `MetricBatch` | Vitest test asserts no header values from `lib/promql` leak into emitter payload | Playwright Slice 01 asserts POST `/v1/metrics` happens within 100 ms of first chart paint |
| Bundle ceiling | n/a | Gate 8 fails on > 300 KB gzipped | Slice 01 Playwright measures KPI 1 against the real bundle |
| Mutation kill rate | n/a | Gate 10 fails on any survivor | Surviving mutant points crafter at missing test |
| Local Prometheus fixture | n/a | digest-pinned image; pull-fail = job-fail | Gate 7 / Gate 11 talk to real container |
| Pre-commit hook | n/a | Hook fails commit on any failed gate | Local feedback within ~30-90s |
| Reverse-proxy routing | n/a | Documented in `platform-architecture.md` § 1; operator-deployed | n/a (operator-side concern) |

Three-layer compliance: every load-bearing element lands in at
least two layers; KPI-coupled gates (Gate 7, Gate 8, Gate 10) land
in all three. The composition-root invariant ("config load is the
startup probe") is honoured at the architecture level (DESIGN's
ADR-0026 § 5) and structurally enforced at the SPA's runtime
(`main.tsx` refuses to mount on `ConfigError`).

---

## Existing-system reuse analysis

Per principle 2, every reuse decision is documented with rationale.
Detailed table in `platform-architecture.md` § 5; summary:

**Reused**:

- `.github/workflows/ci.yml` — extended, not replaced.
- `scripts/hooks/pre-commit` — extended with TS section conditionally.
- Cargo / Aperture / Spark / Sieve / Codex CI pattern (mutation
  testing per crate) — adopted as Gate 10 (StrykerJS).
- Aperture's Strategy C (real-local Prometheus container) — adopted
  for Gates 7 and 11.
- `--locked` / `--frozen-lockfile` discipline — adopted for pnpm.
- Action SHA pinning — reused for new jobs.

**Genuinely new**:

- StrykerJS (no Rust analogue runs on TS).
- Playwright (no Rust analogue runs browser engines).
- Vitest (no Rust analogue runs TS unit tests).
- Browser-side custom emitter (OTel-JS too large for bundle gate).

Each new piece justified against "no existing alternative meets the
requirement".

---

## Constraint and priority analysis

Detailed table in `platform-architecture.md` § 6. Summary:

- **Primary focus** (constraint > 50% of delivery): KPI 1 / KPI 2
  latency budget (Gate 7) and bundle size (Gate 8).
- **Secondary focus** (20-50%): browser matrix (Gate 7 with 3
  engines).
- **Tertiary focus** (< 20%): header redaction (single Vitest test);
  no-CORS posture (single grep CI step).

DORA posture inherited from existing project state (Elite per
Andrea's solo cadence and the trunk-based posture). No DORA
regression expected; Prism gates run in parallel with Rust gates,
total wall-clock bounded by the slower group.

---

## Slice → Gate → KPI mapping

| Slice | Gates that fire | KPIs covered |
|---|---|---|
| 01 walking skeleton | 6, 7, 8, 9, 10, 11 (every gate that has a baseline fires) | 1, 2, 3 (option-builder), 5 (composition-root failure mode) |
| 02 relative presets | 6, 7, 9, 10 | 4 (URL roundtrip relative) |
| 03 errors and empty | 6, 7, 9, 10 | 5 (four documented failure modes) |
| 04 auto-refresh | 6, 7, 9, 10 | 3 (option-builder under tick), 5 (Page Visibility) |
| 05 absolute range | 6, 7, 9, 10 | 4 (URL roundtrip absolute) |
| 06 accessibility | 6, 7, 9, 10 | 5 (keyboard recoverability) |

Every slice's commit triggers the full gate set. Gates that touch
no Prism file short-circuit (e.g. Gate 10's diff-empty case exits
in zero seconds).

---

## Open items NOT decided in DEVOPS

Routed to crafter for DELIVER:

- The actual YAML for the six new CI jobs. Apex specifies the
  contract (`ci-cd-pipeline.md` §§ 3.1-3.6); crafter writes the
  YAML at Slice 01.
- The actual Bash for the pre-commit hook extension. Apex specifies
  the contract (`ci-cd-pipeline.md` § 4.3); crafter writes the
  Bash at Slice 01.
- The actual JS for `apps/prism/scripts/check-bundle-size.js`. Apex
  specifies the contract (`ci-cd-pipeline.md` § 3.3); crafter writes
  the body at Slice 01.
- The actual TS for `apps/prism/src/lib/observability/emitter.ts`.
  Apex specifies the contract (`observability-design.md` §§ 3.4-3.7);
  crafter writes the body at Slice 01.
- The Playwright `globalSetup` for the local Prometheus container.
  Apex specifies the digest-pinning discipline; crafter resolves
  the digest at fixture-authoring time.
- The exact StrykerJS configuration shape (`stryker.config.json`).
  Apex specifies the gate behaviour and baseline cascade; crafter
  picks the configuration file shape (typescript-checker on / off,
  reporters, etc.).

Routed forward to **Aperture team** (Andrea wearing the Aperture
hat at v0.x):

- Implement Aperture's `/v1/metrics` JSON ingestion path. Contract
  in `observability-design.md` § 3.4.

Routed forward to **Loom Phase 2**:

- Build the dashboard panels for `prism.first_chart_latency_ms`
  and `prism.iterate_latency_ms`. Annotation in
  `monitoring-alerting.md` § 7.
- Build the synthetic uptime probe against the operator's deployed
  Prism URL.

Routed forward to **Aegis Phase 3**:

- Burn-rate alerting on KPI 1 / 2. Annotation in
  `monitoring-alerting.md` § 8.

---

## Upstream-changes flag

**No back-propagation to DESIGN required.** The DESIGN ADR cluster
(0026-0032) is the load-bearing input for this wave; every DEVOPS
decision falls within the architecture DESIGN locked. Apex did not
discover a contradiction with any ADR.

**One forward-propagation flag** (not a back-propagation):

- D21 records that Aperture's `/v1/metrics` JSON ingestion path is a
  v0.x dependency. The Prism v0 SPA ships even if Aperture's
  ingestion has not yet landed; the reverse proxy can return 204
  for `/v1/metrics` and the SPA's emit attempts no-op silently.
  This is a graceful-degradation property, not a back-propagation
  to DESIGN — DESIGN's ADR-0027 already supports the no-network
  case (transport-error: network arm).

---

## Recovery posture

This is the seventh occurrence of the agent-stall recovery pattern
in this project (Morgan twice on Codex, Scholar twice on Codex /
Spark, Luna once on Prism DISCUSS, Morgan clean on Prism DESIGN).
Apex completed this DEVOPS wave without stalling; the artefact set
is the eight files under `docs/feature/prism-v0/devops/`.

If the reviewer (`@nw-platform-architect-reviewer`) flags critical
or high issues, Apex addresses them in up to 2 iterations per the
peer-review protocol; subsequent review-uncovered drift falls to
the next-wave reviewer (Crafty at DELIVER for any DEVOPS-deferred
implementation choice).

---

## Next-wave handoffs

### To Reviewer (`@nw-platform-architect-reviewer`)

Bea dispatches the reviewer after both DEVOPS and DISTILL return.
The two waves run in parallel; both must close before DELIVER can
begin.

Reviewer receives:

- The eight files under `docs/feature/prism-v0/devops/` (this
  document plus seven companions).
- The DESIGN ADR cluster (0026-0032) for cross-reference.
- DISCUSS `outcome-kpis.md` for KPI traceability validation.
- Existing CI workflow + hooks for reuse-validation.

### To DELIVER (`@nw-software-crafter`, after reviewer approves and Scholar's DISTILL closes)

Crafter receives:

- This file's D-numbered decisions as the DEVOPS contract surface.
- `ci-cd-pipeline.md` §§ 3.1-3.6 (gate YAML contracts).
- `ci-cd-pipeline.md` § 4.3 (pre-commit hook contract).
- `ci-cd-pipeline.md` § 3.3 (`check-bundle-size.js` contract).
- `observability-design.md` §§ 3.4-3.7 (emitter contract).
- `kpi-instrumentation.md` §§ 2-9 (per-KPI test contract).
- `environments.yaml` (fixture and matrix contract).

Crafter writes the actual YAML, Bash, JS, TS at the relevant
slice's DELIVER pass:

- Slice 01: extends CI workflow with all six gates; writes
  pre-commit hook extension; writes bundle-size script; writes
  emitter; writes Playwright `globalSetup`; writes StrykerJS config.
- Slice 02-06: each slice's tests populate the gate set further;
  no new gates added.

### To DISTILL (`@nw-acceptance-designer` Scholar — runs in parallel)

Scholar's DISTILL wave received only the DESIGN ADR cluster + the
DISCUSS artefact set. Apex's DEVOPS output is not a DISTILL input
(per the orchestrator's parallel-handoff design). The two
handoffs converge at DELIVER's first slice, where the crafter
implements both DISTILL's `*.feature` files and Apex's gate
contracts in the same commit set.

---

## Quality gates (self-attested)

- [x] All KPIs have an instrumentation contract — `kpi-instrumentation.md` § 10.
- [x] Each gate has trigger / duration / parallelism / artefact / retention specified — `ci-cd-pipeline.md` §§ 3.1-3.6.
- [x] Each gate names its three-layer Earned-Trust enforcement — `ci-cd-pipeline.md` §§ 3.1-3.6 + this file's § Earned-Trust posture.
- [x] Existing-infrastructure-first analysis documented — `platform-architecture.md` § 5.
- [x] Constraint impact analysis with priority — `platform-architecture.md` § 6.
- [x] Rollback / contingency posture — operator's reverse proxy hot-reloads the static bundle; revert is `git revert + pnpm build + cp dist`.
- [x] Pre-commit + CI contract specified for crafter — `ci-cd-pipeline.md` §§ 4 + 5.
- [x] Branching strategy documented — `branching-strategy.md`.
- [x] Monitoring posture documented (minimal at v0; graduation roadmap) — `monitoring-alerting.md`.
- [x] Contract-testing decision recorded with migration trigger — D9 + D11; `ci-cd-pipeline.md` § 3.6.
- [x] No back-propagation to DESIGN required — § Upstream-changes flag.
- [ ] Peer review completed and approved — pending reviewer dispatch.
