# Wave Decisions — claims-honesty-pass-v0 (DEVOPS)

- **Wave**: DEVOPS (nWave, SLIM)
- **Engineer**: Apex (nw-platform-architect)
- **Date**: 2026-06-05
- **Mode**: autonomous overnight; no questions returned to the operator.
- **Inputs**: DESIGN `wave-decisions.md` (both flags DOCUMENT), ADR-0062
  (query_range raw points / `step` reserved), DISCUSS `user-stories.md`
  (US-01..US-06) and `story-map.md`, `.github/workflows/ci.yml`.

## Headline

**Existing gates already cover this feature in full. No new CI job is
needed. The only wrinkle is README-path portability for the doc-lint
guard (decision 3).**

This is a prose-honesty feature. Both DESIGN document-vs-implement flags
resolved DOCUMENT, so there is ZERO production-behaviour change and NO
new crate, service, or deploy surface. DEVOPS here is a confirmation that
the standing five-gate pipeline absorbs the work, not a build-out.
**Deliberately not over-built**, in step with the DESIGN wave's posture.

## nWave-order note (read before judging "missing" tests/code)

The nWave order is DISCUSS → DESIGN → **DEVOPS** → DISTILL → DELIVER.
DEVOPS runs BEFORE DISTILL and DELIVER. The doc-lint/grep guard tests,
the two behaviour tests, and the prose corrections themselves DO NOT EXIST
YET — that is the EXPECTED, CORRECT state at this wave. This wave decides
where those artefacts will run and confirms the gates that will catch
them; authoring them is DISTILL's and DELIVER's job. Absent tests/code at
DEVOPS-close is not a defect.

## Decision 1 — CI delta: NO new job

The feature changes docs / strings / codenames in five EXISTING crates
plus the repo-root README and a couple of doc comments. No new crate.

- **Gate 1 (`gate-1-test`, ci.yml:184)** already runs
  `cargo test --workspace --all-targets --locked`. Because it is
  workspace-scoped, it will run every guard test (US-01..US-04 doc-lint /
  grep guards) and both behaviour tests (US-05 query-api step-invariance;
  US-06 harness framing-inert) the moment DISTILL/DELIVER commits them —
  with **zero workflow edits**.
- **Gate 5 (`gate-5-mutants-<crate>`, `--in-diff`)** exists for all five
  touched crates:
  - `gate-5-mutants-query-api` (ci.yml:1038)
  - `gate-5-mutants-otlp-conformance-harness` (ci.yml:455)
  - `gate-5-mutants-codex` (ci.yml:779)
  - `gate-5-mutants-query-http-common` (ci.yml:1811)
  - `gate-5-mutants-trace-query-api` (ci.yml:1299)

  For the four pure-doc slices (US-01..US-04, US-03), the `--in-diff`
  diff is comment / string / markdown only. cargo-mutants finds no
  mutable production lines, so each gate-5 is a **GREEN no-op** (the
  empty-diff short-circuit, or no-mutable-lines on a doc-only diff). This
  is explicitly NOT a coverage gap: per CLAUDE.md and ADR-0005 Gate 5 the
  per-feature 100% kill rate applies to **modified PRODUCTION files**, and
  a doc-only slice has none.

  The two behaviour-test slices (US-05 query-api, US-06 harness) DO touch
  a testable surface — but because DESIGN chose DOCUMENT, they add only
  TESTS, not new production code. The new step-invariance and framing-
  inert tests strengthen the existing query-api / harness mutation kills
  rather than introducing new mutants. Their gate-5 jobs cover them
  either way.

- **Verdict**: no new CI job, no edit to any existing job. The pipeline
  is already shaped for this work.

## Decision 2 — Determinism: confirmed

Every test this feature will add is deterministic and free of wall-clock
/ p95 / ordering dependence:

- **Doc-lint / grep guards (US-01..US-04, US-03)**: assert a false string
  is ABSENT and the corrected string is PRESENT in a fixed file. Pure
  file-read + string match. Deterministic.
- **US-05 behaviour test**: fixed `query`/`start`/`end`; two `step` values
  (`15s`, `60s`) plus omitted-`step` → byte-identical output (the
  INVARIANCE contract from ADR-0062). No time dependence; the window is
  fixed input, not "now". Deterministic.
- **US-06 behaviour test**: prefix-stripped bytes validate identically
  under `HttpProtobuf` and `GrpcProtobuf` (framing inert); a still-
  length-prefixed body under `GrpcProtobuf` fails to decode. Pure-function
  over fixed bytes. Deterministic.

All run identically in the local `clean` hook and in `ci`. None resemble
the lumen/pulse p95 KPI tests that flake under overnight load — there is
no timing surface here to flake on.

## Decision 3 — README-path portability (the one wrinkle)

The repo-root `README.md` is the loudest corrected surface (US-01
codename table + cost line; US-04 harness depth/status; US-05 query_range
framing). It belongs to **no crate**. A guard test compiled inside a
crate's `tests/` directory must reach UP to the workspace root to read it.

- **Idiom already in the tree**: `otlp-conformance-harness/tests/
  slice_07_lock_the_contract.rs` resolves workspace paths via
  `env!("CARGO_MANIFEST_DIR")` + `PathBuf` joins. Crates live at
  `crates/<crate>/`, so the repo root is `../../` from any crate manifest
  dir; the README is `env!("CARGO_MANIFEST_DIR")/../../README.md`.
- **Recommendation to DISTILL**: locate the README with
  `CARGO_MANIFEST_DIR`-relative pathing (NOT a hard-coded absolute path
  and NOT a CWD-relative path — `cargo test`'s CWD is not guaranteed to be
  the crate root across runners). Prefer a SINGLE dedicated docs-guard
  test hosted in `otlp-conformance-harness/tests/` (it already reads
  workspace files and already sits under both gate-1 and a gate-5 job)
  over scattering README greps across multiple crates. One portable
  anchor; one place to maintain the `../../` relative hop. This keeps the
  guard robust whether run from the local hook (CWD = repo root) or a CI
  runner (CWD = crate or workspace, depending on invocation).

This is a DISTILL authoring concern; it changes no CI topology. Recorded
here because it is the single DEVOPS-relevant subtlety in an otherwise
fully-covered feature.

## Constraints reaffirmed (from project memory + DESIGN)

- **Pure trunk-based, CI-is-feedback**: main has no required status checks
  and no enforce_admins. The guard tests are the regression net, not a
  merge gate. A doc-only change is never blocked by CI.
- **Per-feature mutation 100%** applies to modified PRODUCTION files;
  doc-only slices have none (recorded as a guardrail, not a gap). The two
  behaviour slices add tests only (DOCUMENT chosen), so they introduce no
  new production mutants.
- **Rollback** of a prose / guard-test change is `git revert`; there is no
  runtime deploy surface to roll back. (Per the fix-forward + post-merge
  correction posture, a small later defect would be pushed directly with a
  note appended here, not reopened as a feature.)
- **CLAUDE.md not rewritten.** No mutation-strategy change; the existing
  per-feature strategy already fits.
- **No C4 / topology change** — identical before and after.

## environments.yaml summary

Two environments, neither a runtime environment:

- **`clean`** — local developer machine; runs `scripts/hooks/pre-commit`
  (`cargo test` workspace); mirrors the CI commit stage; deterministic;
  developer-blocking (escapable, CI authoritative).
- **`ci`** — GitHub Actions (`.github/workflows/ci.yml`); push + PR on
  main; runs `gate-1-test` (already workspace-scoped → runs the new
  tests with no edit) and the five touched-crate `gate-5` `--in-diff`
  jobs (green no-op on doc-only diffs; cover the two behaviour tests);
  feedback, not a merge-blocker.

No staging / production / canary / rollback environment exists or is
needed for a documentation-honesty feature.

## Peer review

`nw-platform-architect-reviewer` dispatch attempted (see Apex's run
report). If not separately invocable from this sub-agent context, a
structured self-review is recorded below and the wave is flagged for a
top-level reviewer run, WITH the nWave-order reminder above so a reviewer
does not reject on the (correct, expected) absence of the not-yet-written
guard tests, behaviour tests, and prose corrections.

### Self-review (platform-architect critique dimensions)

| Dimension | Assessment |
|---|---|
| Existing-infrastructure-first | PASS. No new component proposed; the five touched crates' gate-5 jobs and the workspace-scoped gate-1 are reused verbatim. New CI job count: 0. |
| Simplest-infrastructure-first | PASS. The simplest possible outcome — "the standing pipeline already covers it" — is the recommendation. No environment, orchestration, or deploy machinery added. |
| Rollback-first | PASS (degenerate). `git revert` is the rollback for a prose/guard change; no runtime surface exists. Stated explicitly. |
| Determinism / measurement coupling | PASS. All new tests are pure file-read or pure-function over fixed inputs; no wall-clock/p95; identical in local hook and CI. Explicitly contrasted with the known lumen/pulse p95 flake. |
| SLO-driven / observability | N/A by construction — zero behaviour change, no new runtime signal to observe. The outcome KPIs (overstatement counts) are measured by the guard tests themselves, which gate-1 runs. Confirmed not a gap. |
| Mutation strategy fit | PASS. Per-feature 100% on modified production files; doc-only slices have none; behaviour slices add tests not production code. No strategy change, no CLAUDE.md edit. |
| Completeness of handoff to DISTILL | PASS. The one actionable for DISTILL (README-path portability via CARGO_MANIFEST_DIR, single dedicated docs-guard test) is stated with the exact in-tree idiom to copy. |

**Self-review verdict**: no critical / high issues. Single watch-item is
the README-path portability note (decision 3), already raised as the
explicit DISTILL actionable. Approved to hand to DISTILL (by a separate
wave; NOT performed here — this wave does not proceed into DISTILL).
