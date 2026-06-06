// Kaleidoscope integration suite — structural acceptance for
// perf-kpi-ci-non-gating-v0
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

//! Structural acceptance test for `perf-kpi-ci-non-gating-v0`
//! (ADR-0070).
//!
//! The acceptance for this feature is **structural**: the observable
//! outcome the maintainer wants lives in the committed CI workflow
//! definition, not in any runtime behaviour. So this test reads the
//! real `.github/workflows/ci.yml` and asserts the shape ADR-0070
//! mandates. There is no service to stand up, no port to drive, no
//! process to spawn — the workflow file IS the driving surface a
//! maintainer reads on the GitHub Actions run page.
//!
//! ## What "done" looks like (the contract, from ADR-0070 / DEVOPS)
//!
//! 1. (US-01) The build-gating `gate-1-test` job does NOT set
//!    `KALEIDOSCOPE_PERF_TESTS`, so the 28 wall-clock KPI tests
//!    self-skip there and a perf breach can no longer red the gate.
//! 2. (US-02) A separate, NON-GATING `perf-kpis` job exists, sets
//!    `KALEIDOSCOPE_PERF_TESTS: "1"`, runs `cargo test --workspace`,
//!    and is `continue-on-error: true` so a breach is a visible red X
//!    that never blocks the workflow.
//! 3. (US-03; negative control) `gate-1-test` STILL runs the
//!    correctness-gating invocation `cargo test --workspace
//!    --all-targets --locked` — de-gating perf must provably NOT
//!    de-gate correctness.
//! 4. (US-04) ADR-0070 records the durable-op honesty note so a
//!    contributor reads a durable-op breach as expected fsync cost,
//!    not a regression, and does not threshold-chase.
//!
//! ## nWave ordering — why #1 and #2 are `#[ignore]`d (RED)
//!
//! DISTILL runs BEFORE DELIVER. The `ci.yml` edit (remove the perf env
//! from `gate-1-test`; add the `perf-kpis` job) is the DELIVER act and
//! does NOT exist yet. So assertions #1 and #2 are RED against today's
//! committed `ci.yml`: today `gate-1-test` DOES set
//! `KALEIDOSCOPE_PERF_TESTS` (the falsifier for #1) and there is NO
//! `perf-kpis` job (the falsifier for #2). They are tagged
//! `#[ignore = "RED until DELIVER: ..."]` so `cargo test` is GREEN on
//! the current tree; `cargo test -- --ignored` shows them FAILING,
//! which is the falsifiability evidence. DELIVER removes the
//! `#[ignore]` when the workflow edit lands.
//!
//! Assertions #3 and #4 are un-ignored GREEN controls: they pass
//! BEFORE and AFTER the DELIVER edit. #3 is the load-bearing negative
//! control (the correctness gate must survive the edit untouched); #4
//! guards that the honesty record is present.
//!
//! ## Mandate 7 — RED, not BROKEN
//!
//! This test reads an existing file and asserts on its content. No
//! production symbol is missing, so it COMPILES and links; the RED
//! assertions FAIL behaviourally (the workflow has not been edited),
//! they do not error on setup. That is the correct RED shape.

use std::fs;
use std::path::PathBuf;

/// The repository root, resolved from this crate's manifest directory.
///
/// `CARGO_MANIFEST_DIR` is `<repo>/crates/integration-suite`; two
/// parents up is the repository root. This is robust regardless of the
/// caller's working directory.
fn repo_root() -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    PathBuf::from(manifest_dir)
        .parent()
        .and_then(|p| p.parent())
        .expect("crate manifest dir has a grandparent (the repo root)")
        .to_path_buf()
}

/// Read the committed CI workflow definition.
fn read_ci_workflow() -> String {
    let path = repo_root().join(".github/workflows/ci.yml");
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("could not read {}: {e}", path.display()))
}

/// Read ADR-0070.
fn read_adr_0070() -> String {
    let path = repo_root().join("docs/product/architecture/adr-0070-perf-kpi-non-gating-ci.md");
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("could not read {}: {e}", path.display()))
}

/// Extract the lines belonging to a single top-level job block from the
/// `ci.yml` text.
///
/// Jobs are indented by exactly two spaces under `jobs:` (e.g.
/// `  gate-1-test:`). A job block runs from its `  <name>:` header up to
/// (but not including) the next two-space-indented `  <other>:` header
/// or end of file. This lets an assertion be SCOPED to one job, so a
/// key present in a *different* job (e.g. the `perf-kpis` job's env)
/// cannot false-pass or false-fail a check meant for `gate-1-test`.
///
/// Returns `None` if the job is not found.
fn job_block<'a>(workflow: &'a str, job_name: &str) -> Option<&'a str> {
    let header = format!("  {job_name}:");
    let lines: Vec<&str> = workflow.lines().collect();

    // Find the header line whose trimmed content is exactly `<name>:`
    // and which is indented by exactly two spaces (a top-level job).
    let start = lines.iter().position(|line| {
        *line == header.as_str()
            || (line.starts_with("  ")
                && !line.starts_with("   ")
                && line.trim_end() == header.trim_end())
    })?;

    // The byte offset of the start-of-block line within `workflow`.
    let start_byte = byte_offset_of_line(workflow, start);

    // Find the next two-space-indented job header after `start`.
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
        // `lines()` strips the trailing '\n'; add it back. This is
        // correct for '\n'-terminated files (the repo convention).
        offset += line.len() + 1;
    }
    offset
}

// =====================================================================
// Scenario 3 (US-03; C2) — GREEN CONTROL, un-ignored.
//
// The correctness gate is preserved: `gate-1-test` STILL runs
// `cargo test --workspace --all-targets --locked`. This passes against
// today's ci.yml AND must keep passing after the DELIVER edit (the
// edit only removes the perf env; it must not touch the invocation).
// It is the negative control that proves de-gating perf does not
// de-gate correctness.
// =====================================================================
#[test]
fn gate_1_test_still_runs_the_correctness_gating_invocation() {
    let workflow = read_ci_workflow();
    let gate_1 = job_block(&workflow, "gate-1-test").expect("gate-1-test job must exist in ci.yml");

    assert!(
        gate_1.contains("cargo test --workspace --all-targets --locked"),
        "gate-1-test must run the correctness-gating invocation \
         `cargo test --workspace --all-targets --locked` (US-03; C2). \
         De-gating perf must NOT remove the correctness gate. \
         gate-1-test block was:\n{gate_1}"
    );
}

// =====================================================================
// Scenario 4 (US-04) — GREEN CONTROL, un-ignored.
//
// The durable-op honesty note exists in ADR-0070: the durable-op
// budgets are dev-indicative not CI-contractual, the cost is the
// per-record fsync of ADR-0049/0060 (not a regression), and
// threshold-raising is explicitly not the fix. This passes today
// (ADR-0070 is already written) and guards that the honesty record
// stays present.
// =====================================================================
#[test]
fn adr_0070_records_the_durable_op_honesty_note() {
    let adr = read_adr_0070();

    assert!(
        adr.contains("dev-indicative") || adr.contains("DEV-INDICATIVE"),
        "ADR-0070 must record that the durable-op budgets are \
         dev-indicative, not CI-contractual (US-04)."
    );
    assert!(
        adr.contains("ADR-0049") && adr.contains("ADR-0060"),
        "ADR-0070 must attribute the durable cost to the per-record \
         fsync of ADR-0049 / ADR-0060, not a regression (US-04)."
    );
    assert!(
        adr.contains("Threshold-raising is explicitly NOT the fix")
            || adr.contains("threshold-raising is explicitly NOT the fix")
            || adr.contains("Threshold-raising is explicitly not the fix"),
        "ADR-0070 must state that threshold-raising is explicitly NOT \
         the fix, citing the non-gating posture (US-04)."
    );
}

// =====================================================================
// Scenario 1 (US-01) — RED until DELIVER, `#[ignore]`d.
//
// FALSIFIABILITY: against TODAY's ci.yml this FAILS, because
// gate-1-test currently sets `KALEIDOSCOPE_PERF_TESTS: "1"` (the
// env block at the pre-change ci.yml:140-141). DELIVER deletes that
// env block; only then does this assertion pass, and DELIVER removes
// the `#[ignore]`. Run `cargo test -- --ignored` to see it fail today.
//
// Scoped to the gate-1-test job block ONLY, so the `perf-kpis` job's
// own (correct) `KALEIDOSCOPE_PERF_TESTS` env cannot false-pass this.
// =====================================================================
#[test]
#[ignore = "RED until DELIVER: gate-1-test must stop setting KALEIDOSCOPE_PERF_TESTS (ci.yml not yet edited)"]
fn gate_1_test_does_not_opt_into_wall_clock_perf_tests() {
    let workflow = read_ci_workflow();
    let gate_1 = job_block(&workflow, "gate-1-test").expect("gate-1-test job must exist in ci.yml");

    assert!(
        !gate_1.contains("KALEIDOSCOPE_PERF_TESTS"),
        "the gate-1-test job must NOT set KALEIDOSCOPE_PERF_TESTS \
         (US-01): with the variable absent the 28 wall-clock KPI tests \
         self-skip in the gate, so a perf breach on noisy CI hardware \
         can no longer red the build. gate-1-test block was:\n{gate_1}"
    );
}

// =====================================================================
// Scenario 2 (US-02) — RED until DELIVER, `#[ignore]`d.
//
// FALSIFIABILITY: against TODAY's ci.yml this FAILS, because there is
// NO `perf-kpis` job yet. DELIVER adds the non-gating job; only then
// does this assertion pass, and DELIVER removes the `#[ignore]`. Run
// `cargo test -- --ignored` to see it fail today.
//
// The perf-kpis job must (a) exist, (b) set KALEIDOSCOPE_PERF_TESTS,
// (c) run `cargo test --workspace`, and (d) be `continue-on-error:
// true` (the non-gating lever). All four are checked, scoped to the
// perf-kpis job block.
// =====================================================================
#[test]
#[ignore = "RED until DELIVER: a non-gating perf-kpis job must exist (ci.yml not yet edited)"]
fn a_non_gating_perf_kpis_job_runs_the_wall_clock_family() {
    let workflow = read_ci_workflow();

    let perf = job_block(&workflow, "perf-kpis").unwrap_or_else(|| {
        panic!(
            "a `perf-kpis` job must exist in ci.yml (US-02): the \
             non-gating home for the 28 wall-clock KPI tests the \
             gating Gate 1 no longer runs."
        )
    });

    assert!(
        perf.contains("KALEIDOSCOPE_PERF_TESTS: \"1\"")
            || perf.contains("KALEIDOSCOPE_PERF_TESTS: '1'"),
        "the perf-kpis job must set KALEIDOSCOPE_PERF_TESTS to \"1\" \
         (US-02) so the ADR-0058 self-skip guard lets the whole \
         guarded family run. perf-kpis block was:\n{perf}"
    );
    assert!(
        perf.contains("cargo test --workspace"),
        "the perf-kpis job must run `cargo test --workspace` so the \
         whole wall-clock family (all 28 tests, 11 crates) runs by \
         env-var presence (US-02; C5). perf-kpis block was:\n{perf}"
    );
    assert!(
        perf.contains("continue-on-error: true"),
        "the perf-kpis job must be `continue-on-error: true` (US-02): \
         the non-gating lever, so a perf breach is a visible red X on \
         the job while the overall workflow conclusion stays success. \
         perf-kpis block was:\n{perf}"
    );
}
