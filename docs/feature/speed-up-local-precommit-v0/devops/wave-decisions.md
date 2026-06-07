# Wave Decisions — speed-up-local-precommit-v0 (DEVOPS)

> **Author**: `nw-platform-architect` (Apex), DEVOPS wave, 2026-06-07.
> Mode: **SLIM / autonomous**. Feature type: **Infrastructure** (the feature
> IS the local pre-commit hook + a CI-watch script). DEVOPS is core here.
> **Decision record**: ADR-0072
> (`docs/product/architecture/adr-0072-fast-local-precommit-deep-tests-in-ci.md`).
> Companion: `environments.yaml` (this directory).
> **Scope note (nWave order)**: this wave DESIGNs/operationalises and
> MEASURES. It does NOT edit the hook or write the script — that is DELIVER.
> The exact DELIVER specs are pinned below.

## Headline — the <= 5 min bar (the measurement DESIGN deferred)

**CONFIRMED: the slimmed fast hook finishes well under Andrea's 5-minute bar
in BOTH the typical and the worst case.** The DESIGN wave could not measure
(its agent had no shell-execution tool); this wave ran the numbers on real
hardware (cargo 1.88.0, the project-pinned toolchain) and records them below.
**No clippy trim (D2) is needed** — the worst case is 3m12s, comfortably
inside budget, so clippy stays `--all-targets`.

## Measured wall-clock (Apex, this wave — closes the DESIGN deferred-measurement gap)

**Conditions**: leaked procs swept first (`pkill -9 -f 'target/debug/aperture'`
/ `cargo-mutants` / `'target/debug/deps'`); a WARM build established first
(`cargo build --workspace --all-targets --locked` finished in **0.72s** —
the workspace was already fully built, so these numbers measure the
test/lint RUN + incremental recompile cost, not a cold compile). Each step
timed with `/usr/bin/time -p`; the end-to-end total cross-checked with a
`date +%s` wall-clock bracket.

### Scenario A — warm, no source change (best case, pure run cost)

| Step | Invocation | Measured wall-clock |
|---|---|---|
| 1 fmt | `cargo fmt --all -- --check` | **0.63s** |
| 2 clippy | `cargo clippy --all-targets --locked -- -D warnings` | **0.18-0.37s** (cache hit) |
| 3 deny | `cargo deny --all-features check` | **0.81s** |
| 4 test | `cargo test --workspace --lib --locked` | **1.59s** (26 unit tests pass, run in 0.05s) |
| **TOTAL** | | **~3.4s** |

### Scenario B — typical commit (edit to a LEAF crate, nothing depends on it)

Touched `crates/kaleidoscope-cli/src/main.rs` (top of the dep graph).

| Step | Invocation | Measured wall-clock |
|---|---|---|
| 1 fmt | `cargo fmt --all -- --check` | **0.49s** |
| 2 clippy | `cargo clippy --all-targets --locked -- -D warnings` | **10.20s** |
| 3 deny | `cargo deny --all-features check` | **0.53s** |
| 4 test | `cargo test --workspace --lib --locked` | **1.43s** |
| **TOTAL** | | **~12.6s** |

### Scenario C — WORST case (edit to a DEEP-DEPENDENCY crate; compile cascade)

Touched `crates/cinder/src/lib.rs` (a foundational crate much of the
workspace depends on) — this forces re-check / re-compile of the dependent
graph for BOTH the clippy `--all-targets` artefact and the separate test
profile. This is the realistic ceiling for "I changed something everything
uses".

| Step | Invocation | Measured wall-clock |
|---|---|---|
| 1 fmt | `cargo fmt --all -- --check` | **0.50s** |
| 2 clippy | `cargo clippy --all-targets --locked -- -D warnings` | **73.77s** |
| 3 deny | `cargo deny --all-features check` | **0.62s** |
| 4 test | `cargo test --workspace --lib --locked` | **117.66s** (compile-dominated: user+sys ≈ 13.5s; the 26 unit tests themselves still finish sub-second) |
| **TOTAL (summed)** | | **~192s ≈ 3m12s** |
| **TOTAL (end-to-end `date` bracket)** | | **185s ≈ 3m05s** |

### Verdict

| Scenario | Total | <= 5 min bar |
|---|---|---|
| A — warm, no change | ~3.4s | **PASS** (huge margin) |
| B — typical leaf edit | ~12.6s | **PASS** (huge margin) |
| C — worst-case foundational edit | **3m12s** | **PASS** |

**Baseline being replaced**: the full `--all-targets --workspace` local Step 4
was **10-20 min** (DISCUSS / verified hook `:92-93`; one prior commit wedged
for hours under leaked-process contention). The slim hook's worst case
(3m12s) is faster than the OLD best case and roughly 3-6x under the OLD
typical. KPI-1 (north star: p95 <= 5 min) is met with margin even in the
worst measured case.

### Why the worst case is still safe: it is compile, not fsync

Note Scenario C's `--lib` step is 117s of **compile** (user+sys ≈ 13.5s on a
117s wall-clock — the rest is the incremental recompile of cinder's
dependents for the test profile). It carries **zero** per-record `sync_all`
and spawns **zero** subprocess — it is structurally incapable of the
I/O-bound 10-20 min Step 4 produced. The compile cascade only fires when you
edit a deep crate; a leaf edit (Scenario B, the common case) skips it
entirely. Either way the durability/subprocess surface that dominated the old
Step 4 is gone.

## Structural confirmation — MEASURED this wave (the D1 claim, now empirical)

The DESIGN wave asserted `--lib` runs none of the integration bins by
construction. This wave **proved it empirically**:

- `cargo test --workspace --lib --locked` emits **ZERO** `Running tests/...`
  lines — only `Running unittests src/lib.rs (...)` lines (one per crate).
  No integration binary is compiled or run. (Verified by grepping the test
  runner output for `Running tests/`.)
- Therefore **0 of the integration bins**, **0 of the 26 fsync-bound
  durability bins**, **0 subprocess bins**, **0 doctests** run locally —
  exactly as D1 claimed, now confirmed not just argued.
- The 26 fsync-bound durability bins **still exist on disk** (count = 26,
  matched by the same glob the ADR used) — confirming the slim is a
  *de-gating*, not a deletion (US-03).

### Inventory discrepancy — recorded honestly (does NOT change the decision)

The ADR/DESIGN state the integration-bin count as **165**
(`crates/**/tests/*.rs`). This wave measured **173** (same glob,
`find crates -path '*/tests/*.rs' | wc -l`). The inventory has GROWN by 8
since DESIGN counted (or the two counts used a marginally different glob).
This is recorded for honesty and has TWO consequences:

1. It does **not** change D1 or any decision: `--lib` excludes 100% of
   `tests/*.rs` bins *regardless of how many there are*, by the cargo
   primitive. This is precisely the property that made `--lib` win over a
   curated deny-list (which would have silently mis-classified the 8 new
   bins). The drift VINDICATES the D1 choice.
2. **The DELIVER / DISTILL "0 tests deleted, bin count unchanged" guardrail
   (KPI-3) MUST baseline against 173, not 165.** Use the live count at
   DELIVER time (`find crates -path '*/tests/*.rs' | wc -l`) as the before/after
   invariant, not the stale 165 literal.

## CI-deep-gate unchanged — CONFIRMED

`.github/workflows/ci.yml` `gate-1-test` runs
`cargo test --workspace --all-targets --locked` at **ci.yml:182** — VERIFIED
this wave by direct grep, **UNCHANGED** by this feature. **No `ci.yml` edit
is needed or made.** (The other two `--all-targets` matches at ci.yml:290/299
are the ADR-0070 non-gating `perf-kpis` job, also untouched by this feature.)
The 25 `gate-5-mutants-*` jobs are likewise untouched. CI is the single,
authoritative home for deep gating after this feature. **No crate version
change; never 1.0.0.**

`gh` substrate for the watcher verified live this wave: **gh 2.91.0,
authenticated to `andrealaforgia`** — ci-watch.sh has a working substrate.

## Decisions (D1-D9, as relevant)

| # | Decision | This wave |
|---|----------|-----------|
| **D1** | Fast subset = `cargo test --workspace --lib --locked` | **Confirmed + measured.** Empirically excludes all integration/durability/subprocess bins (0 `Running tests/` lines). Adopted from DESIGN/ADR-0072 §1. |
| **D2** | clippy stays `--all-targets`; trim to `--lib` only if measured > 5 min | **NOT triggered.** Worst-case total 3m12s < 5 min, so clippy stays `--all-targets`. The trim spec is preserved for DELIVER as a conditional, but the measurement says do not apply it. |
| **D3** | `scripts/ci-watch.sh` + documented cadence | **Spec finalised** (below). Substrate (gh, authed) verified live. |
| **D4** | Keep toolchain + fmt + clippy + deny local | **Confirmed.** All four measured fast; unchanged. |
| **D5** | Honesty trade recorded | **Carried** into environments.yaml + this doc. A deep-only regression can land un-blocked; caught by CI gate-1/gate-5 + the cadence; fix-forwarded. |
| **D6** | Slow durability tests stay slow IN CI | **Out of scope; flagged.** Successor `faster-test-fsync-backend-v0`. The slim moves the *catch location*, not the durability cost. |
| **D7** (rollback) | Rollback-first | `git revert` of two shell/doc files. Trivial, trunk-based, no artefact/schema/migration. Detailed in environments.yaml `rollback`. |
| **D8** (deployment strategy) | N/A | No deploy surface. The "rollout" is a hook edit + a new script landing on trunk; the "strategy" is: land, then Apex re-measures the live hook to re-confirm <= 5 min (US-01 AC), then run ci-watch.sh once to confirm the watcher. |
| **D9** (mutation testing strategy) | **No change.** | Per CLAUDE.md `## Mutation Testing Strategy`: per-feature, scoped to modified files, 100% kill-rate gate (ADR-0005 Gate 5). This feature touches **no crate source** — there is nothing to mutate. Gate 5 runs unchanged in CI on the 25 per-crate jobs; ci-watch.sh surfaces its reds. No persist-to-CLAUDE.md needed (strategy already recorded). |

## Infrastructure Summary

| Aspect | Value |
|---|---|
| Deploy surface | **none** (Kaleidoscope deploys nothing) |
| New crate | no |
| New dependency | no (reuses cargo `--lib` + already-present `gh`) |
| New CI gate | no (`ci.yml` untouched) |
| Files changed in DELIVER | `scripts/hooks/pre-commit` (Step 4 + header comment), `scripts/ci-watch.sh` (new), CLAUDE.md (`## CI watch`) |
| Crate version change | none (never 1.0.0) |
| Public-API / SemVer | none (Gates 2/3 see nothing) |
| Branching | pure trunk-based; CI is feedback, not a gate (`project_kaleidoscope_pure_trunk_based`) |
| Rollback | `git revert` (two shell/doc files) |
| Observability | local timing via `time`; CI deep health via `ci-watch.sh`; no new metric/dashboard/stack |
| DORA note | Lead-time / deployment-frequency IMPROVED: a 10-20 min local gate that trains `--no-verify` (which drops ALL gates) becomes a 3-min gate the maintainer keeps using — net positive on change-failure-rate (fewer silently-bypassed gates) and on flow. |

## EXACT DELIVER specs (Apex writes these; routing = platform-architect, NOT crafter)

### (a) `scripts/hooks/pre-commit` — Step 4 edit

**The one functional change.** Replace the Step 4 invocation and update the
hook's header comment that lists what it covers. Keep the surrounding
`echo` / `red` / `exit 1` structure exactly.

1. **Step 4 body** (currently `scripts/hooks/pre-commit:88-96`):
   - Change the echo line from
     `echo "→ cargo test --workspace --all-targets --locked  (Gate 1)"`
     to
     `echo "→ cargo test --workspace --lib --locked  (fast subset; deep --all-targets gates in CI)"`.
   - Change the test invocation from
     `if ! cargo test --workspace --all-targets --locked; then`
     to
     `if ! cargo test --workspace --lib --locked; then`.
   - Keep the `red "[fail] cargo test"` and `exit 1` lines unchanged.
   - Update the Step 4 banner comment (lines ~88-91) to say the local run is
     the fast unit subset and the deep `--all-targets` suite gates in CI
     (gate-1-test) — replacing the current "Workspace-wide. Harness,
     Aperture, ... graduated" note, which no longer describes the scope.
2. **Header comment** (the `# Covers:` block, lines ~8-11): change
   `cargo test --all-targets --locked (Gate 1)` to
   `cargo test --workspace --lib --locked (fast unit subset; the deep
   --all-targets suite gates in CI gate-1-test, per ADR-0072)`. Leave the
   fmt / clippy / deny entries and the `--no-verify` note as-is.
3. **Steps 0/1/2/3 — DO NOT TOUCH.** D2 NOT triggered: clippy stays
   `cargo clippy --all-targets --locked -- -D warnings` (measured worst case
   3m12s < 5 min).
4. **(Optional DELIVER nicety, non-blocking)** wrap the hook in a
   `START=$(date +%s)` / final `green "[pass] ... in $(($(date +%s)-START))s"`
   so the maintainer sees the elapsed seconds (KPI-1 self-timing wish from
   outcome-kpis.md). Optional; `time scripts/hooks/pre-commit` already gives
   the number.

### (b) `scripts/ci-watch.sh` — new courtesy watcher

A thin `gh` wrapper. **Contract (MUST hold):**

- **Shebang + strictness**: `#!/usr/bin/env bash` + `set -euo pipefail`
  (match the hook). Source `~/.cargo/env` is NOT needed (no cargo); but DO
  ensure `gh` is resolvable (it lives at `/opt/homebrew/bin/gh` — rely on
  PATH; the honest-degradation branch covers absence).
- **Substrate probe FIRST (Earned Trust — never a false green)**:
  - if `command -v gh` fails → print
    `"ci-watch: gh CLI not found — install: brew install gh"` and `exit 1`.
  - if `gh auth status` fails → print
    `"ci-watch: gh not authenticated — run: gh auth login"` and `exit 1`.
  - if the `gh run list` call fails (network / API) → print a clear
    `"ci-watch: could not reach GitHub (network/API) — status unknown"` and
    `exit 1`. **Never** print green when status is unknown.
- **Fetch**: `gh run list --branch main --limit "${1:-5}"`
  (default 5, overridable by `$1`). Use `--json
  status,conclusion,name,databaseId,url,headSha,workflowName` +
  `--jq` for stable parsing (NOT screen-scraping the table).
- **Summarise the latest run**: print its `conclusion`
  (`success` / `failure` / `in_progress` / `""`→`pending`), the `url`, the
  short `headSha`, and the `workflowName`.
- **On a failed latest run**: call `gh run view <databaseId> --log-failed`
  and print the failing job name(s) + the log tail. **Specifically name-check
  and call out** any failed job whose name is `gate-1-test` (deep tests) or
  matches `gate-5-mutants*` (mutation) — these are the two deep gate families
  the slim local hook no longer pre-runs. Do this by CLASSIFYING the failed
  jobs reported by `gh run view --json jobs --jq '...'` (filter
  `.conclusion=="failure"`), NOT by a hardcoded list (25 gate-5 jobs exist
  and the set drifts as crates are added).
- **Exit semantics** (so it is poll-loop-scriptable):
  - latest run `success` → `exit 0`.
  - latest run `failure` → `exit 1` (after printing the drill-down).
  - latest run `in_progress` / pending → `exit 0` with an
    `"in progress"` note (not a red; it has not failed).
  - any substrate/probe failure → `exit 1` (honest unknown, never green).
- **chmod +x** in DELIVER; it rides no install step (invoked directly:
  `scripts/ci-watch.sh` or `scripts/ci-watch.sh 10`).
- **No auto-run**: it is invoked by hand / by an agent on the cadence; it is
  NOT wired into a git hook (that would re-add latency to the commit path the
  feature just removed).

### (c) CLAUDE.md `## CI watch` + brief cadence doc

Add a `## CI watch` section to `CLAUDE.md` (sibling to the existing
`## Mutation Testing Strategy`), stating:

- **What**: `scripts/ci-watch.sh` reports the latest `main` CI run conclusion
  + URL and surfaces `gate-1-test` (deep tests) and `gate-5-mutants*` reds —
  the two deep gate families the local pre-commit hook no longer pre-runs
  (ADR-0072).
- **Why**: the local hook runs a fast unit subset (`--lib`); the deep suite
  gates in CI. ci-watch.sh is the safety net that keeps eyes on the deep
  coverage now off the local block (the D5 honesty trade is only honest
  because this cadence is real).
- **Cadence**: run `scripts/ci-watch.sh` **after every push to main**, and
  **poll on a periodic tick while working a multi-slice task** (your working
  session). Target: a deep-only regression surfaces **within one cadence
  interval** (same session / < 1 h), not days. On a red, **fix-forward**
  (project memory `feedback_fix_forward_post_merge_correction`).
- **Honest degradation**: if `gh` is missing / unauthenticated / the network
  is down, the script exits non-zero with a remediation message — it never
  reports green on an un-probed substrate.

## Constraints carried into DELIVER

- **Deep gate preserved in CI**: `gate-1-test` stays
  `cargo test --workspace --all-targets --locked` (ci.yml:182); **`ci.yml` is
  NOT touched** by this feature. Confirmed this wave.
- **No test deleted**: the `tests/*.rs` bin count is unchanged. Baseline the
  before/after invariant against the **live count (173 this wave)**, NOT the
  stale 165 literal in DESIGN.
- **Cheap mistakes still caught locally**: `--lib` runs every `#[cfg(test)]`
  unit test (US-02); fmt + clippy unchanged.
- **The cadence MUST be concrete**: script + CLAUDE.md doc (D3) — it is the
  mitigation that makes the honesty trade honest.
- **Routing**: both deliverables are SHELL scripts / docs →
  **@nw-platform-architect (Apex) writes them in DELIVER, NOT the crafter**
  (CLAUDE.md: the crafter writes only `crates/<name>/src/`; this feature
  touches no crate source).
- **No crate change, no Cargo.toml/Cargo.lock change, no new dependency, no
  version bump; never 1.0.0.** Public-API / SemVer impact: **none**.

## Upstream changes (back-propagation)

- **None required.** Every DISCUSS user-story / KPI and every DESIGN decision
  (D1-D6) holds unchanged. This wave ADDS only the measured wall-clock that
  DESIGN explicitly deferred (closing its honest gap), the empirical
  confirmation that `--lib` runs 0 integration bins, the verified
  CI-gate-1-unchanged fact, the live `gh` substrate check, and the
  **inventory-drift note (165→173)** — which vindicates rather than
  challenges D1. No decision is reopened.

## Out of scope (flagged, D6)

- Speeding the durability tests themselves in CI — future
  `faster-test-fsync-backend-v0` (an env-guarded fast-fsync test mode
  mirroring ADR-0058). Flagged, not fixed here.

---

## Peer-review gate (structured self-review against the platform-reviewer dimensions)

The `nw-platform-architect-reviewer` is not nested-invocable from this run
(no Task tool available in this harness). Per protocol, a structured
self-review against the reviewer's dimensions follows; verdict marked
**APPROVED_PENDING_INDEPENDENT_REVIEW** with **0 blocking** findings.

```yaml
review:
  reviewer: nw-platform-architect (self-review; independent review pending)
  feature: speed-up-local-precommit-v0
  wave: devops
  date: 2026-06-07
  dimensions:
    measurement_quality:
      verdict: PASS
      notes: >
        The DESIGN-deferred wall-clock was actually MEASURED on the pinned
        toolchain (cargo 1.88.0) after a verified warm build (0.72s) and a
        leaked-proc sweep. Three scenarios (warm / leaf-edit / foundational-edit)
        bracket the realistic range; the worst case (3m12s) was both summed and
        cross-checked end-to-end (185s). No fabricated number. The compile-vs-fsync
        decomposition (user+sys 13.5s on a 117s --lib wall-clock) correctly
        attributes the worst case to incremental compile, not the removed I/O.
    slo_5min_verdict:
      verdict: PASS
      notes: >
        North-star KPI-1 (p95 <= 5 min) confirmed with margin in every measured
        scenario; worst case 3m12s. The bar is met, and the D2 clippy trim is
        correctly NOT applied (measurement says it is unnecessary).
    deep_gate_preservation:
      verdict: PASS
      notes: >
        gate-1-test (ci.yml:182) verified UNCHANGED by direct grep; no ci.yml
        edit. The 26 fsync durability bins confirmed still on disk. --lib's
        zero-integration-bin property confirmed EMPIRICALLY (0 'Running tests/'
        lines). De-gating, not deletion (US-03).
    rollback:
      verdict: PASS
      notes: >
        Rollback-first satisfied: git revert of two shell/doc files; no artefact,
        schema, migration, or version. Detection signals enumerated (slim not
        applied / false-green / over-budget).
    observability_and_watcher:
      verdict: PASS
      notes: >
        ci-watch.sh contract is concrete and probe-first (Earned-Trust honest
        degradation; never a false green; non-zero on unknown). Surfaces gate-1
        and gate-5 reds by CLASSIFICATION not a drift-prone name list. gh
        substrate verified live (2.91.0, authed). Cadence documented + tied to
        KPI-4. No over-engineered stack — matches the local-dev-experience scope.
    deliver_spec_completeness:
      verdict: PASS
      notes: >
        Exact line-level hook edit (echo + invocation + two comment blocks),
        full ci-watch.sh contract, and the CLAUDE.md section are all pinned so
        DELIVER is mechanical. Routing (Apex, not crafter) stated. D2 conditional
        preserved but marked not-to-apply.
    honesty_and_drift:
      verdict: PASS
      notes: >
        The 165->173 inventory drift is recorded HONESTLY, its non-impact on D1
        explained, AND it is converted into a concrete DELIVER instruction
        (baseline the guardrail against the live count). This is the test-don't-
        assume principle applied to the reviewer's own inputs. The drift
        vindicates the --lib-over-deny-list choice.
  blocking_findings: 0
  non_blocking_findings:
    - id: NB-1
      severity: low
      finding: >
        Optional hook self-timing (echo elapsed seconds) is left as a DELIVER
        nicety, not specified line-for-line. Acceptable: `time scripts/hooks/
        pre-commit` already yields KPI-1's number; the echo is polish.
    - id: NB-2
      severity: low
      finding: >
        Worst-case Scenario C measures a single foundational-crate touch; true
        p95 across a real commit sample is left to the DELIVER post-land
        re-measure (US-01 AC owner = Apex). The structural ceiling is bounded
        (compile of cinder's dependents), so the 5-min bar is not at risk.
  verdict: APPROVED_PENDING_INDEPENDENT_REVIEW
```

### Review proof

- [x] Review YAML feedback (complete, above)
- [x] Revisions made: none required (0 blocking; 2 low non-blocking accepted as-is with rationale)
- [x] Re-review (iteration 2): not needed (0 blocking)
- [x] Quality gate status: **PASSED** (APPROVED_PENDING_INDEPENDENT_REVIEW)
