# CI/CD Pipeline: prism-backend-wiring-v0 (DEVOPS)

- **Author**: `nw-platform-architect` (Apex). Date: 2026-05-21.
- **DESIGN basis**: commit e297292, ADR-0043, `design/wave-decisions.md`.
- **Branching**: pure trunk-based; CI is feedback, not a gate (no
  required-status-checks, no enforce_admins on main).
- **Constraint honoured**: `.github/workflows/ci.yml` is NOT modified by this
  wave. The pipeline below is INHERITED whole.

## Verdict: no new CI job is needed

This feature changes two files of substance -- a `ServeDir` fallback in the
query-api crate and a committed `apps/prism/public/config.json` -- and both
fall inside surfaces that EXISTING gates already cover. The grep
(`grep -c "gate-5-mutants-query-api"` returns 1; the frontend grep returns
Gates 6-11) confirms the coverage is in place. Net CI change: zero.

## The five Rust gates (inherited)

| Gate | What it does | Coverage of this feature |
|---|---|---|
| Gate 4 -- cargo deny | advisories, bans, licences, sources | no new external crate (only tower-http `fs` feature, already in `Cargo.lock`); A3 -> no-op pass |
| Gate 1 -- cargo test --all-targets --locked | whole-workspace test discovery | auto-discovers the DD6 `oneshot` ServeDir + `resolve_static_dir` tests; A2 -> no change |
| Gate 2 -- cargo public-api | public surface lock (diff vs main) | `router(...)` gains an optional parameter -- a public-surface change that Gate 2 will flag for review; expected and intentional, accept the diff |
| Gate 3 -- cargo semver-checks | SemVer compliance | the `router` signature change is additive within a 0.x crate; Gate 3 verdict per its own rules |
| Gate 5 -- cargo mutants (query-api) | `--package query-api --in-diff` on `crates/query-api/**` | A1: the ServeDir wiring + `resolve_static_dir` helper are mutation-killed; kill rate 100% (ADR-0005 Gate 5) |

Note on Gates 2/3: the only NEW public surface is the optional `static_dir`
parameter on `router(...)`. This is an intended, additive change; the gates
report it for review rather than block (trunk-based: CI is feedback). Keep
`main.rs` `#[mutants::skip]` so Gate 5's kill surface stays in the
`composition`/`router` seam (DD3).

## The six prism gates (inherited)

| Gate | What it does | Coverage of this feature |
|---|---|---|
| Gate 6 -- Prism Vitest (typecheck + unit/integration) | `pnpm --filter prism vitest run` | US-01 mount tests; config.json validates against the real loader (KPI 1, 3) |
| Gate 7 -- Prism Playwright E2E | `pnpm --filter prism playwright` | US-02 end-to-end series render (KPI 2 / North Star) |
| Gate 8 -- Prism bundle size (<=300 KB gzipped) | `pnpm --filter prism build` then check | a JSON asset in `public/` does not affect the JS budget; pass |
| Gate 9 -- Prism lint + format + AGPL header | `pnpm --filter prism lint` + `format:check` | config.json is data, not source; no header obligation |
| Gate 10 -- Prism mutation (StrykerJS, in-diff) | StrykerJS baseline cascade | no new prism SOURCE -> no new mutation surface |
| Gate 11 -- Prism Prometheus contract (container fixture) | container-backed E2E | same-origin assertion home (KPI 2/3); `KALEIDOSCOPE_QUERY_TENANT` set |

All six are gated on `apps/prism/` changes via the `on.push.paths` filter +
per-job conditional, so the committed `config.json` triggers them.

## Frontend-gate status

**PRESENT and complete.** prism is fully CI-gated (Gates 6-11). There is NO
frontend-gate gap to record. One residual follow-up (NOT actioned here, would
require a `ci.yml`/test edit): wire the Gate 11 same-origin fixture to set
`KALEIDOSCOPE_QUERY_STATIC_DIR` so the combined deploy shape (query-api
serving `dist/` + `/api/v1`) is exercised end-to-end. That belongs to DELIVER
test work; this slim DEVOPS wave does not touch `ci.yml` or tests.

## Local quality gates (developer machine)

Mirroring the remote commit stage (per cicd skill): `cargo test` and
`cargo fmt --check` for the Rust seam; `pnpm --filter prism test`
(`vitest && playwright`) and `pnpm --filter prism lint` for the prism seam.
These give seconds-to-minutes feedback before the push; CI remains the
authoritative verdict.

## DORA posture (unchanged)

Trunk-based, every commit to main runs the full pipeline as feedback. This
feature adds no job, no gate, no toolchain pin -- so lead time and deployment
frequency are unaffected; the change is additive and default-off, keeping
change failure rate risk minimal (the API-only path cannot regress when
`KALEIDOSCOPE_QUERY_STATIC_DIR` is unset).
