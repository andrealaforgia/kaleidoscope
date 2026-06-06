# DESIGN Decisions: perf-kpi-ci-non-gating-v0

Author: `nw-solution-architect` (Morgan), DESIGN wave, 2026-06-06.
Mode: PROPOSE (autonomous). Paradigm: Rust idiomatic (set; not re-asked).

Primary deliverable: `docs/product/architecture/adr-0070-perf-kpi-non-gating-ci.md`
(supersedes ADR-0058 §3). Brief section: `## Application Architecture —
perf-kpi-ci-non-gating-v0` in `docs/product/architecture/brief.md`.

## The decision in one line

Remove `KALEIDOSCOPE_PERF_TESTS` from the gating `gate-1-test` job (the 28
perf tests then self-skip there via the ADR-0058 guard); add a separate
`perf-kpis` job that sets the variable, runs the family, and is
`continue-on-error: true` (visible-but-non-blocking); document the durable-op
budgets as dev-indicative; supersede ADR-0058's gating clause. No code, no
threshold, no test deletion, no durability change, no crate bump.

## Flags resolved (F1-F6)

### F1 — Exact CI restructure (the core) — RESOLVED

- **Gate-1 change**: delete the job-level `env:` block and its single
  `KALEIDOSCOPE_PERF_TESTS: "1"` entry from `gate-1-test`
  (`.github/workflows/ci.yml:140-141`). With the var absent, all 28 tests hit
  the ADR-0058 early-return preamble and self-skip; `gate-1-test` goes green iff
  the non-perf correctness suite passes. The gating invocation `cargo test
  --workspace --all-targets --locked` (`ci.yml:184`) is UNCHANGED.
- **New non-gating job**: add `perf-kpis` — sets `KALEIDOSCOPE_PERF_TESTS: "1"`
  in its own job-level `env` (hardcoded literal, per the ADR-0058 §3 GitHub
  Actions env-quirk note), runs the same `cargo test --workspace`, runs the
  whole family by env-var presence.
- **Non-gating mechanism**: **`continue-on-error: true` on the job**
  (RECOMMENDED and chosen). The job runs, a breach marks the job with a red X
  (visible), but the overall workflow conclusion stays success and nothing is
  blocked. Rejected: `|| true` on the step (hides the breach — step shows green
  even on a real breach, defeats C4); a separate workflow file (more moving
  parts; loses run-page co-location — deferred to DEVOPS as an allowed
  placement variant).
- **Trigger**: **every push / PR** (RECOMMENDED and chosen) so the signal is
  timely; nightly rejected (too coarse to spot a sustained regression; being
  non-gating already removes the batching cost argument).
- Backs US-01 + US-02.

### F2 — Supersede ADR-0058 — RESOLVED (YES, mandatory)

ADR-0070 SUPERSEDES ADR-0058 §3 (the CI-gating clause, quoted verbatim in
ADR-0070's Context). It PRESERVES ADR-0058's still-valid parts: the
presence-based env-guard mechanism (§1, §2, §4, §5, §6) and the
no-threshold-chasing stance. ADR-0070 records the durable-fsync root cause
ADR-0058 omitted (ADR-0049 §4 / ADR-0060 §3 added per-record `sync_all` AFTER
ADR-0058 was accepted), the trunk-based "CI is feedback, not a gate" posture
(memory `project_kaleidoscope_pure_trunk_based`), and the
train-the-team-to-ignore-red harm. Leaving ADR-0058 "Accepted" while
contradicting it in `ci.yml` would be an honesty violation. Backs US-04.

### F3 — Assert-but-swallow vs report-only — RESOLVED (keep the asserts)

Keep the tests **asserting**, in the non-gating job (assert-but-non-gating via
`continue-on-error`). The asserts ARE the KPI definition. Crucial empirical
finding: the existing assert message already prints the measured value on a
breach — `assert!(p95_us <= 200, "KPI 1: place p95 must be ≤ 200 µs; got
{p95_us} µs ...")` at `crates/cinder/tests/v1_slice_01_wal_durability.rs:282-285`.
So visibility-on-breach (C4) holds with **NO test change**; the only change is
WHERE the tests run (the new job's env). Report-only rejected (discards the
budget as a machine-checkable KPI; needs a 28-file edit).

- **Print-on-PASS**: OUT OF SCOPE / DEFERRED. Today the number prints only on
  breach; on a PASS the assert succeeds silently. Emitting the p95 on every run
  would require a 28-file `eprintln!` edit, contradicting D1 ("28 tests NOT
  modified") and C8. Recorded as a clean optional successor; not built here.
- Backs US-02.

### F4 — Honesty-note placement — RESOLVED (ADR-only)

The durable-op honesty note lives in ADR-0070 §6 only: (a) the durable-op
budgets reflect durable fsync cost since ADR-0049/0060; (b) they are
dev-indicative, not CI-contractual; (c) a CI breach is expected durable cost,
not a regression; (d) threshold-raising is explicitly NOT the fix (memory
`project_p95_wallclock_flakes_overnight`); (e) the caveat applies to
fsync-bearing ops (`place`, `enqueue`, WAL `ingest`), NOT to in-memory ops
(`get_tier` 50 us, `observe` 10/20 us, in-mem `enqueue_and_dequeue` 50 us)
whose budgets stay meaningful on CI. Per-site comment rejected (would touch ~7
of the 28 files for docs only, against D1/C8). Backs US-04.

### F5 — Scope is the whole family — CONFIRMED

The non-gating job runs all 28 tests across 11 crates via env-var presence; a
future guarded test is picked up for free (inventory-drift mitigation). One env
key removed from `gate-1-test` de-gates all 28 at once. Backs US-02 / C5.

### F6 — Local hook: no change — CONFIRMED

`scripts/hooks/pre-commit:92-93` runs `cargo test --workspace --all-targets
--locked` with NO `KALEIDOSCOPE_PERF_TESTS` (re-verified this wave). Perf tests
already self-skip locally per ADR-0058. NO change to the hook; the variable MUST
NOT be added to it. Backs C6.

## MANDATORY Reuse Analysis

| Capability needed | Existing asset | Verdict | Justification |
|---|---|---|---|
| Make perf tests skip when not wanted | ADR-0058 presence-based env guard, byte-identical at all 28 sites (`crates/cinder/tests/...:256`) | **REUSE (unchanged)** | The guard is the lever; remove the var from Gate 1 → all 28 self-skip there; set it in the new job → all 28 run. No new mechanism, no guard edit. |
| Print the p95 on breach (C4) | Existing assert message embedding the got-value (`crates/cinder/tests/...:282-285`) | **REUSE (unchanged)** | Number already prints on a panic; no report-only mode, no `eprintln!` edit. Lets F3 resolve to "keep asserts" with zero test change. |
| Run a CI job whose failure does not block | GitHub Actions `continue-on-error: true` job primitive | **REUSE (GitHub primitive)** | Native, visible-but-non-blocking; no bespoke `\|\| true`, no separate workflow, no third-party action. |
| Hardcode the env literal at job level | The `NIGHTLY_PIN` job-level literal pattern (ADR-0058 §3; gates 2/3) | **REUSE (pattern)** | New job sets `KALEIDOSCOPE_PERF_TESTS: "1"` as a hardcoded literal in its own job-level `env`, same shape the workspace already relies on. |
| The workflow file to edit | `.github/workflows/ci.yml` (existing) | **EXTEND** | Remove the env block from `gate-1-test`; add one new job. Extend the existing file; no parallel workflow. |
| The non-gating perf job itself | none (perf rode inside `gate-1-test`) | **CREATE (only new asset)** | No existing non-gating job sets the variable; de-gating removes perf from Gate 1, so something must still run the family (C4). Minimal new asset, composed of reused primitives. |
| Document the durable-op honesty note | none (ADR-0058 rejected threshold-raises but not the durable-cost reason) | **CREATE (ADR-0070)** | The reason did not exist when ADR-0058 was written (ADR-0049/0060 landed later). ADR-0070 is the citable home. |

**Reuse verdict**: EXTEND `ci.yml`; REUSE the self-skip guard, the
assert-message visibility, the `continue-on-error` primitive, and the
job-level-literal pattern; CREATE only the single `perf-kpis` job and ADR-0070.
No code, no test-body, no crate version touched.

## Supersede-ADR-0058 record

- ADR-0070 supersedes **ADR-0058 §3** (the CI-gating clause) and its gating
  consequence. It quotes the superseded clause verbatim in its Context.
- ADR-0058 PRESERVED parts: the presence-based guard mechanism (§1/§2/§4/§5/§6)
  and the no-threshold-chasing stance.
- ADR-0070 header: `Supersedes: ADR-0058 §3`. ADR-0058 remains immutable
  (superseded, never edited); a cross-reference is added to ADR-0058 only at a
  future revision per the repository immutability rule. ADR-0070 is the next
  free number (highest existing 0069, verified via `ls
  docs/product/architecture/adr-*.md`).

## Test seam

- **The seam is the CI env, not a code seam.** The single lever is the presence
  of `KALEIDOSCOPE_PERF_TESTS` in a job's environment. Removing it from
  `gate-1-test` and setting it in `perf-kpis` is the entire behavioural change.
- **Visibility seam (reused)**: the assert message's `got {p95_us} µs` text
  (`crates/cinder/tests/...:282-285`) is the existing seam that surfaces the
  number on breach — no new seam needed.
- **Acceptance is structural** (assert on the workflow YAML) plus one
  behavioural negative control (a real correctness break still reds Gate 1 —
  US-03; C2). Detailed in the brief's "For Acceptance Designer" note.
- **Root-cause citation**: `cinder.place` per-record fsync at
  `crates/cinder/src/file_backed.rs:433` (`fsync_backend.fsync_file`), comment
  citing ADR-0049 §4 / ADR-0060 §3 — the durable cost the 200 us budget
  predates.

## Constraints (all hold)

- **C1 Durability not weakened** — no `sync_all` removed; the fsync stays; the
  durable budgets reflect durable cost (honesty note, not a regression).
- **C2 Correctness gating not loosened** — `gate-1-test` still runs `cargo test
  --workspace --all-targets --locked` (`ci.yml:184`, unchanged); every non-perf
  test still executes and asserts; US-03 negative control demonstrates a real
  break still reds Gate 1.
- **C3 No threshold chasing** — no budget literal, sample count, warm-up loop,
  or percentile index changed.
- **C4 Visibility preserved** — the non-gating `perf-kpis` job runs the family
  and the assert message prints the p95 on breach; breach = non-blocking red X.
- **C5 Whole family** — env-var presence runs all 28 tests / 11 crates.
- **C6 Local hook already correct** — no change; var not added.
- **C7 Trunk-based posture** — non-gating job aligns with "CI is feedback, not
  a gate".
- **C8 No crate version impact** — CI + docs only; no `crates/*/src` change, no
  test-body change, no `Cargo.toml`/`Cargo.lock` change, no SemVer/public-api
  surface change; never 1.0.0.
- **C9 British English; em dashes structural only** — honoured.

## Upstream Changes

- No DISCOVER / DIVERGE artefacts. The relevant upstream is the predecessor
  `perf-kpi-ci-gating-v0` and ADR-0058, which this feature corrects (D6, F2),
  plus ADR-0049 / ADR-0060 (the durability features whose per-record fsync is
  the root cause ADR-0058 did not foresee).
- No back-propagation to DISCUSS required: the DISCUSS facts (28-test
  inventory, the `ci.yml:140-141` env, the `ci.yml:184` gating invocation, the
  `:256` guard, the `:282-285` assert message, the `:92-93` hook, the `:433`
  fsync) were all re-verified in code this wave and hold.

## Peer-review verdict

solution-architect-reviewer (Atlas) applied via the critique-dimensions skill
(nested invocation unavailable in this context). See the review YAML in the
DESIGN report. Verdict: **APPROVED** (iteration 1; 0 critical, 0 high after the
nWave-order note that the absent `ci.yml` change is EXPECTED at DESIGN, not a
rejection reason).
