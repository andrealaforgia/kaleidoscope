# Wave Decisions — speed-up-local-precommit-v0 (DISCUSS)

Feature type: **Infrastructure**. Persona: the developer/maintainer (human or
agent) committing locally to Kaleidoscope. Walking skeleton: **No** (the
operator surfaces — `git commit` and the CI results page — already exist;
this feature changes WHAT the hook runs and adds a watching cadence).

## Verified problem (read in code, not assumed)

- `scripts/hooks/pre-commit` runs, in order: Step 0 toolchain symmetry
  check (fast), Step 1 `cargo fmt --all -- --check` (fast), Step 2
  `cargo clippy --all-targets --locked -- -D warnings` (moderate), Step 3
  `cargo deny --all-features check` (Gate 4, fast), Step 4
  **`cargo test --workspace --all-targets --locked` (Gate 1)** — the slow
  part (hook lines 92-93).
- `--all-targets --workspace` pulls in the heavy I/O-bound integration and
  durability suites across 26 crates: the fsync-heavy `v1_slice_0x` WAL /
  snapshot / torn-tail tests (lumen, ray, strata, cinder, sluice, beacon,
  pulse), the `*_crash_target` durability tests, and the subprocess tests
  (aperture `slice_10_ingest_auth`, `serve_loop_error_surfacing`,
  `cli_smoke`, `probe_gold_runner`, kaleidoscope-cli `*_roundtrip`). These
  are I/O-bound under per-record `sync_all` (ADR-0049/0060) and take
  10-20 min under parallel load; a prior commit's hook wedged for hours
  under leaked-process contention.
- **CI already runs the deep gate**: `.github/workflows/ci.yml`
  `gate-1-test` runs `cargo test --workspace --all-targets --locked`
  (ci.yml line 182). The deep coverage already exists in CI; the local
  hook DUPLICATES it slowly.
- Posture: Kaleidoscope is pure trunk-based, no required-status-checks,
  "CI is feedback, not a gate" (memory `project_kaleidoscope_pure_trunk_based`;
  brief C7). The local hook is a courtesy ("main is socially always
  green"), not a hard gate. Moving the slow tests' GATING to CI is
  consistent with the project's stated posture.
- Sibling precedent: ADR-0070 (perf-kpi-non-gating-v0) already moved the
  *perf wall-clock assertions* off Gate 1 because durability fsync made
  them slow/flaky. **But ADR-0070 did NOT remove the durability tests
  from the local hook** — it only made the perf *measurement* self-skip
  locally; the durability test bodies still RUN locally and still pay the
  full fsync I/O cost. This feature is the missing local-side sibling:
  move the slow tests' local-blocking off the commit path.

## Decided here (DISCUSS scope)

- Make the LOCAL pre-commit hook fast: target **<= 5 min** (Andrea's
  explicit acceptance bar).
- Keep the cheap-high-value gates local: toolchain check, `cargo fmt
  --check`, `cargo clippy`, `cargo deny`.
- Replace the slow `cargo test --workspace --all-targets --locked` with a
  FAST test subset that still catches the cheap/common mistakes (compile
  errors, unit-test breaks).
- Keep the DEEP run (`cargo test --workspace --all-targets --locked`,
  incl. integration/durability/subprocess) in CI gate-1 (already there)
  as the authoritative deep gate. Do NOT weaken CI. Do NOT delete tests.
- Establish a concrete CI-results-watching cadence so the deep tests
  still have eyes once they leave the local blocking path.

## Flagged for DESIGN / DEVOPS (decisions DESIGN owns)

| # | Decision | Options surfaced in DISCUSS | DISCUSS recommendation |
|---|----------|-----------------------------|------------------------|
| D1 | Exact fast local test subset | (a) `cargo test --workspace --lib` (unit only — excludes all `tests/` integration, subprocess, durability bins, doctests); (b) curated `--all-targets`-minus-slow set via a deny-list / `--exclude` of the durability+subprocess test bins; (c) run fast integration tests but exclude durability/subprocess ones | **(a) `--lib`** as the simplest honest cut — it deterministically excludes every slow `tests/*.rs` bin and is trivially explainable. DESIGN MUST measure/estimate wall-clock and confirm <= 5 min (fmt+clippy+deny+`--lib`). If `--lib` alone leaves too much value on the table, DESIGN may widen to (b), but only with a measured time budget. |
| D2 | clippy scope locally | keep `--all-targets` (slower, but catches test-code issues) vs trim to lib-only | **Keep `--all-targets --locked` clippy locally** unless DESIGN's measurement shows it alone blows the 5-min budget. Clippy is compile-bound, not fsync-bound; it is the high-value cheap gate. Trim only if measured necessary. |
| D3 | CI-results-watching mechanism + cadence | (a) a documented `gh run list` / `gh run view` poll the developer/agent runs every N minutes; (b) a small wrapper script (e.g. `scripts/ci-watch.sh`) that summarises the latest main run; (c) a documented practice in CLAUDE.md / brief | **(b)+(c)**: a small, low-friction `scripts/ci-watch.sh` wrapping `gh run list --branch main`/`gh run view`, plus a documented cadence (e.g. "watch after every push, and a periodic poll while the agent works"). DESIGN owns the exact script shape and the cadence number. Must be concrete, not hand-waved — this is the mitigation for moving deep tests off the local blocking path. |
| D4 | Keep the fast gates | toolchain check + fmt + deny stay (all fast) | **Keep all three.** No change. |
| D5 | Honesty trade documentation | with deep tests off the local blocking path, a local commit CAN reach main with a deep-test regression that only CI catches | **ACCEPTABLE under trunk-based "CI is feedback" PROVIDED the D3 cadence is real.** DESIGN MUST document this trade explicitly (an ADR is the natural home, consistent with ADR-0070's framing). |
| D6 | Slow durability tests themselves | still slow IN CI | **Out of scope here.** This feature is about the LOCAL gate. A separate future feature could speed the durability tests (e.g. a faster test fsync backend). Flag, do not fix. |

## Constraints carried into DESIGN

- Do NOT weaken CI (the deep gate `cargo test --workspace --all-targets
  --locked` stays in gate-1).
- Do NOT delete any test.
- The fast subset MUST still catch the cheap/common mistakes (compile
  errors, unit-test breaks, fmt, clippy).
- Honour the trunk-based "CI is feedback" posture.
- The CI-watching cadence is the mitigation and MUST be concrete.
- A fast hook makes the rule "NEVER `--no-verify`" easier to honour
  (side benefit, not a goal).
- Rust idiomatic where any script changes land.
- NEVER bump any crate to 1.0.0 (no crate change in this feature).

## Risks

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| A deep-test regression reaches main caught only by CI | Medium | Medium | D3 watching cadence; trunk-based fix-forward posture |
| The fast subset is still > 5 min (clippy --all-targets dominates) | Low | Medium | D2 measurement; trim clippy scope if measured necessary |
| Developer/agent ignores the CI cadence, deep regressions linger | Medium | Medium | D3 makes the check one command; document in CLAUDE.md |
| DIVERGE artifacts absent (no recommendation.md / job-analysis.md) | N/A | Low | Infrastructure feature; JTBD supplied verbatim by Andrea in the origin brief and grounded below. Noted as accepted. |
