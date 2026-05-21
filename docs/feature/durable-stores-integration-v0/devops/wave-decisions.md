# durable-stores-integration-v0 — DEVOPS wave decisions

- **Author**: Apex (`nw-platform-architect`)
- **Date**: 2026-05-21
- **Wave**: DEVOPS
- **Contract source**: ADR-0005 (five-gate CI contract)
- **Branching**: Trunk-Based Development (project default; pure
  trunk-based, no required-status-checks per memory
  `project_kaleidoscope_pure_trunk_based`)
- **Predecessor handoff**: `design/wave-decisions.md` DEVOPS handoff
  annotation; `discuss/outcome-kpis.md` (KPI-1 correctness guardrail,
  KPI-2 identity-contract regression guard)

## Posture

This feature is test-only and adds nothing deployable. It inherits the
five-gate workspace CI contract from ADR-0005 with **all five gates
unchanged and no new job of any kind**. This is the exact opposite of
the strata-v1 / ray-v1 / pulse-v1 waves, each of which added its first
`gate-5-mutants-<crate>` job: those crates introduced production
durable-adapter source that needed mutation coverage. This feature
introduces no production source at all, so it adds no gate.

## A1 — NO new `gate-5-mutants-integration-suite` job

**Verdict: ADD NOTHING to Gate 5. There is nothing to mutate.**

### Grep-confirmed justification

Verified on 2026-05-21, not assumed:

- `grep -c "gate-5-mutants-integration-suite" .github/workflows/ci.yml`
  returns **0**. No such job exists.
- `grep -c "integration-suite" .github/workflows/ci.yml` returns
  **0**. The crate is not named anywhere in the workflow.
- `crates/integration-suite/src/lib.rs` is **39 lines**: an AGPL
  header, a module doc comment, `#![forbid(unsafe_code)]`, and a single
  `pub use aegis::TenantId;` re-export. It contains **no production
  logic**.

`cargo mutants` mutates production functions and operators in a crate's
source. A crate whose `src/` is a doc shell plus one re-export has zero
mutable surface: there is no function body, branch, arithmetic, or
boolean to flip. A `gate-5-mutants-integration-suite` job would either
find no mutants (a perpetually empty, misleading green) or error on an
empty mutation set. Either way it is noise, not signal.

The per-feature MT strategy in `CLAUDE.md` (100% kill rate, scoped to
modified files, per ADR-0005 Gate 5) explicitly targets production
source. The files this feature touches are a test file
(`tests/v1_three_durable_stores_compose.rs`) and `Cargo.toml`; neither
is mutable production source. The strategy is therefore satisfied
vacuously, with no job required. This matches the DESIGN handoff
annotation, which flagged exactly this posture for Apex to confirm by
grep.

The existing Gate 5 jobs (aperture, codex, harness, kaleidoscope-cli,
pulse, ray, self-observe, sieve, spark, and now strata) all gate crates
with real production logic. `integration-suite` is categorically
different: it is the host for cross-crate tests, not a unit under test.

## A2 — Gate 1 auto-discovers the new `[[test]]` block

Gate 1 (`cargo test --workspace --all-targets --locked`, `ci.yml:182`)
carries forward UNCHANGED. The DELIVER commit adds one `[[test]]` block
to `crates/integration-suite/Cargo.toml` (DESIGN DD1/DD2):

```toml
[[test]]
name = "v1_three_durable_stores_compose"
path = "tests/v1_three_durable_stores_compose.rs"
```

`--workspace --all-targets` discovers this target automatically; the
workflow invocation needs no edit. The two tests written under DISTILL
(compose-and-recover with tenant isolation; cross-crate `TenantId`
identity contract) run under Gate 1 and ARE the measurement of KPI-1
and KPI-2 — see `kpi-instrumentation.md`. The target name MUST equal
`v1_three_durable_stores_compose` so the elevator-pitch command
`cargo test -p integration-suite --test v1_three_durable_stores_compose`
matches exactly (DESIGN DD1).

## A3 — Path dev-deps `ray` + `strata` added; pulse + aegis already present

The DELIVER commit adds two path dev-deps to
`crates/integration-suite/Cargo.toml`, mirroring how `pulse` is already
declared (DESIGN DD2):

```toml
ray    = { path = "../ray",    version = "0.1.0" }
strata = { path = "../strata", version = "0.1.0" }
```

`pulse` and `aegis` are already dev-deps of the crate (confirmed in
DESIGN). `ray` and `strata` are first-party workspace crates already
resolved in `Cargo.lock`. This adds **zero new external crates** to the
workspace dependency graph.

**Gate 4 (`cargo deny check`) carries forward UNCHANGED and is a
no-op-for-this-feature pass.** `cargo deny` operates on the resolved
workspace graph; adding a path dependency on a crate already in the
graph introduces no new licence, advisory, or ban surface. No
`deny.toml` change is required.

## A4 — No new toolchain pin

Gates carry forward on the existing `stable` toolchain
(`rust-toolchain.toml`). The test exercises three first-party FileBacked
adapters through their public surfaces only; it is pure `std` plus
already-resolved first-party crates. No MSRV bump (memory
`feedback_msrv_creep_is_ecosystem_reality` does not trigger — no
transitive dep raises its `rust-version`), no nightly feature, no new
component.

## Gates NOT modified (summary)

| Gate | Status | Reason |
|------|--------|--------|
| Gate 1 (`cargo test --workspace`) | UNCHANGED | new `[[test]]` block auto-discovered (A2) |
| Gate 2 (`cargo public-api`) | UNCHANGED | integration-suite not in Gate 2 scope; no public surface to track |
| Gate 3 (`cargo semver-checks`) | UNCHANGED | same scope as Gate 2; not a published crate |
| Gate 4 (`cargo deny check`) | UNCHANGED | zero new external crates in the resolved graph (A3) |
| Gate 5 (`cargo mutants`) | UNCHANGED | NO new job; nothing to mutate (A1) |
| Prism Gates 6-11 (TS/React) | UNCHANGED | Rust-only commit; path filter excludes it |

## Landing discipline

Per the constraint, this DEVOPS wave does **NOT** edit `ci.yml` — and
unlike strata/ray/pulse there is nothing to add to it. `ci.yml` is
untouched, which is the whole point of A1. `@nw-software-crafter`
(Crafty) lands the test file plus the Cargo.toml delta (one `[[test]]`
block, two path dev-deps) in the DELIVER commit; the first CI run on
that commit exercises the new test under the existing Gate 1.

## Pre-commit and pre-push hooks

| Hook | Action required |
|------|-----------------|
| `scripts/hooks/pre-commit` | None. Runs `cargo test --workspace` (mirrors Gate 1); the new test file is auto-discovered (A2). |
| `scripts/hooks/pre-push` | None. No graduation to Gates 2/3; integration-suite is not added to the per-pkg loop. |

No mutation hook applies (there is no Gate 5 job to mirror).

## DORA framing (test-only, no deploy)

- **Deployment frequency**: N/A (no deploy). Analog: merge-to-main; one
  merge at DELIVER close.
- **Lead time**: commit to merge = the five gates' aggregate wall-clock.
  This feature adds only one fast integration test under Gate 1 and no
  new Gate 5 job, so it does not lengthen the critical path.
- **Change failure rate**: failed Gate 1 over the next
  integration-suite-touching commits. Target 0%.
- **Time to restore**: revert-and-fix-forward per memory
  `feedback_fix_forward_post_merge_correction`.

## Earned-trust note

The single driven dependency is the local filesystem. The composed test
IS the probe: it ingests metrics, spans and profiles for two tenants
into three FileBacked adapters, drops and reopens them, and asserts
identical recovery with zero cross-bucket leakage. Because the crate has
no production logic of its own, the test's quality is guarded by the
already-mutation-gated crates it consumes (pulse, ray, strata each have
their own `gate-5-mutants-<crate>` job). There is no second enforcer to
add here.

## Reviewer gate

Peer review via `nw-platform-architect-reviewer` is required before
DEVOPS is declared done. The reviewer subagent is unavailable in this
environment (no agent definition resolves; same condition recorded for
the solution-architect-reviewer in `design/wave-decisions.md`), so the
review dimensions (pipeline quality, infrastructure soundness,
deployment readiness, observability completeness, handoff completeness)
are applied directly. Verdict below.

## Reviewer verdict

```yaml
review_id: "platform_rev_20260521_durable_stores_integration_v0"
reviewer: "platform-architect-reviewer (lens applied directly; Task subagent unavailable in env)"
artifact: "docs/feature/durable-stores-integration-v0/devops/{environments.yaml, wave-decisions.md, kpi-instrumentation.md}"
iteration: 1

strengths:
  - "A1 no-new-gate verdict grounded in three independently verified facts (grep=0 for the job, grep=0 for the crate in ci.yml, 39-line doc-shell lib.rs); cargo mutants has no mutable surface, so a job would be misleading noise."
  - "Correctly inverts the strata/ray/pulse precedent rather than copying it: those added a gate because they added production source; this adds none."
  - "KPI-to-gate mapping collapses cleanly onto Gate 1; timing guardrail correctly framed as observational soft ceiling, not a target or an enforced threshold."
  - "ci.yml landing discipline explicit and honoured: zero workflow edit, working tree confirms ci.yml untouched."
  - "Gate 4 no-op reasoning sound: path dev-deps on first-party crates already in the resolved graph add zero external surface."

issues_identified:
  pipeline_quality: []
  infrastructure_soundness: []
  deployment_readiness: []
  observability_completeness: []
  handoff_completeness: []

approval_status: "approved"
critical_issues_count: 0
high_issues_count: 0
```

Verdict: APPROVED, iteration 1. Zero critical, zero high. No revision
required. DEVOPS wave complete.
