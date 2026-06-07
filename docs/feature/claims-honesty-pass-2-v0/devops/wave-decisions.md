<!-- markdownlint-disable MD013 MD024 -->

# Wave Decisions — claims-honesty-pass-2-v0 (DEVOPS)

- **Wave**: DEVOPS (nWave, SLIM)
- **Engineer**: Apex (nw-platform-architect)
- **Date**: 2026-06-07
- **Mode**: autonomous overnight; no questions returned to the operator.
- **Inputs**: DESIGN `wave-decisions.md` (the 9-locus overstatement->truth
  table; prism-e2e flag resolved MARK; NO ADR; NO semver; the structural-test
  seam; mixed DELIVER ownership), DISCUSS `outcome-kpis.md` (KPI-1/2/3,
  north-star = residual overstatements 0), ADR-0005 (the five CI gates),
  ADR-0072 (local hook now FAST `--lib`; deep tests gate in CI),
  `.github/workflows/ci.yml`, the `claims-honesty-pass-v0/devops` sibling.

## Headline

**The standing five-gate pipeline already covers this feature in full.
No new CI job, no new gate, no edit to any existing job. The one net-new
artefact (a std::fs structural guard test) lands in the integration-suite
`tests/` directory and is run by the EXISTING CI gate-1.**

This is a doc/comment/config-honesty feature plus ONE structural test.
DESIGN resolved its single flag (prism-e2e) to MARK, so there is ZERO
production-behaviour change, NO new crate/service/deploy surface, NO new
dependency, NO semver bump. DEVOPS here is a confirmation that the
standing pipeline absorbs the work — **deliberately not over-built**, in
step with the DESIGN posture and the pass-v0 precedent.

## nWave-order note (read before judging "missing" code)

The nWave order is DISCUSS -> DESIGN -> **DEVOPS** -> DISTILL -> DELIVER.
DEVOPS runs BEFORE DISTILL and DELIVER. The structural guard test and the
prose/comment/config corrections themselves DO NOT EXIST YET — that is the
EXPECTED, CORRECT state at this wave. This wave decides where those
artefacts will run and confirms the gates that catch them; authoring them
is DISTILL's and DELIVER's job. Absent corrections/tests at DEVOPS-close
is not a defect. (RED-today verified 2026-06-07: the false strings
`In-memory only at v0` in `pulse/src/lib.rs`, `RED-ready NO-OP` in
`gateway/src/main.rs`, and `no-op subscriber` in the gateway test prose
are all still present — so the guard, once written, fails today, exactly
the Earned-Trust shape.)

## Decision 1 — Deploy strategy: N/A (no deploy surface)

No service is deployed by this feature or by Kaleidoscope itself
(operators run the binaries). No rolling / blue-green / canary applies.
The "delivery target" is corrected prose/comments/config plus one
structural guard test. **Rollback = `git revert`** (degenerate; no
runtime surface). Recorded per the rollback-first principle even though
the surface is empty.

## Decision 2 — CI delta: NO new job, NO new gate

The feature changes doc-comments + a Cargo.toml description + comments +
test prose + README + a playwright config across THREE existing crates
(pulse, kaleidoscope-gateway) and the repo-root README / apps/prism, plus
ONE new structural test in the existing integration-suite crate. No new
crate is created.

- **Gate 1 (`gate-1-test`, ci.yml:182)** runs
  `cargo test --workspace --all-targets --locked`. Because it is
  `--workspace --all-targets`, it compiles and runs every
  `crates/**/tests/*.rs` integration binary — INCLUDING the new
  integration-suite structural guard bin — the moment DELIVER commits it,
  with **zero workflow edits**. This is the exact home of the existing
  structural-test precedent bins
  `crates/integration-suite/tests/v0_fast_precommit_structure.rs` and
  `v0_perf_kpi_ci_non_gating_structure.rs`, confirmed present on disk.
- **Gate 5 (`gate-5-mutants-<crate>`, `--in-diff`)** exists for every
  crate this feature touches:
  - `gate-5-mutants-pulse` (ci.yml:1443 / `--in-diff` :1507)
  - `gate-5-mutants-kaleidoscope-gateway` (ci.yml:2475)
  - `gate-5-mutants-integration-suite` (ci.yml:2389)
- **Verdict**: no new CI job, no new gate, no edit to any existing job.
  The pipeline is already shaped for this work.

## Decision 3 — Structural test runs in CI gate-1, NOT the local --lib hook (confirmed FINE)

Per ADR-0072 the local pre-commit hook's Step 4 is now the FAST subset
`cargo test --workspace --lib --locked` — it runs only in-`src/`
`#[cfg(test)]` unit tests and deliberately runs NONE of the
`crates/**/tests/*.rs` integration binaries.

The new structural guard is a `tests/*.rs` integration bin (a std::fs
string test in the integration-suite). Therefore **`--lib` does NOT run
it locally** — and that is **fine and expected**:

- The structural test is fast (pure std::fs read + substring match; no
  fsync, no subprocess, no wall-clock), so it does NOT contribute to the
  10-20 min Step-4 problem ADR-0072 solved. But it is still a `tests/` bin
  by construction, so it sits outside `--lib` scope regardless.
- It gates in **CI gate-1** (`--all-targets`), the authoritative deep
  gate, exactly like the existing `v0_fast_precommit_structure.rs` and
  `v0_perf_kpi_ci_non_gating_structure.rs` structural bins, which are also
  only run by CI gate-1, not the local `--lib` hook. This is established
  precedent, not a new gap.
- Under the trunk-based "CI is feedback, not a gate" posture, the guard
  being CI-side (not local-blocking) is consistent and correct.

**Confirmation: the structural test running in CI's deep gate-1 rather
than the local `--lib` hook is fine, by ADR-0072 design and by the
existing structural-test precedent.**

## Decision 4 — Determinism: confirmed

The single new test is fully deterministic and free of wall-clock / p95 /
ordering / fsync / subprocess dependence:

- It reads each corrected file with `std::fs::read_to_string` and asserts
  the false phrase is ABSENT and the corrected phrase is PRESENT. Pure
  file-read + string match. No timing surface to flake on (unlike the
  known lumen/pulse p95 KPI tests that flake under overnight load).

The behaviour guardrails this feature must NOT degrade (DESIGN +
outcome-kpis) are protected by EXISTING tests, all already deterministic
and already run by gate-1: the pulse durability + snapshot tests, the
gateway always-run tracing AC-02 scenarios, the unchanged `#[ignore]`
attributes, and the prism per-spec `UNIMPLEMENTED` bodies.

## Decision 5 — Mutation: N/A for this feature (empty surface)

Per CLAUDE.md and ADR-0005 Gate 5, per-feature mutation testing (100%
kill rate) applies to **modified PRODUCTION-logic files**. This feature
modifies none:

- `pulse/src/lib.rs`, `gateway/src/main.rs` changes are **doc-comments**
  (`//!` / `///`) and **comments**, not production-logic lines. A
  doc-comment adds no mutable line; `cargo-mutants --in-diff` on the pulse
  / gateway diff finds the doc lines non-mutable -> no viable mutant ->
  `gate-5-mutants-pulse` / `-kaleidoscope-gateway` are a **GREEN no-op**.
- `pulse/Cargo.toml` `description` is **metadata**, not a mutable line.
- The gateway test-prose edit and the README / playwright.config.ts /
  prism-README edits are **prose/config**, not production logic.
- The ONE net-new artefact is a `tests/*.rs` **test bin**; cargo-mutants
  does not mutate test bins, so `gate-5-mutants-integration-suite`
  introduces no new mutant (GREEN).

**Mutation surface for this feature = empty. N/A is the correct,
recorded state — explicitly NOT a coverage gap.** No CLAUDE.md
mutation-strategy change; the existing per-feature strategy already fits.

## Decision 6 — Public-API / SemVer: NOT triggered, NO bump

Verified against `.github/workflows/ci.yml`:

- **Gate 2 (`cargo public-api`, ci.yml:385-407)** and **Gate 3
  (`cargo semver-checks`, ci.yml:479-490)** are scoped to
  **`otlp-conformance-harness`, `spark`, `sieve`, `codex` ONLY**. pulse,
  kaleidoscope-gateway, and prism are **NOT** in the public-surface lock —
  so even an actual API change to pulse/gateway would not trip those
  gates, and a doc/metadata change certainly does not.
- Independently: a `Cargo.toml` `description` change is package
  **metadata**, not the Rust API surface. `cargo public-api` diffs public
  Rust items (types, fns, traits, re-exports), **not** the manifest
  `description`. So a description edit is not an API diff EVEN IF pulse
  were gated.
- A doc-comment (`//!` / `///`) change is not a public-API change. No
  `pub` item is added, removed, or changed in any crate.
- **No semver bump.** All three crates stay at `0.1.0`. **Never 1.0.0**
  (project memory: semver 1.0.0 is Andrea's call).

## DELIVER ownership / split — RECOMMENDATION

DESIGN flagged this as mixed-ownership. Two clean options; both honour the
CLAUDE.md rule that **the crafter writes only `crates/<name>/src/`**.

| Surface | Owner under SPLIT (recommended) | Notes |
|---|---|---|
| `crates/pulse/src/lib.rs` doc-comments | **crafter** | edits a `crates/*/src/` file — crafter territory by CLAUDE.md, even though they are doc-comments |
| `crates/kaleidoscope-gateway/src/main.rs` comments | **crafter** | same — `crates/*/src/` file |
| `crates/kaleidoscope-gateway/tests/slice_01_tracing_subscriber.rs` prose | **crafter** | Rust test source under a crate |
| `crates/integration-suite/tests/<guard>.rs` (the structural test) | **crafter** | the net-new Rust test bin |
| `crates/pulse/Cargo.toml` `description` | **Apex** (non-crafter) | package metadata, not `src/` |
| `README.md` (Prism row + cost line) | **Apex** (non-crafter) | repo-root docs |
| `apps/prism/README.md` (`pnpm playwright` note) | **Apex** (non-crafter) | app docs |
| `apps/prism/playwright.config.ts` (MARK header) | **Apex** (non-crafter) | TS config, not a Rust crate |

**Recommendation: SPLIT (the table above).** Rationale:

1. It respects the CLAUDE.md boundary literally: every `crates/<name>/src/`
   or crate-test edit + the new Rust guard test goes to the crafter; the
   non-crate docs/metadata/config (`README.md`, `pulse/Cargo.toml`
   description, `apps/prism/*`) go to Apex. This is the same split DESIGN
   recorded in its routing note.
2. The single structural guard test (a crafter artefact) reads ALL the
   corrected files — including the Apex-owned non-`.rs` ones via
   `std::fs` — so the regression net is unified in ONE place
   (integration-suite) regardless of who edits each source. The split of
   the *edits* does not fragment the *guard*.

**Considered alternative — single-agent (Apex does everything).** Because
the `pulse/src/lib.rs` and `gateway/src/main.rs` changes are
**doc-comments only** (not src logic), one could argue Apex could make
them too. **Rejected** as the default: the CLAUDE.md rule is by FILE
LOCATION (`crates/<name>/src/`), not by "is it logic or a comment", and
the net-new guard test is unambiguously a Rust test the crafter writes.
Keeping the crafter as the sole author of anything under `crates/*/` —
including the new test — avoids relitigating the boundary per-line and
keeps the rule a bright line. SPLIT is therefore preferred; single-agent
is recorded only as the considered-and-rejected lighter alternative.

**Upstream changes: none from this DEVOPS wave.** (DESIGN noted a
one-line brief pointer; that is a DESIGN/DISCUSS artefact, not a DEVOPS
edit. This wave touches no `docs/evolution/` and commits nothing.)

## Infrastructure Summary

**Inherits the five gates from the prior waves, unchanged.** No new gate,
no new job, no new environment, no new dependency, no deploy surface.

- Gate 1 (`cargo test --workspace --all-targets --locked`) runs the new
  structural guard test in CI with zero edits.
- Gates 2/3 (`public-api` / `semver-checks`) are NOT triggered (pulse /
  gateway / prism out of the scoped lock; metadata/doc-comment changes are
  not API diffs anyway).
- Gate 4 (`cargo deny`) unaffected — no dependency added or changed.
- Gate 5 (`--in-diff` mutation) is a GREEN no-op for pulse / gateway
  (doc-comment + metadata diff -> no viable mutant) and adds no mutant for
  integration-suite (a test bin).
- Local hook (ADR-0072): FAST `--lib`; the structural test (a `tests/`
  bin) runs in CI gate-1, not the local hook — confirmed fine (Decision 3).

## Constraints

- **No-new-gate / no-new-job**: the standing pipeline absorbs the feature
  verbatim. New CI job count: 0.
- **No-semver**: doc-comments + a Cargo.toml `description` + prose/config
  are not public-API changes; Gate 2/3 not triggered and out of scope;
  all crates stay 0.1.0; never 1.0.0.
- **Mutation N/A for doc-comment edits**: doc/comment/metadata/config
  changes add no mutable production line; the new artefact is a test bin.
  Empty mutation surface, recorded as N/A, not a gap.
- **Structural test runs in CI gate-1**: it is a `tests/*.rs` bin outside
  the local `--lib` scope (ADR-0072), gating in CI's deep gate-1 alongside
  the existing structural-test precedent bins. Fast and deterministic.
- **Pure trunk-based, CI-is-feedback** (project memory): main has no
  required status checks, no enforce_admins. The guard test is the
  regression net, not a merge gate; a doc-only change is never blocked.
- **Rollback** of a prose/config/guard-test change is `git revert`; no
  runtime deploy surface exists. (Fix-forward + post-merge correction
  posture applies for any small later defect: push directly, append a note
  to wave-decisions.md, do not reopen a feature.)
- **No CLAUDE.md change** for this feature (no mutation-strategy change;
  DELIVER plans none beyond the in-scope edits, which do not touch
  CLAUDE.md).
- **No C4 / topology change** — byte-identical before and after.
- **Upstream changes: none** from this DEVOPS wave; no `docs/evolution/`
  touched; nothing committed.

## What this DEVOPS wave does NOT do

- Does NOT add a CI job, gate, environment, dependency, or deploy surface.
- Does NOT bump any crate version (never 1.0.0).
- Does NOT change behaviour or weaken any real capability.
- Does NOT author the corrections or the guard test (DISTILL/DELIVER job;
  their absence now is the correct nWave-order state).
- Does NOT proceed into DISTILL (per the brief).
- Does NOT commit, and does NOT touch `docs/evolution/`.

## Peer review

`nw-platform-architect-reviewer` dispatch is not separately
nested-invocable from this sub-agent context. A structured self-review
against the platform-architect critique dimensions is recorded below; the
wave is marked **APPROVED_PENDING_INDEPENDENT_REVIEW** with **0 blocking
issues**, flagged for a top-level reviewer run WITH the nWave-order
reminder above (so a reviewer does not reject on the correct, expected
absence of the not-yet-written corrections + guard test).

### Self-review (platform-architect critique dimensions)

| Dimension | Assessment |
|---|---|
| Measure-before-action | PASS. CI contract read directly at HEAD: gate-1 line (ci.yml:182), gate-2/3 crate scope (ci.yml:385-407, 479-490), gate-5 job ids (pulse 1443, gateway 2475, integration-suite 2389). RED-today confirmed by grep (3 false strings still present). No data assumed. |
| Existing-infrastructure-first | PASS. Zero new components. The three touched crates' gate-5 jobs, the workspace gate-1, and the integration-suite structural-test home (`v0_fast_precommit_structure.rs` precedent) are reused verbatim. New CI job count: 0. |
| Simplest-infrastructure-first | PASS. The simplest possible outcome — "the standing pipeline already covers it" — is the recommendation. No environment, orchestration, or deploy machinery added; no new gate. |
| SLO-driven / observability | N/A by construction — zero behaviour change, no runtime signal. The outcome KPIs (residual overstatement counts) are measured by the structural guard test itself, which gate-1 runs. Confirmed not a gap (matches outcome-kpis "no runtime telemetry required"). |
| Rollback-first | PASS (degenerate). `git revert` is the rollback for a prose/config/test change; no runtime surface exists. Stated explicitly (Decision 1). |
| Shift-left security | N/A. No dependency added (Gate 4 `cargo deny` unaffected), no new attack surface, no secret. A doc/config-honesty change has no security gate to shift. |
| Determinism / measurement coupling | PASS. The one new test is pure std::fs read + substring match; no wall-clock/p95/fsync/subprocess. Explicitly contrasted with the known lumen/pulse p95 flake. The ADR-0072 `--lib`-vs-`--all-targets` coupling is addressed head-on (Decision 3): the structural bin is correctly CI-gate-1-only, matching precedent. |
| Mutation strategy fit | PASS. Per-feature 100% on modified production files; this feature modifies none (doc-comments + metadata + prose + a test bin). Empty surface -> N/A, recorded not as a gap. No CLAUDE.md edit. |
| Public-API / semver discipline | PASS. Gate 2/3 scope verified to exclude pulse/gateway/prism; `description` is metadata not an API diff; doc-comments are not API changes; no `pub` item changes; no bump; never 1.0.0. |
| Completeness of DELIVER handoff | PASS. The mixed-ownership split is given as a concrete per-file table with a clear SPLIT recommendation and a considered-and-rejected single-agent alternative, plus the unified-guard-in-one-place note. The structural-test home + portable-path idiom are pinned. |

**Self-review verdict**: **APPROVED_PENDING_INDEPENDENT_REVIEW — 0
critical / 0 high / 0 medium blocking issues.** Single watch-item: the
guard test must use `env!("CARGO_MANIFEST_DIR")`-relative pathing to reach
the non-crate files (`README.md`, `apps/prism/*`) from the
integration-suite crate dir — already pinned as a DISTILL/DELIVER
actionable in environments.yaml (`structural_test_home`), mirroring the
pass-v0 README-path-portability note. Approved to hand to DISTILL (by a
separate wave; NOT performed here — this wave does not proceed into
DISTILL).
