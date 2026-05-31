# DESIGN Decisions: perf-kpi-ci-gating-v0

Author: `nw-solution-architect` (Morgan), DESIGN wave, 2026-05-31.
Mode: propose. Scope: application. British English; no em dashes in body text.

This DESIGN pins the six flags Luna raised in DISCUSS (commit c6f4480) and
records the one architectural decision worth a durable note (ADR-0058). The
feature is test-infrastructure only: it touches `crates/*/tests/*.rs` and
`.github/workflows/ci.yml` and adds one ADR. No production source under
`crates/*/src/` is changed; no threshold literal moves.

## Key Decisions

- [DD1] **Guard mechanism: inline, not a shared helper.** Each gated test gains
  a four-line early-return preamble as the FIRST statement of the test body. The
  preamble is byte-identical across all 28 sites. Rationale: no shared test-util
  crate is consumed by the 11 perf crates today (a `tests/common/mod.rs` exists
  only in 8 OTHER crates: harness, aperture, spark, sieve, codex, query-api,
  log-query-api, trace-query-api, none of which is in the perf set). A shared
  `skip_unless_perf()` would force EITHER a new workspace dev-dependency crate
  consumed by 11 crates OR a copied `common/mod.rs` per crate. The first creates
  a new crate purely to host four lines; the second is the same duplication as
  inline but with extra `mod common;` wiring. Inline is surgical, readable,
  greppable, and mutation-safe because the identical line everywhere means no
  per-site mutant can hide. (see: DISCUSS flag 1)

- [DD2] **Env-var: `KALEIDOSCOPE_PERF_TESTS`, presence-based.**
  `std::env::var("KALEIDOSCOPE_PERF_TESTS").is_err()` is the absence test:
  unset means skip, ANY value (including `1`, `true`, or the empty string) means
  run. Empty-string counts as SET under `is_err()` (it returns `Ok("")`), which
  is the documented and intended contract: the variable is a flag, not a value.
  CI sets it to the literal `"1"`. (see: DISCUSS flag 2)

- [DD3] **Skip mechanism: early-return, not `#[ignore]`.** The guard returns
  early so the test PASSES with no measurement and an stderr note. `#[ignore]`
  plus `--include-ignored` in CI is rejected: `--include-ignored` is workspace-
  global and would re-activate every UNRELATED ignored test in the workspace
  (for example the AC-CAP `#[ignore]` tests in the trace and log query-api
  slices), which is broader than intended and couples this feature to the state
  of unrelated ignored tests. Early-return is per-test and surgical.
  (see: DISCUSS flag 3, DISCUSS D5)

- [DD4] **Exact list: the 28 tests in the DISCUSS inventory, confirmed.** The
  inventory in `discuss/wave-decisions.md` (28 tests, 11 crates) is the
  authoritative set. Two sites were spot-confirmed at the source for the exact
  body shape: `lumen ingest_p95_latency_under_three_milliseconds`
  (`crates/lumen/tests/v1_slice_01_wal_durability.rs:307`, body opens with
  `let base = temp_base("kpi1");`) and `pulse query_p95_latency_under_ten_milliseconds`
  (`crates/pulse/tests/slice_02_structured_query.rs:330`, body opens with
  `let store = InMemoryMetricStore::new(...)`). Both are plain `#[test] fn`
  with a direct body, so the guard drops in as the first statement cleanly. The
  full per-file table is in `application-architecture.md`. (see: DISCUSS flag 4)

- [DD5] **CI workflow: job-level `env` block on `gate-1-test`, literal value.**
  Add `env:` with `KALEIDOSCOPE_PERF_TESTS: "1"` to the `gate-1-test` job
  (`.github/workflows/ci.yml`, job header at line 136, `cargo test --workspace`
  invocation at line 182). `gate-1-test` has NO existing `env:` block, so this
  adds one immediately under `needs: gate-4-deny` (line 139), before `steps:`.
  The literal `"1"` is hardcoded, NOT referenced via workflow-level
  `${{ env.X }}`: the project already works around the GitHub Actions job-level
  env evaluation quirk this way for `NIGHTLY_PIN` in gate-2 (line 86) and gate-3
  (lines 250, 358). No other gate job runs `cargo test`, so no other job changes.
  (see: DISCUSS flag 5, project memory feedback_github_actions_job_level_env)

- [DD6] **ADR-0058: yes.** The decision of WHERE the wall-clock KPIs are
  enforced (CI only, skipped locally by default) is a policy worth a durable
  record. ADR-0058 cites ADR-0005 (the five gates; Gate 1 is `cargo test`)
  WITHOUT modifying it. The slot `adr-0058*` is confirmed free.
  (see: DISCUSS flag 6, US-05)

## Architecture Summary

A presence-gated early-return guard, identical at every site, fronts each of the
28 wall-clock p95 tests. The variable is absent in the local pre-commit hook (it
does not set it), so the hook skips the perf tests and goes green deterministically
under machine load. The CI `gate-1-test` job sets the variable, so the same tests
run their full measurement and enforce their UNCHANGED thresholds on the
`ubuntu-latest` runner the thresholds were tuned for. The guard is a test-body
preamble, not a framework: no ports, no adapters, no new component. The
"architecture" here is the placement contract (first statement of each test body)
plus the CI policy (one job sets the flag) plus the documented standard (ADR-0058)
that keeps future perf tests uniform.

## Scale

**Single slice.** Every edit is mechanical and identical: the same four-line
preamble at 28 sites plus one job-level `env` block. The change is coherent and
low-risk (no production source, no threshold, no public surface). US-01 acts as
the thin end-to-end demonstration on one confirmed flaker; US-04 is the fan-out
of the identical edit. There is no sequencing dependency between sites, so the
28 test edits and the CI edit land together in one atomic DELIVER commit.

## Reuse Analysis

| Existing Component | File | Overlap | Decision | Justification |
|--------------------|------|---------|----------|---------------|
| 28 wall-clock p95 tests | `crates/{lumen,pulse,ray,strata,cinder,sluice,beacon,augur,aegis}/tests/*.rs` | Each already is the perf measurement the guard fronts | EXTEND | Add the 4-line preamble as the first body statement; no threshold, sample count, warm-up, or percentile index touched (US-03) |
| Gate 1 CI job | `.github/workflows/ci.yml` gate-1-test (line 136, invocation line 182) | Already runs `cargo test --workspace` | EXTEND | Add a job-level `env:` block; mirrors gate-2/gate-3 hardcoded-literal env pattern |
| Local pre-commit hook | `scripts/hooks/pre-commit` (line 92, `cargo test --workspace`) | Already runs the workspace test suite | UNCHANGED | Hook deliberately does NOT set the variable; that absence IS the local-skip mechanism. No edit needed |
| Shared test-util / `perf_support` crate | (none exists for the 11 perf crates) | Would host `skip_unless_perf()` | CREATE NEW: REJECTED | No `tests/common/mod.rs` in any of the 11 perf crates; a new crate or per-crate copied module costs more wiring than it saves (DD1). Inline preamble chosen |
| ADR slot 0058 | `docs/product/architecture/adr-0058-*` | New policy record | CREATE NEW: JUSTIFIED | Slot confirmed free; the WHERE-enforced policy has no existing home; ADR-0005 is immutable and only cited |

Zero unjustified CREATE NEW. The single CREATE NEW (ADR-0058) is the policy
record itself; the rejected CREATE NEW (a helper crate) is documented above.

## Technology Stack

- **`std::env::var`** (Rust standard library): the only mechanism the guard
  needs. Already in `std`; zero new dependency, zero `Cargo.toml` edit, zero
  `Cargo.lock` diff anywhere in the workspace.
- **`eprintln!`** (Rust standard library): the one-line stderr skip note.
- **GitHub Actions job-level `env`**: the CI opt-in, hardcoded literal value.

No new crate. No new dev-dependency. No external integration, therefore no
consumer-driven contract-test recommendation.

## DEVOPS Handoff

The CI change (the `env` block) and the test guards are a single coherent
change: the variable in CI without the guards is a no-op, and the guards without
the CI variable would silently disable the KPIs everywhere. They must land
together.

- **Apex (nw-platform-architect, DEVOPS wave): documentation-only slim wave.**
  Apex records the environment contract (the `KALEIDOSCOPE_PERF_TESTS` variable,
  presence-based, set in `gate-1-test` only) in the DEVOPS environments doc and
  produces a slim `devops/wave-decisions.md`. Apex does NOT edit
  `.github/workflows/ci.yml`. The reason: the `env` block is meaningless without
  the 28 guard lines, and splitting a one-line CI edit from the guards it gates
  would create a window where CI sets a variable no test reads. Apex documents;
  it does not pre-land the CI edit.
- **Crafty (nw-software-crafter, DELIVER wave): the atomic implementation.**
  Crafty makes BOTH edits in one `feat` commit: the four-line guard preamble at
  all 28 sites AND the `gate-1-test` job-level `env` block in
  `.github/workflows/ci.yml`. This is the only commit that touches either. The
  pre-commit hook is left untouched. Crafty reconciles the 28-site list against
  the DISCUSS inventory at DELIVER time (US-04 technical note) in case a perf
  test was added between DISCUSS and DELIVER.

This split is RECOMMENDED, not the only option. The alternative (Apex lands the
CI `env` block in DEVOPS, Crafty lands only the guards in DELIVER) is rejected
because it severs a coherent change across two waves and two commits for no
benefit, and leaves main in a state where CI sets an unread variable.

## Constraints Established

- The guard is the FIRST statement of each test body; nothing above it may run
  a measurement.
- The guard line is byte-identical at all 28 sites (mutation-safety and grep
  uniformity, US-04).
- No threshold literal, sample count, warm-up loop, or percentile index changes
  (US-03 / DISCUSS K3).
- The variable is presence-based; CI sets the literal `"1"`.
- The pre-commit hook is NOT modified and does NOT set the variable.
- ADR-0005 is cited, never edited (immutability).
- No crate bumped to 1.0.0.

## Upstream Changes

- None. No DESIGN decision changes any DISCUSS assumption. The inline-versus-
  helper flag is resolved toward the DISCUSS recommendation (inline), the
  presence-based contract is resolved as recommended, and the early-return and
  ADR-0058 recommendations are all confirmed rather than overturned.
