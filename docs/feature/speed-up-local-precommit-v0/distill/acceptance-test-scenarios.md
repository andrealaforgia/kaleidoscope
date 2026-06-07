# Acceptance Test Scenarios — speed-up-local-precommit-v0 (DISTILL)

> **Author**: `nw-acceptance-designer` (Quinn), 2026-06-07.
> **Test file**: `crates/integration-suite/tests/v0_fast_precommit_structure.rs`
> **Cargo**: `[[test]] v0_fast_precommit_structure` in
> `crates/integration-suite/Cargo.toml`.
> **Acceptance kind**: STRUCTURAL — assertions on committed shell scripts +
> the CI workflow + the on-disk test inventory (mirrors the perf-kpi
> structural precedent, ADR-0070). No service, no port, no spawned process.

## Structural-acceptance note (why no Gherkin .feature file)

This is an infrastructure feature whose deliverable is committed TEXT (a hook
edit, a new shell script, an unchanged CI gate). There is no runtime user
journey to express as Given-When-Then through a driving port; the "driving
surface" is the script a maintainer invokes (`git commit` -> the hook; the
maintainer -> `scripts/ci-watch.sh`). Per the precedent set by
`v0_perf_kpi_ci_non_gating_structure.rs`, the acceptance is encoded directly
as Rust `#[test]` assertions over the committed files. The scenarios below
are the business-language statement of each assertion; the Rust functions are
their executable form.

## Scenario 1 (US-01) — RED `#[ignore]` — the local commit gate runs the fast subset

```gherkin
@walking_skeleton @us-01 @structural @ignore
Scenario: The local pre-commit hook's test step runs the fast unit subset, not the deep suite
  Given the committed scripts/hooks/pre-commit
  When its test step invocation is read
  Then the invocation runs "cargo test --workspace --lib"
  And no invocation runs "cargo test --workspace --all-targets" locally
```

- **Test fn**: `local_hook_test_step_runs_the_fast_lib_subset_not_all_targets`
- **AC**: US-01 "the hook's test step does NOT execute the fsync-heavy
  durability/snapshot/torn-tail/subprocess test binaries"; ADR-0072 D1.
- **Why RED today**: hook Step 4 (pre-commit:92-93) is still `--all-targets`.
- **Scoping**: asserts on CODE lines only (comments stripped) so the
  `# Covers:` header mention of `--all-targets` cannot false-pass/fail.

## Scenario 2 (US-04) — RED `#[ignore]` — the CI-results watcher exists

```gherkin
@us-04 @structural @ignore
Scenario: A one-command CI-results watcher is available to keep eyes on the deep coverage
  Given the deep tests no longer block the local commit
  When the committed scripts/ci-watch.sh is read
  Then it fetches the latest main runs via "gh run list" on "--branch main"
  And it drills into a run via "gh run view" with "--log-failed"
```

- **Test fn**: `ci_watch_script_exists_and_wraps_gh_run_inspection`
- **AC**: US-04 "a concrete, low-friction mechanism reports the latest main CI
  run status"; ADR-0072 D3.
- **Why RED today**: `scripts/ci-watch.sh` is absent (verified
  `ls` -> No such file).
- **RED-not-BROKEN**: a missing file is a behavioural `panic!` ("must exist"),
  not an unwrapped IO error, so `--ignored` reports FAILED not ERROR.

## Scenario 3 (US-03) — GREEN control — the deep CI gate is preserved

```gherkin
@us-03 @structural @control
Scenario: The deep suite still gates in CI after the local hook is slimmed
  Given the .github/workflows/ci.yml gate-1-test job
  When its run invocation is read
  Then it still runs "cargo test --workspace --all-targets --locked"
```

- **Test fn**: `ci_gate_1_test_still_runs_the_deep_all_targets_gate`
- **AC**: US-03 "CI gate-1 runs on every push and is unchanged"; ADR-0072 D4.
- **Load-bearing negative control**: proves slimming the LOCAL hook does not
  de-gate CI. Scoped to the `gate-1-test` job block (ci.yml has the same
  invocation in the non-gating `perf-kpis` job at 290/299 — job-scoping stops
  a false-pass).
- **GREEN today AND after** DELIVER (DELIVER does not touch ci.yml).

## Scenario 4 (US-03) — GREEN control — no test binary is deleted

```gherkin
@us-03 @structural @control
Scenario: De-gating the local hook does not delete any test
  Given the crates/**/tests test sources on disk
  When they are counted
  Then the count is at or above the no-deletion threshold (170; baseline 173/174)
  And the known slow durability test crates/cinder/tests/v1_slice_01_wal_durability.rs still exists
```

- **Test fn**: `no_integration_test_binary_is_deleted_by_the_slim_down`
- **AC**: US-03 "no test file is deleted from any crate; CI is not weakened".
- **Glob reconciliation**: counts every `*.rs` under any `tests/` dir to match
  the EXACT DEVOPS `find -path '*/tests/*.rs'` semantics (173/174, incl. 8
  `tests/common/mod.rs` helpers). Threshold 170 + a specific durability-bin
  existence check backstops the count.
- **GREEN today AND after** DELIVER (DELIVER deletes no test).

## Story -> scenario coverage map

| US | AC | Scenario(s) |
|----|----|-------------|
| US-01 | fast subset, no fsync bins locally | Scenario 1 |
| US-02 | fast subset still catches unit/fmt/clippy | covered structurally by Scenario 1 (`--lib` runs unit tests); behavioural negative-control is Apex's DELIVER measurement pass (ADR-0072 §Verification) |
| US-03 | deep gate preserved in CI; no test deleted | Scenario 3, Scenario 4 |
| US-04 | CI-results-watching mechanism exists | Scenario 2 |

## Mandate-7 / falsifiable self-review checklist

- [x] RED scenarios (1, 2) FAIL behaviourally against today's tree, proven by
  the `--ignored` run (2 FAILED).
- [x] Controls (3, 4) GREEN today and remain GREEN after DELIVER.
- [x] Test COMPILES (RED, not BROKEN): reads existing files, no missing
  production symbol; script-absence is a behavioural panic, not an IO error.
- [x] Default `cargo test` GREEN (controls pass, RED `#[ignore]`d).
- [x] Assertions correctly scoped: Scenario 1 = invocation line not header
  comment; Scenario 3 = gate-1-test job block not whole-file/other-job.
- [x] Path resolution via `CARGO_MANIFEST_DIR` -> repo root (cwd-independent).
- [x] No Fixture Theater: the test reads the GENUINE committed files; deleting
  the real hook/script/workflow would red it (it is not testing a fixture
  copy of itself).
- [x] No production code / hook edit / `ci-watch.sh` written here (DELIVER).
- [x] fmt clean; clippy clean (`-D warnings`); fast (0.04s, no fsync, no
  subprocess).
