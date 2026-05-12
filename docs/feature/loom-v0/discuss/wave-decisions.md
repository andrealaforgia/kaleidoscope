# DISCUSS Decisions — loom-v0

## Key decisions

- **[D1] CLI + library, no server at v0.** `loom` ships as a Rust
  crate exposing the planner / applier, plus a `loom` binary that
  wraps the library with a CLI. The Git-backed daemon arrives at
  v1; at v0 the binary runs in CI on pull-request and on `main`
  push. (See: user-stories.md system constraint 1.)
- **[D2] Beacon rules only at v0.** Sieve sampling, Prism
  dashboards, and Aegis policies arrive at v1 / v2 once each
  consumer's contract is settled. The Loom pattern transfers
  verbatim. (See: system constraint 3.)
- **[D3] TOML at v0, mirroring Beacon ADR-0034.** Same SPIKE
  outcome applies: the Rust CUE ecosystem is too sparse to deliver
  the diagnostic quality KPI 4 demands. Migration is a parser swap
  when CUE matures. (See: system constraint 4.)
- **[D4] Three commands.** `loom validate`, `loom plan`,
  `loom apply`. No `loom rollback`, `loom diff` (use git), or
  `loom deploy` (the operator owns the deployment step). Minimal
  surface; future additions are additive. (See: system constraint
  5.)
- **[D5] No separate state file.** Git history is the state.
  Drift is detected by `loom plan`. Per-rule deployment audit is
  `git log -- path/to/rule.toml`. (See: system constraint 6.)
- **[D6] Local filesystem only at v0.** SSH / volume-mounted
  directories are how operators apply Loom's output to remote
  Beacon deployments. Remote API arrives at v1. (See: system
  constraint 7.)
- **[D7] Idempotent apply.** `loom apply` twice on the same input
  produces byte-equal output and zero file writes on the second
  run. This is the load-bearing safety contract. (See: KPI 3.)
- **[D8] Wraps `beacon::load_rules` as runtime dep.** No schema
  re-implementation. Loom is a Cargo workspace member alongside
  Beacon; the two crates evolve together at v0. (See: slice 01
  brief.)
- **[D9] AGPL-3.0-or-later.** Same licensing as every platform
  component. (See: system constraint 2.)
- **[D10] No telemetry-on-telemetry.** Loom is a CLI; no OTLP
  emission. Operator stdout/stderr is the audit trail. (See:
  system constraint 9.)

## Requirements summary

- Primary user need: a Git-backed, reviewable, auditable
  change-control surface for the Beacon rule catalogue. Same
  pattern transfers to Sieve sampling and Prism dashboards in
  future v1 / v2 cycles.
- Walking skeleton scope: `loom validate --rules <dir>` invokes
  `beacon::load_rules`, exits with operator-readable diagnostics.
  (Slice 01.)
- Feature type: backend (CLI tooling, no UI).

## Constraints established

- Loom v0 cannot depend on Sieve, Prism, Aegis, or a remote API.
- Loom must remain CI-fast: validate ≤ 100 ms on 50-rule corpus.
- `loom apply` writes only `.toml` files; non-TOML files in the
  destination directory are preserved untouched.

## Upstream changes

None. Beacon v0 shipped first; Loom DISCUSS reaffirms Beacon's
public `load_rules` API as the integration point. ADR-0034's TOML
choice carries forward unchanged.
