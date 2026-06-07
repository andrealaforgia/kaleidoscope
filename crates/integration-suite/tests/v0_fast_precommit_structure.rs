// Kaleidoscope integration suite — structural acceptance for
// speed-up-local-precommit-v0
// Copyright (C) 2026 The Kaleidoscope authors
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as
// published by the Free Software Foundation, either version 3 of the
// License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU
// Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public
// License along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Structural acceptance test for `speed-up-local-precommit-v0`
//! (ADR-0072).
//!
//! The acceptance for this feature is **structural**, exactly like its
//! sibling `v0_perf_kpi_ci_non_gating_structure.rs` (ADR-0070): the
//! observable outcome the maintainer wants lives in committed *shell
//! scripts* and a *CI workflow definition*, not in any runtime
//! behaviour of a crate. So this test reads the real
//! `scripts/hooks/pre-commit`, the real `scripts/ci-watch.sh`, the real
//! `.github/workflows/ci.yml`, and the on-disk `crates/**/tests/*.rs`
//! inventory, and asserts the shape ADR-0072 / the DEVOPS wave mandate.
//! There is no service to stand up, no port to drive, no process to
//! spawn — the committed scripts ARE the driving surface a maintainer
//! invokes (`git commit` runs the hook; the maintainer runs
//! `scripts/ci-watch.sh`).
//!
//! ## What "done" looks like (the contract, from ADR-0072 / DEVOPS)
//!
//! 1. (US-01) The local hook's Step-4 test invocation runs the FAST
//!    subset `cargo test --workspace --lib` and does NOT run
//!    `cargo test --workspace --all-targets` — so the 173 integration
//!    bins (26 of them fsync-bound durability bins, plus the subprocess
//!    bins) no longer run on the commit path. The 10-20 min Step 4
//!    becomes a unit-only run.
//! 2. (US-04) A `scripts/ci-watch.sh` watcher EXISTS and wraps the `gh`
//!    CLI (`gh run list --branch main` + `gh run view ... --log-failed`)
//!    so the deep coverage now off the local block still has eyes.
//! 3. (US-03; negative control) `.github/workflows/ci.yml`
//!    `gate-1-test` STILL runs the deep gate
//!    `cargo test --workspace --all-targets --locked` — slimming the
//!    LOCAL hook must provably NOT remove the deep CI gate.
//! 4. (US-03; negative control) No test is deleted: the
//!    `crates/**/tests/**/*.rs` test-source count stays at/above the
//!    live DEVOPS baseline (173/174; threshold 170), AND a known slow
//!    durability bin (`cinder/tests/v1_slice_01_wal_durability.rs`)
//!    still exists on disk. De-gating must not become deletion.
//!
//! ## nWave ordering — why #1 and #2 are `#[ignore]`d (RED)
//!
//! DISTILL runs BEFORE DELIVER. The hook Step-4 edit (swap
//! `--all-targets` -> `--lib`) and the new `scripts/ci-watch.sh` are the
//! DELIVER acts and do NOT exist yet. So assertions #1 and #2 are RED
//! against today's committed tree: today the hook's Step 4 IS
//! `cargo test --workspace --all-targets --locked` (the falsifier for
//! #1) and `scripts/ci-watch.sh` is ABSENT (the falsifier for #2). They
//! are tagged `#[ignore = "RED until DELIVER: ..."]` so `cargo test` is
//! GREEN on the current tree; `cargo test -- --ignored` shows them
//! FAILING, which is the falsifiability evidence. DELIVER removes the
//! `#[ignore]` when the hook edit + the script land.
//!
//! Assertions #3 and #4 are un-ignored GREEN controls: they pass BEFORE
//! and AFTER the DELIVER edit. #3 is the load-bearing negative control
//! (the deep CI gate must survive the slim-down untouched); #4 guards
//! that no test binary is deleted by the de-gating.
//!
//! ## Mandate 7 — RED, not BROKEN
//!
//! This test reads existing files and asserts on their content. No
//! production symbol is missing, so it COMPILES and links; the RED
//! assertions FAIL behaviourally (the hook is not yet `--lib`;
//! `ci-watch.sh` does not yet exist), they do not error on setup. That
//! is the correct RED shape. The script-absence RED (#2) is written so
//! that a MISSING file is a behavioural assertion failure (a `false`
//! assert with a clear message), NOT an unhandled I/O panic — so the
//! `--ignored` run reports a clean FAILED, not an ERROR.

use std::fs;
use std::path::PathBuf;

/// The repository root, resolved from this crate's manifest directory.
///
/// `CARGO_MANIFEST_DIR` is `<repo>/crates/integration-suite`; two
/// parents up is the repository root. Robust regardless of the caller's
/// working directory. (Mirrors the perf-kpi sibling's resolution.)
fn repo_root() -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    PathBuf::from(manifest_dir)
        .parent()
        .and_then(|p| p.parent())
        .expect("crate manifest dir has a grandparent (the repo root)")
        .to_path_buf()
}

/// Read the committed local pre-commit hook.
fn read_pre_commit_hook() -> String {
    let path = repo_root().join("scripts/hooks/pre-commit");
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("could not read {}: {e}", path.display()))
}

/// Read the committed CI workflow definition.
fn read_ci_workflow() -> String {
    let path = repo_root().join(".github/workflows/ci.yml");
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("could not read {}: {e}", path.display()))
}

/// Lines of the hook that are actual shell (not blank, not a `#` comment
/// line). This is how Scenario 1 scopes its assertion to the ACTUAL
/// invocation, so a `# Covers:` header-comment mention of `--all-targets`
/// (which the historical header carries) cannot false-fail the check.
fn hook_code_lines(hook: &str) -> Vec<&str> {
    hook.lines()
        .map(|l| l.trim_start())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect()
}

/// Extract the lines belonging to a single top-level job block from the
/// `ci.yml` text. Jobs are indented by exactly two spaces under `jobs:`.
/// A job block runs from its `  <name>:` header up to (but not
/// including) the next two-space-indented header or end of file. This
/// SCOPES an assertion to one job, so the `--all-targets` invocation in
/// a *different* job (the ADR-0070 `perf-kpis` job at ci.yml:290/299)
/// cannot false-pass a check meant for `gate-1-test`.
///
/// Returns `None` if the job is not found. (Lifted from the perf-kpi
/// sibling test; identical job-scoping need.)
fn job_block<'a>(workflow: &'a str, job_name: &str) -> Option<&'a str> {
    let header = format!("  {job_name}:");
    let lines: Vec<&str> = workflow.lines().collect();

    let start = lines.iter().position(|line| {
        *line == header.as_str()
            || (line.starts_with("  ")
                && !line.starts_with("   ")
                && line.trim_end() == header.trim_end())
    })?;

    let start_byte = byte_offset_of_line(workflow, start);

    let end_line = lines
        .iter()
        .enumerate()
        .skip(start + 1)
        .find(|(_, line)| {
            line.starts_with("  ")
                && !line.starts_with("   ")
                && !line.starts_with("  #")
                && line.trim_end().ends_with(':')
                && !line.trim_start().starts_with('#')
        })
        .map(|(idx, _)| idx);

    let end_byte = match end_line {
        Some(idx) => byte_offset_of_line(workflow, idx),
        None => workflow.len(),
    };

    Some(&workflow[start_byte..end_byte])
}

/// Byte offset at which line `line_index` (0-based) begins in `text`.
fn byte_offset_of_line(text: &str, line_index: usize) -> usize {
    let mut offset = 0usize;
    for (i, line) in text.lines().enumerate() {
        if i == line_index {
            return offset;
        }
        offset += line.len() + 1;
    }
    offset
}

/// Count every `crates/**/tests/**/*.rs` file under the workspace.
///
/// This matches the EXACT glob the DEVOPS wave used to baseline the
/// no-tests-deleted invariant: `find crates -path '*/tests/*.rs'` (which
/// recurses to any depth under a `tests/` dir, so it also counts shared
/// `tests/common/mod.rs` helper modules — those are NOT independent
/// binaries, but they are still test *source* that must not vanish, and
/// matching the documented glob keeps this control's baseline reconciled
/// with the DEVOPS-recorded number rather than drifting on a subtly
/// different glob — exactly the 165-vs-173 reconciliation lesson DEVOPS
/// flagged). DEVOPS measured this at **173**; it is **174** at DISTILL
/// time (the inventory grows as features land). The threshold (170) sits
/// safely below both, so this control reds only on an actual deletion of
/// several test files, never on ordinary growth.
///
/// Walks the real filesystem from the repo root, NOT a hardcoded list,
/// so it tracks the live inventory by construction.
fn count_test_source_files() -> usize {
    fn walk(dir: &std::path::Path, under_tests: bool, count: &mut usize) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let is_tests = path.file_name().and_then(|n| n.to_str()) == Some("tests");
                walk(&path, under_tests || is_tests, count);
            } else if under_tests
                && path.is_file()
                && path.extension().and_then(|e| e.to_str()) == Some("rs")
            {
                *count += 1;
            }
        }
    }
    let mut count = 0usize;
    walk(&repo_root().join("crates"), false, &mut count);
    count
}

// =====================================================================
// Scenario 3 (US-03) — GREEN CONTROL, un-ignored.
//
// The deep CI gate is preserved: `gate-1-test` STILL runs
// `cargo test --workspace --all-targets --locked`. Passes against
// today's ci.yml AND must keep passing after the DELIVER hook edit (the
// edit slims the LOCAL hook; it must not touch ci.yml). The load-bearing
// negative control proving the local slim-down does not de-gate CI.
//
// Scoped to the gate-1-test job block so the perf-kpis job's own
// `--all-targets` invocation (ci.yml:290/299) cannot false-pass it.
// =====================================================================
#[test]
fn ci_gate_1_test_still_runs_the_deep_all_targets_gate() {
    let workflow = read_ci_workflow();
    let gate_1 = job_block(&workflow, "gate-1-test").expect("gate-1-test job must exist in ci.yml");

    assert!(
        gate_1.contains("cargo test --workspace --all-targets --locked"),
        "gate-1-test must still run the deep gate \
         `cargo test --workspace --all-targets --locked` (US-03). \
         Slimming the LOCAL pre-commit hook must NOT remove the deep CI \
         gate — CI is the single authoritative home for deep gating. \
         gate-1-test block was:\n{gate_1}"
    );
}

// =====================================================================
// Scenario 4 (US-03; no-tests-deleted guard) — GREEN CONTROL,
// un-ignored.
//
// De-gating locally must not become deleting. The integration-bin count
// stays at/above the live DEVOPS baseline (173 this wave; assert >= 170
// to tolerate ordinary churn), AND a known fsync-heavy durability bin
// still exists on disk. Passes today and must keep passing after
// DELIVER (DELIVER touches only the hook + a new script + docs; it
// deletes no test). If a future edit silently deletes durability bins
// to "speed things up", this control reds.
// =====================================================================
#[test]
fn no_integration_test_binary_is_deleted_by_the_slim_down() {
    let count = count_test_source_files();
    assert!(
        count >= 170,
        "crates/**/tests/**/*.rs source-file count ({count}) dropped \
         below the no-tests-deleted threshold (170; DEVOPS baseline 173, \
         174 at DISTILL). De-gating the local hook must NOT delete any \
         test source (US-03)."
    );

    let durability_bin = repo_root().join("crates/cinder/tests/v1_slice_01_wal_durability.rs");
    assert!(
        durability_bin.is_file(),
        "the known fsync-heavy durability bin {} must still exist: \
         the slim-down de-gates it LOCALLY, it must not delete it \
         (US-03). It still runs in CI gate-1.",
        durability_bin.display()
    );
}

// =====================================================================
// Scenario 1 (US-01) — RED until DELIVER, `#[ignore]`d.
//
// The local hook's Step-4 test invocation runs the FAST subset
// `cargo test --workspace --lib` and does NOT run
// `cargo test --workspace --all-targets`.
//
// FALSIFIABILITY: against TODAY's hook this FAILS — the Step-4 actual
// invocation is `cargo test --workspace --all-targets --locked`
// (pre-commit:92-93), so the "no --all-targets invocation" assertion
// fails and the "contains --lib" assertion fails. DELIVER swaps it to
// `--lib`; only then does this pass, and DELIVER removes the `#[ignore]`.
// Run `cargo test -p integration-suite --test
// v0_fast_precommit_structure -- --ignored` to see it fail today.
//
// SCOPING: the assertions look only at CODE lines (comments stripped),
// so the historical `# Covers: ... --all-targets ...` header comment
// neither false-passes nor false-fails. We assert (a) some code line
// contains `cargo test --workspace --lib`, and (b) NO code line
// contains a `cargo test --workspace --all-targets` invocation.
// =====================================================================
#[test]
#[ignore = "RED until DELIVER: hook Step 4 is still --all-targets; DELIVER swaps it to --lib"]
fn local_hook_test_step_runs_the_fast_lib_subset_not_all_targets() {
    let hook = read_pre_commit_hook();
    let code = hook_code_lines(&hook);

    let runs_lib_subset = code
        .iter()
        .any(|l| l.contains("cargo test --workspace --lib"));
    assert!(
        runs_lib_subset,
        "the pre-commit hook's test step must run the FAST subset \
         `cargo test --workspace --lib` (US-01 / ADR-0072 D1): it runs \
         every crate's unit tests but NONE of the 173 integration / 26 \
         fsync-bound durability / subprocess bins, so the commit gate \
         finishes fast. No hook CODE line contains it. \
         Hook code lines were:\n{code:#?}"
    );

    let runs_all_targets = code
        .iter()
        .any(|l| l.contains("cargo test --workspace --all-targets"));
    assert!(
        !runs_all_targets,
        "the pre-commit hook's test step must NOT run \
         `cargo test --workspace --all-targets` locally (US-01 / \
         ADR-0072 D1): the deep --all-targets run is what made Step 4 \
         take 10-20 min and now gates in CI instead. A hook CODE line \
         still invokes it. Hook code lines were:\n{code:#?}"
    );
}

// =====================================================================
// Scenario 2 (US-04) — RED until DELIVER, `#[ignore]`d.
//
// `scripts/ci-watch.sh` EXISTS and is the `gh`-based CI-results watcher:
// it references `gh run list`, `--branch main`, and
// `gh run view`/`--log-failed`, so the deep coverage now off the local
// block still has eyes.
//
// FALSIFIABILITY: against TODAY's tree this FAILS — `scripts/ci-watch.sh`
// is ABSENT (verified: `ls scripts/ci-watch.sh` -> No such file).
// DELIVER (Apex, platform-architect) writes it; only then does this
// pass, and DELIVER removes the `#[ignore]`. Run with `-- --ignored` to
// see it fail today.
//
// RED-NOT-BROKEN: the file read is `try`'d into a behavioural assert (a
// clear FAILED message), NOT an unwrapped panic — so the `--ignored` run
// shows a clean FAILED, not an ERROR.
// =====================================================================
#[test]
#[ignore = "RED until DELIVER: scripts/ci-watch.sh does not exist yet; DELIVER (Apex) writes it"]
fn ci_watch_script_exists_and_wraps_gh_run_inspection() {
    let path = repo_root().join("scripts/ci-watch.sh");

    let script = match fs::read_to_string(&path) {
        Ok(s) => s,
        Err(_) => panic!(
            "scripts/ci-watch.sh must exist (US-04 / ADR-0072 D3): the \
             one-command CI-results watcher that is the safety net for \
             the deep tests now off the local commit path. It is ABSENT \
             on today's tree (the expected DISTILL RED — DELIVER writes \
             it). Expected at: {}",
            path.display()
        ),
    };

    assert!(
        script.contains("gh run list"),
        "scripts/ci-watch.sh must call `gh run list` to fetch the \
         latest main runs (US-04 / ADR-0072 D3)."
    );
    assert!(
        script.contains("--branch main"),
        "scripts/ci-watch.sh must scope to `--branch main` — main's \
         health is what the cadence watches (US-04 / ADR-0072 D3)."
    );
    assert!(
        script.contains("gh run view"),
        "scripts/ci-watch.sh must call `gh run view` to drill into a \
         run (US-04 / ADR-0072 D3)."
    );
    assert!(
        script.contains("--log-failed"),
        "scripts/ci-watch.sh must use `--log-failed` so a gate-1 / \
         gate-5 red surfaces the failing job/step directly, not just \
         'a run failed' (US-04 / ADR-0072 D3)."
    );
}
