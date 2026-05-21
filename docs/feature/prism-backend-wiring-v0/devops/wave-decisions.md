# Wave Decisions: prism-backend-wiring-v0 (DEVOPS)

- **Wave**: DEVOPS (slim)
- **Architect**: `nw-platform-architect` (Apex)
- **Date**: 2026-05-21
- **DESIGN basis**: commit e297292 (`design/` artefacts, ADR-0043)
- **Branching**: pure trunk-based; main has no required-status-checks and
  no enforce_admins. CI is feedback, not a gate
  (memory: project_kaleidoscope_pure_trunk_based).

## Context

DESIGN resolved the central fork to same-origin via `tower-http` `ServeDir`
behind `KALEIDOSCOPE_QUERY_STATIC_DIR` (DD1, ADR-0043), and pinned the
committed `apps/prism/public/config.json` (DD2/DD4). The DESIGN DEVOPS
handoff (`design/wave-decisions.md` "DEVOPS handoff") already asserts that no
new CI gate is required. This wave verifies that claim against the live
`.github/workflows/ci.yml` by grep, and records the verdicts. It does NOT
modify `ci.yml`, code, or tests.

## Pre-flight grep results (ground truth)

```
$ grep -c "gate-5-mutants-query-api" .github/workflows/ci.yml
1
```

Confirms query-api is mutation-gated. The job
`gate-5-mutants-query-api` (CI line 1036) runs
`cargo mutants --package query-api --in-diff "$DIFF_FILE"` where `$DIFF_FILE`
is `git diff BASELINE HEAD -- 'crates/query-api/**'`. So any change under
`crates/query-api/**` is mutation-tested by the existing gate.

```
$ grep -niE "prism|vite|npm|pnpm|node|playwright|frontend" \
    .github/workflows/ci.yml | head
```

Confirms a FULL frontend CI surface exists (CI lines 1549-1732): Gate 6
(Prism Vitest typecheck + unit/integration), Gate 7 (Prism Playwright E2E),
Gate 8 (bundle size <=300 KB gzipped), Gate 9 (lint + format + AGPL header),
Gate 10 (StrykerJS mutation, in-diff), Gate 11 (Prism Prometheus contract,
container fixture). All run on `pnpm`/Node 22, gated on `apps/prism/` changes.

---

## A1 -- NO new Rust gate

**Decision.** No new Rust CI gate is added.

**Why.** The grep returns exactly 1 hit: `gate-5-mutants-query-api` already
exists and was added when the crate shipped (query-range-api-v0). It mutates
`crates/query-api/**` via `--package query-api --in-diff`. The DD3 change set
lands entirely in that crate:

- `crates/query-api/src/lib.rs` `router(...)` gains the optional static-dir
  parameter and the `ServeDir` fallback wiring;
- `crates/query-api/src/composition.rs` gains the pure
  `resolve_static_dir(env)` helper;
- `crates/query-api/src/main.rs` reads the env var (stays `#[mutants::skip]`,
  per DD3 -- keep the logic in the `composition`/`router` seam so the kill
  rate stays honest).

The diff therefore touches `crates/query-api/**`, so the existing
path-filtered `--in-diff` selects exactly the new mutable lines. Kill-rate
gate stays 100% (ADR-0005 Gate 5, CLAUDE.md mutation strategy). No new gate
is justified -- adding one would duplicate existing coverage.

## A2 -- Gate 1 auto-discovers the new query-api ServeDir tests

**Decision.** No Gate 1 change.

**Why.** Gate 1 runs `cargo test --all-targets --locked` over the whole
workspace. The new RED tests from DISTILL/DELIVER -- the `oneshot` ServeDir
route-precedence tests (`/config.json` served, `/` -> `index.html` SPA
fallback, `/api/v1/query_range` wins over the static fallback, and `None` ->
404) plus the `resolve_static_dir` unit test (DD6) -- live in the query-api
crate's test targets. `--all-targets` discovers them automatically; no
registration step, no Gate 1 edit.

## A3 -- Dependencies: tower-http `fs` feature only; Gate 4 a no-op pass

**Decision.** No Gate 4 (cargo deny) change; expect a no-op pass.

**Why.** The only dependency delta is enabling tower-http's `fs` feature on
`crates/query-api/Cargo.toml` (DD3). tower-http 0.6.8 is ALREADY in the
workspace `Cargo.lock`, pulled transitively via aperture's tree (ADR-0006).
Enabling a feature flag on an already-resolved crate adds ZERO new external
crates to the dependency graph. `cargo deny` (advisories/bans/licenses/
sources) sees no new crate node, so Gate 4 passes without new findings.
Licence: tower-http is MIT, compatible with query-api's AGPL-3.0-or-later.

## A4 -- No new toolchain pin

**Decision.** No change to `rust-toolchain.toml` and no new Node/pnpm pin.

**Why.** The Rust change compiles on the existing pinned stable toolchain;
no nightly feature, no edition bump. The prism change is a static asset (a
JSON file in `public/`), requiring no new Node or pnpm version -- the prism
gates already run on Node 22 / pnpm. No MSRV creep is triggered (no
transitive dep raises its rust-version), so the workspace floor is untouched
(memory: msrv_creep_is_ecosystem_reality -- not applicable here).

## FRONTEND-gate finding (from the grep)

**Finding: prism IS fully CI-gated.** The grep confirms six prism gates
(6-11) in `ci.yml`, all gated on `apps/prism/` changes. Mapping to this
feature:

- **Gate 6 (Vitest)** -- covers the US-01 mount tests (QueryPanel mounts
  against the served config; three error arms keep it dark).
- **Gate 7 (Playwright E2E)** -- covers the US-02 end-to-end series render.
- **Gate 8 (bundle size)** -- a small JSON file in `public/` does not affect
  the JS bundle budget; expect a pass.
- **Gate 9 (lint + format + AGPL header)** -- the committed `config.json` is
  data, not source; no header obligation on a JSON asset.
- **Gate 10 (StrykerJS mutation)** -- no new prism SOURCE, so no new mutation
  surface; in-diff selects nothing.
- **Gate 11 (Prism Prometheus contract)** -- the natural home for the
  same-origin end-to-end assertion (query-api serving `dist/` + `/api/v1`),
  with `KALEIDOSCOPE_QUERY_TENANT` set.

**No gap to fix overnight.** The frontend surface is gated. The DESIGN
handoff notes a residual DEVOPS task (DD6 / handoff): the Gate 11 fixture (or
a same-origin E2E job) should set `KALEIDOSCOPE_QUERY_STATIC_DIR` and
`KALEIDOSCOPE_QUERY_TENANT` so the same-origin path is exercised end-to-end.
That wiring belongs to the implementation/test work in DELIVER and would be a
`ci.yml`/test edit; it is recorded here as a follow-up, NOT actioned in this
slim wave (constraint: do not modify `ci.yml` or tests).

A second, narrower honest note for Andrea: the prism Vitest/Playwright suites
ALSO run locally outside CI via `pnpm --filter prism test`; the CI gates are
the authoritative verdict, the local run is fast feedback. No divergence
between the two was found in this grep.

---

## Summary of verdicts

| Item | Verdict | Evidence |
|---|---|---|
| A1 new Rust gate | NO | grep = 1; existing `gate-5-mutants-query-api` covers `crates/query-api/**` via `--in-diff` |
| A2 Gate 1 | auto-discovers tests | `cargo test --all-targets` |
| A3 dependencies | tower-http `fs` only; Gate 4 no-op pass | already in `Cargo.lock` (ADR-0006); zero new crates |
| A4 toolchain pin | NONE | stable unchanged; JSON asset needs no Node/pnpm pin |
| Frontend gate | PRESENT (Gates 6-11) | grep confirms; no gap to fix |

**Net CI change for this feature: zero. `ci.yml` is untouched.**

## Artifacts produced (DEVOPS)

- `devops/environments.yaml` (same-origin static-serving deploy shape)
- `devops/wave-decisions.md` (this file)
- `devops/kpi-instrumentation.md` (North Star -> test mapping)
- `devops/ci-cd-pipeline.md` (5+6 gate inheritance, no new job)
