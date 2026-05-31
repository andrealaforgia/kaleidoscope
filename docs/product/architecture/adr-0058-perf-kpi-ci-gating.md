# ADR-0058 — Wall-clock KPI tests are enforced in CI, skipped locally by default

- **Status**: Accepted
- **Date**: 2026-05-31
- **Author**: `nw-solution-architect` (Morgan)
- **Supersedes**: none
- **Superseded by**: none

## Context

Kaleidoscope has 28 wall-clock KPI tests across 11 crates (lumen, pulse, ray,
strata, cinder, sluice, beacon, augur, aegis). Each measures latency with
`std::time::Instant` and asserts a p95 threshold tuned for the GitHub Actions
`ubuntu-latest` runner. Examples: `lumen ingest_p95_latency_under_three_milliseconds`
(3 ms) and `pulse query_p95_latency_under_ten_milliseconds` (10 ms).

These tests flake in the LOCAL pre-commit hook when the developer machine is
under load, for example during an autonomous development loop running many
parallel cargo builds. The flakes are not regressions: the fastest ordered
samples are tens of microseconds, and the p95 inflation comes from fsync and
scheduler contention under local load. The same tests pass reliably on the
controlled CI runner the thresholds were tuned for.

The local flakes have forced repeated `git commit --no-verify` bypasses, which
erode the pre-commit discipline that keeps main socially always green under
pure trunk-based development. The root problem is that the local machine was
never a reliable environment for these particular measurements, yet the local
hook ran them as though it were.

This ADR records WHERE the wall-clock KPIs are enforced. It cites ADR-0005 (the
five-gate CI contract; Gate 1 is `cargo test`) without modifying it.

## Decision

Wall-clock KPI tests are enforced in CI and skipped locally by default, via a
presence-based environment-variable guard.

1. **Guard.** Each gated test body opens with an inline early-return preamble,
   byte-identical at all 28 sites:

   ```rust
   if std::env::var("KALEIDOSCOPE_PERF_TESTS").is_err() {
       eprintln!("perf test skipped: set KALEIDOSCOPE_PERF_TESTS=1 to run");
       return;
   }
   ```

2. **Contract.** Presence-based. The variable absent (`is_err()` true) means
   skip: the test prints the note to stderr and passes with no measurement taken.
   The variable present with any value means run the full measurement and
   threshold assertion. The guard never panics; a panic would be
   indistinguishable from a real failure and would not solve the bypass problem.

3. **CI opt-in.** The `gate-1-test` job in `.github/workflows/ci.yml` sets
   `KALEIDOSCOPE_PERF_TESTS: "1"` in a job-level `env` block with a hardcoded
   literal, consistent with the existing `NIGHTLY_PIN` workaround for the GitHub
   Actions job-level env evaluation quirk in gates 2 and 3. The local pre-commit
   hook does NOT set the variable, so it skips these tests.

4. **Thresholds unchanged.** No threshold literal, sample count, warm-up loop,
   or percentile index is altered. The guard is added only as a preamble.

5. **Inline, not a shared helper.** No shared test-util crate is consumed by the
   11 perf crates, so a helper would require either a new workspace
   dev-dependency crate or a copied module per crate. The inline preamble avoids
   both and is mutation-safe because the identical text everywhere leaves no
   per-site mutant a place to hide.

6. **Future perf tests.** A contributor adding a new wall-clock KPI test applies
   the same preamble. This ADR is the citable standard.

## Consequences

### Positive

- The local pre-commit hook is fast and deterministic under machine load; no
  `--no-verify` bypass is forced by a load-induced perf flake.
- The wall-clock KPIs remain a real, enforced gate in CI on the runner their
  thresholds were tuned for. A genuine latency regression turns `gate-1-test`
  red and blocks the merge.
- The pattern is uniform across all 28 tests and documented here for future
  contributors, so no straggler test is left to flake.

### Negative

- A developer who wants to run the perf tests locally must export the variable
  (`KALEIDOSCOPE_PERF_TESTS=1`). This is a deliberate opt-in, documented in the
  skip note itself.
- The perf tests no longer run in the pre-commit hook, so a real performance
  regression is discovered in CI rather than in pre-commit. This is acceptable
  because the local hook was never a reliable environment for these measurements;
  catching a regression on the controlled CI runner is the correct and only
  trustworthy gate.

## Alternatives Considered

### Raise the thresholds to absorb local load

Rejected. Loosening the thresholds weakens the real CI gate to accommodate an
environment (the loaded local machine) the gate was never meant to run on. The
thresholds are correct for `ubuntu-latest`; the problem is location, not value.

### Remove the wall-clock KPI tests

Rejected. This discards the KPI coverage entirely. The tests are valuable on the
controlled runner; only their local execution under load is the problem.

### A Criterion benchmark harness

Rejected for now. A `criterion`-based harness is a larger change (benchmark
targets, statistical machinery, separate invocation) than the problem warrants.
It remains available as future work if richer latency tracking is wanted; it is
out of scope for eliminating the local flake.

### `#[ignore]` plus `--include-ignored` in CI

Rejected. `--include-ignored` is workspace-global and would re-activate every
unrelated ignored test in the workspace (for example the AC-CAP `#[ignore]`
tests in the trace and log query-api slices), which is broader than intended.
The early-return guard is per-test and surgical.

## References

- ADR-0005 (CI contract; Gate 1 is `cargo test`). Cited, NOT modified.
- DISCUSS artefacts: `docs/feature/perf-kpi-ci-gating-v0/discuss/`.
- DESIGN artefacts: `docs/feature/perf-kpi-ci-gating-v0/design/wave-decisions.md`,
  `docs/feature/perf-kpi-ci-gating-v0/design/application-architecture.md`.
- Project memory: p95 wall-clock flakes under local parallel-build load forcing
  `--no-verify` bypasses; GitHub Actions job-level env evaluation quirk
  (hardcode literals in job-level `env`).
