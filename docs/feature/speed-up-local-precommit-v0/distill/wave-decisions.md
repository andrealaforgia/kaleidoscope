# Wave Decisions — speed-up-local-precommit-v0 (DISTILL)

> **Author**: `nw-acceptance-designer` (Quinn), DISTILL wave, 2026-06-07.
> Mode: **autonomous**. Feature type: **Infrastructure** — the feature IS a
> local pre-commit hook edit + a new CI-watch shell script. There is no crate
> service, no driving port in the application sense; the acceptance is
> **STRUCTURAL** (assert on the committed scripts + the CI workflow), exactly
> like the perf-kpi structural precedent.
> **Inputs**: ADR-0072, DEVOPS `wave-decisions.md`, DESIGN/DISCUSS
> `user-stories.md` (US-01..US-04). **Sibling precedent**:
> `crates/integration-suite/tests/v0_perf_kpi_ci_non_gating_structure.rs`
> (ADR-0070) — mirrored for resolution, scoping, `#[ignore]`-RED + control
> structure.
> **Scope note (nWave order)**: DISTILL writes the failing acceptance test
> (outer loop). It does NOT edit the hook or write `ci-watch.sh` — that is
> DELIVER (Apex, platform-architect).

## The walking-skeleton strategy: STRUCTURAL ASSERTION on the committed scripts

This feature has no runtime user journey to drive through a port. The
observable outcome the maintainer (Devon) wants lives entirely in committed
text:

- `scripts/hooks/pre-commit` — what `git commit` runs.
- `scripts/ci-watch.sh` — what the maintainer runs to watch CI.
- `.github/workflows/ci.yml` `gate-1-test` — the deep gate that must survive.
- the `crates/**/tests/*.rs` inventory — the tests that must not be deleted.

So the walking skeleton is a **structural acceptance test** that reads those
real files and asserts the post-DELIVER shape. This is the same WS strategy
ADR-0070 used (`v0_perf_kpi_ci_non_gating_structure.rs`): the committed
script/workflow file IS the driving surface — `git commit` invokes the hook,
the maintainer invokes `ci-watch.sh`. There is no InMemory adapter, no
spawned process, no service; the test reads real on-disk files via
`std::fs`, no new dependency. **WS strategy declared here per
critique-dimension 9a.** Strategy classification: this is the structural
analogue of "real I/O" — it reads the genuine committed artefacts, not a
fixture copy, so deleting the real script would red the test (dimension 9d
litmus satisfied for the script-existence scenario).

## The four scenarios (test-fn -> US/AC map)

| # | Test fn | US / AC | Kind | Today |
|---|---------|---------|------|-------|
| 1 | `local_hook_test_step_runs_the_fast_lib_subset_not_all_targets` | US-01 (`the hook's test step does NOT run the fsync-heavy bins`; ADR-0072 D1) | **RED** `#[ignore]` | hook Step 4 is `--all-targets` -> FAILS |
| 2 | `ci_watch_script_exists_and_wraps_gh_run_inspection` | US-04 (`a CI-results-watching mechanism is established`; ADR-0072 D3) | **RED** `#[ignore]` | `scripts/ci-watch.sh` absent -> FAILS |
| 3 | `ci_gate_1_test_still_runs_the_deep_all_targets_gate` | US-03 (`the deep suite still runs in CI`; ADR-0072 D4) | GREEN control | ci.yml:182 already deep -> PASSES |
| 4 | `no_integration_test_binary_is_deleted_by_the_slim_down` | US-03 (`no test file is deleted`) | GREEN control | 174 test files on disk -> PASSES |

US-02 (the fast subset still catches unit/fmt/clippy mistakes) is intentionally
NOT given a structural assertion here: it is a **negative-control behaviour**
(inject a broken unit test / fmt drift / clippy lint, confirm the hook reds),
which ADR-0072 §Verification and DEVOPS assign to **Apex's DELIVER measurement
pass**, not to a static text assertion. A structural test cannot meaningfully
assert "a broken unit test reds the hook" without running the hook; asserting
"the Step-4 invocation contains `--lib`" (Scenario 1) is the structural proxy
that PROVES unit tests are still run (US-02's mechanism), so US-02's
structural content is covered by Scenario 1. This is recorded honestly rather
than fabricating a static assertion that does not test what US-02 means.

## Falsifiability note (the RED ones genuinely fail today)

Verified against the live tree this wave:

- **Scenario 1 falsifier**: `scripts/hooks/pre-commit:92-93` is
  `echo "→ cargo test --workspace --all-targets --locked  (Gate 1)"` /
  `if ! cargo test --workspace --all-targets --locked; then`. So the
  "contains a `cargo test --workspace --lib` code line" assertion fails AND
  the "no `cargo test --workspace --all-targets` code line" assertion fails.
  The `--ignored` run output shows the hook code lines including the live
  `--all-targets` invocation — proving it fails for the right (behavioural)
  reason, not a setup error.
- **Scenario 2 falsifier**: `ls scripts/ci-watch.sh` -> `No such file or
  directory`. The test's `fs::read_to_string` miss is converted to a
  behavioural `panic!` with a clear "must exist" message (NOT an unwrapped
  I/O error), so the `--ignored` run reports a clean FAILED, not an ERROR
  (Mandate-7 compliant).

Both RED scenarios will go GREEN only when DELIVER (a) swaps hook Step 4 to
`cargo test --workspace --lib --locked` and (b) writes `scripts/ci-watch.sh`
wrapping `gh run list --branch main` + `gh run view --log-failed`. DELIVER
removes the two `#[ignore]` lines at that point.

## Scoping precision (so a header comment cannot false-pass/fail)

- **Scenario 1** strips comment lines (`hook_code_lines`) before asserting, so
  the historical `# Covers: ... cargo test --all-targets --locked (Gate 1)`
  header comment (pre-commit:8-10) — which DELIVER also rewrites — neither
  false-passes nor false-fails. The assertion is on the ACTUAL shell
  invocation line only. (DEVOPS spec (a) rewrites both the echo+invocation
  AND the header/banner comments; this test deliberately ignores the comments
  and pins the invocation, the load-bearing behaviour.)
- **Scenario 3** reuses the perf-kpi sibling's `job_block` helper to scope the
  `--all-targets` assertion to the `gate-1-test` job ONLY. Critical: ci.yml
  has THREE `cargo test --workspace --all-targets --locked` matches
  (line 182 = gate-1-test; lines 290/299 = the ADR-0070 non-gating
  `perf-kpis` job). A naive whole-file `contains` would false-pass if DELIVER
  ever removed it from gate-1-test but left it in perf-kpis. Job-scoping
  prevents that.

## Reconciliation: the 173-vs-166 inventory glob (recorded honestly)

DEVOPS baselined "no test deleted" at **173** via
`find crates -path '*/tests/*.rs' | wc -l`. My first Rust counter walked only
`crates/*/tests/*.rs` (one level) and got **166** — failing the control.
Diagnosis: `find -path '*/tests/*.rs'` recurses to ANY depth, so it ALSO
counts **8** `crates/*/tests/common/mod.rs` shared-helper modules (which are
`mod` includes, not independent `[[test]]` binaries). 166 top-level bins + 8
nested `common/mod.rs` = 174 (DEVOPS measured 173; the inventory grew by 1
since their count — features land).

**Decision**: the control's `count_test_source_files` now recurses to match
the EXACT DEVOPS glob semantics (any `*.rs` under any `tests/` dir), so its
baseline is reconciled with the documented 173/174 rather than drifting on a
subtly different glob — which is precisely the 165-vs-173 reconciliation
lesson DEVOPS itself flagged. Threshold is **170** (safely below both 173 and
174; reds only on a real multi-file deletion, never on ordinary growth). The
control is ALSO backstopped by a specific known-slow durability bin assertion
(`cinder/tests/v1_slice_01_wal_durability.rs` must still exist) so even a
count that drifts cannot hide deletion of the durability surface that is the
whole point of US-03.

## Proven-RED + trunk-green evidence (this test binary only; fast, no fsync)

`cargo test -p integration-suite --test v0_fast_precommit_structure --locked`:

```
running 4 tests
test ci_gate_1_test_still_runs_the_deep_all_targets_gate ... ok
test ci_watch_script_exists_and_wraps_gh_run_inspection ... ignored, RED until DELIVER: ...
test local_hook_test_step_runs_the_fast_lib_subset_not_all_targets ... ignored, RED until DELIVER: ...
test no_integration_test_binary_is_deleted_by_the_slim_down ... ok
test result: ok. 2 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 0.04s
```

`... -- --ignored` (falsifiability):

```
failures:
    ci_watch_script_exists_and_wraps_gh_run_inspection
    local_hook_test_step_runs_the_fast_lib_subset_not_all_targets
test result: FAILED. 0 passed; 2 failed; 0 ignored; 0 measured; 2 filtered out; finished in 0.00s
```

Default `cargo test` = GREEN (controls pass, RED ignored). `--ignored` = the
2 RED FAILING. fmt clean (`cargo fmt -p integration-suite -- --check` exit 0);
clippy clean (`cargo clippy -p integration-suite --tests --locked -- -D
warnings` Finished, no warnings). The test is pure string/file parsing — it
spawns NO aperture/durability/subprocess test, pays NO `sync_all`, runs in
0.04s. The full `cargo test --workspace --all-targets` was deliberately NOT
run (avoid contention with any concurrent gate; only this binary was run).

## The test-home decision

The test lives at
`crates/integration-suite/tests/v0_fast_precommit_structure.rs` with a
`[[test]]` block in `crates/integration-suite/Cargo.toml`, ALONGSIDE the
perf-kpi structural sibling. Rationale: `integration-suite` is the existing
cross-cutting home for structural/cross-crate acceptance that belongs to no
single crate; the perf-kpi structural test (the direct precedent) already
lives there; the feature touches no crate `src`, so the test cannot live in a
feature crate. No new dependency added (`std::fs` only).

## Mandate-7 / falsifiable self-review checklist

- [x] Test COMPILES and links (reads existing files; no missing production
  symbol) — RED, not BROKEN.
- [x] RED scenarios FAIL behaviourally (assertion failure with a clear
  message), not on a setup/IO error — the script-absence miss is a
  behavioural `panic!`, not an unwrapped `unwrap`.
- [x] Each RED assertion was verified to FAIL against today's tree
  (falsifiers enumerated above).
- [x] Controls GREEN today AND structurally GREEN after DELIVER (DELIVER does
  not touch ci.yml gate-1; deletes no test).
- [x] RED scenarios `#[ignore = "RED until DELIVER: <reason>"]`; default
  `cargo test` GREEN; `--ignored` FAILING.
- [x] Assertions scoped to the ACTUAL invocation / the gate-1-test job (not a
  header comment, not a different job).
- [x] Path resolution via `CARGO_MANIFEST_DIR` -> repo root (cwd-independent);
  inventory walk matches the DEVOPS glob.
- [x] No production code / no `crates/*/src` / no hook / no `ci-watch.sh`
  written (that is DELIVER).

## Peer-review gate (critique-dimensions self-review; independent review pending)

The `nw-acceptance-designer-reviewer` (Sentinel) is not nested-invocable in
this harness (no Task tool). Per protocol, a structured self-review against
the reviewer's Dimensions 1-9 follows.

```yaml
review:
  reviewer: nw-acceptance-designer (self-review; independent review pending)
  feature: speed-up-local-precommit-v0
  wave: distill
  date: 2026-06-07
  dimensions:
    happy_path_bias: { verdict: PASS, note: "Structural acceptance; the suite IS balanced — 2 shape-positive (1,3) + 2 negative controls (2 watcher-must-exist, 4 no-deletion). The deep-only-regression error path is what Scenarios 2+4 protect." }
    gwt_format: { verdict: PASS, note: "Each scenario: Given committed artefact / When read / Then single observable assertion." }
    business_language: { verdict: PASS, note: "Domain terms (commit gate, deep suite, CI run, watcher). cargo invocation literals are the artefacts UNDER assertion, quoted — acceptable for structural acceptance per the perf-kpi precedent." }
    coverage_completeness: { verdict: PASS, note: "US-01/03/04 each have a scenario; US-02 structural content folded into Scenario 1 (--lib runs unit tests) with honest recorded rationale (behavioural part = Apex DELIVER)." }
    walking_skeleton_centricity: { verdict: PASS, note: "WS = structural assertion; titles describe the maintainer outcome, not layer wiring." }
    priority: { verdict: PASS, note: "Targets the measured bottleneck (10-20 min --all-targets Step 4); DEVOPS data-justified." }
    observable_behaviour: { verdict: PASS, note: "Asserts the content of the committed artefacts the maintainer reads/invokes — structural analogue of observable outcome; no private-state/mock-call assertions." }
    traceability: { verdict: PASS, note: "Every scenario @us-tagged; story->scenario map present. Environment mapping N/A (infrastructure feature, no runtime journeys)." }
    walking_skeleton_boundary_proof: { verdict: PASS, note: "9a strategy declared (structural). 9d litmus: deleting the real hook reds S1, deleting the real script reds S2, deleting tests reds S4 — genuine artefacts, not fixtures. Not Fixture Theater." }
  investigated_would_be_blocker:
    - finding: "Does Scenario 1's `no cargo test --workspace --all-targets` assertion go GREEN against the DELIVER end-state, which KEEPS the literal `--all-targets` in (a) the new Step-4 echo string `(... deep --all-targets gates in CI)`, (b) Step-2 clippy `cargo clippy --all-targets`, (c) the `# Covers:` header comment?"
      resolution: "RESOLVED — assertion pins the contiguous substring `cargo test --workspace --all-targets`. (a) the echo's only `--all-targets` is `deep --all-targets`, NOT preceded by `cargo test --workspace ` — no match; (b) is `cargo clippy --all-targets`, not `cargo test` — no match; (c) is a `#` comment, stripped by hook_code_lines. The assertion goes GREEN post-DELIVER. No change needed."
  blocking_findings: 0
  non_blocking_findings:
    - id: NB-1
      severity: low
      finding: "US-02's behavioural negative control (broken unit test reds the hook) is NOT a static assertion — it is recorded as Apex's DELIVER measurement responsibility. Acceptable: a structural test cannot run the hook; Scenario 1 is the structural proxy that proves --lib (the US-02 mechanism) is invoked."
    - id: NB-2
      severity: low
      finding: "The no-deletion threshold (170) is a band, not the exact 173/174 count, to tolerate growth. Backstopped by a specific durability-bin existence check so the durability surface cannot be silently deleted under the band."
  verdict: APPROVED_PENDING_INDEPENDENT_REVIEW
```

### Review proof

- [x] Review YAML (complete, above); Dimensions 1-9 all PASS.
- [x] One would-be-blocker (Scenario-1 substring scoping vs DELIVER end-state)
  investigated against the DEVOPS DELIVER spec and RESOLVED — no change needed.
- [x] Revisions made during the wave: the 173-vs-166 glob reconciliation
  (recursive counter) — applied, re-run GREEN.
- [x] Re-review (iteration 2): not needed (0 blocking after investigation).
- [x] Quality gate status: **PASSED** (APPROVED_PENDING_INDEPENDENT_REVIEW).
