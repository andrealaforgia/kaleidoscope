# Acceptance Test Scenarios — perf-kpi-ci-non-gating-v0 (DISTILL)

- **Agent**: Scholar (`nw-acceptance-designer`), 2026-06-06
- **Acceptance type**: STRUCTURAL — assert on the committed
  `.github/workflows/ci.yml` (and ADR-0070), not a runtime behaviour. The
  observable outcome the maintainer wants lives in the workflow definition
  they read on the GitHub Actions run page.
- **Test file**:
  `crates/integration-suite/tests/v0_perf_kpi_ci_non_gating_structure.rs`
- **Parsing**: robust string/section assertions, scoped per job block. No
  `serde_yaml` (not in `Cargo.lock`; no dep added). Path via
  `CARGO_MANIFEST_DIR` → repo root.

## Structural-acceptance note

A CI-gating restructure has no runtime user journey and no driving port. The
acceptance is the SHAPE of the committed workflow file: a maintainer's
"observable outcome" is what `gate-1-test` and a `perf-kpis` job look like on
the run page. So each scenario below is a Given (the committed `ci.yml` /
ADR-0070), a When (a maintainer / the structural check inspects a job block),
and a Then (the asserted shape). This mirrors the UAT scenarios in
`discuss/user-stories.md` US-01/02 ("inspects the job-level env block",
"a job ... sets ... continue-on-error") rendered as machine-checkable
structural assertions.

## Scenarios

### Scenario 1 — Gate 1 does not opt in to wall-clock perf tests (US-01)

```gherkin
@US-01 @structural @red-until-deliver
Scenario: The build-gating job does not opt in to wall-clock perf tests
  Given the gate-1-test job definition in the committed CI workflow
  When the structural check inspects the gate-1-test job block
  Then KALEIDOSCOPE_PERF_TESTS is not set anywhere in that job block
```

- **Test fn**: `gate_1_test_does_not_opt_into_wall_clock_perf_tests`
- **State**: RED, `#[ignore = "RED until DELIVER: gate-1-test must stop
  setting KALEIDOSCOPE_PERF_TESTS ..."]`
- **Falsifier (today)**: `ci.yml:140-141` sets `KALEIDOSCOPE_PERF_TESTS: "1"`
  in the `gate-1-test` job-level env → assertion FAILS under `--ignored`.
- **Scoping**: assertion runs against the `gate-1-test` job block ONLY, so the
  post-DELIVER `perf-kpis` job (which legitimately sets the same variable)
  cannot false-pass it.

### Scenario 2 — A non-gating perf job runs the wall-clock family (US-02)

```gherkin
@US-02 @structural @red-until-deliver
Scenario: A separate non-gating job runs and reports the perf KPIs
  Given the committed CI workflow
  When the structural check inspects the perf-kpis job block
  Then a perf-kpis job exists
  And it sets KALEIDOSCOPE_PERF_TESTS to "1"
  And it runs cargo test --workspace
  And it is marked continue-on-error: true so a breach never blocks the workflow
```

- **Test fn**: `a_non_gating_perf_kpis_job_runs_the_wall_clock_family`
- **State**: RED, `#[ignore = "RED until DELIVER: a non-gating perf-kpis job
  must exist ..."]`
- **Falsifier (today)**: no `perf-kpis` job exists in `ci.yml` → the
  `job_block` lookup is `None` → assertion FAILS under `--ignored`.
- **Covers C4 (visibility) + C5 (whole family)**: `continue-on-error: true`
  keeps a breach a visible red X; `cargo test --workspace` runs the whole
  guarded family by env-var presence (no per-test enumeration).

### Scenario 3 — A real correctness break still gates (US-03; C2) — GREEN CONTROL

```gherkin
@US-03 @structural @negative-control @green
Scenario: De-gating perf does not de-gate correctness
  Given the gate-1-test job definition in the committed CI workflow
  When the structural check inspects the gate-1-test job block
  Then it still runs cargo test --workspace --all-targets --locked
```

- **Test fn**: `gate_1_test_still_runs_the_correctness_gating_invocation`
- **State**: GREEN, un-ignored. Passes today AND must keep passing after the
  DELIVER edit (which deletes only the env block). The load-bearing negative
  control: if a future edit removed the correctness invocation, this reds.
- **Behavioural complement**: the runtime demonstration (a throwaway-branch
  correctness failure observed to red Gate 1) is a DELIVER/CI act, recorded as
  the DELIVER obligation in `distill/wave-decisions.md`.

### Scenario 4 — The durable-op honesty note is recorded (US-04) — GREEN CONTROL

```gherkin
@US-04 @structural @docs-presence @green
Scenario: ADR-0070 records the durable-op honesty note
  Given ADR-0070
  When a contributor reads the honesty note
  Then it states the durable-op budgets are dev-indicative, not CI-contractual
  And it attributes the cost to the per-record fsync of ADR-0049 / ADR-0060
  And it states threshold-raising is explicitly not the fix
```

- **Test fn**: `adr_0070_records_the_durable_op_honesty_note`
- **State**: GREEN, un-ignored. ADR-0070 is already written; this guards that
  the honesty record stays present.

## Test-fn → US / AC map

| Test fn | US | AC covered | State |
|---|---|---|---|
| `gate_1_test_does_not_opt_into_wall_clock_perf_tests` | US-01 | "gate-1-test does NOT set KALEIDOSCOPE_PERF_TESTS" | RED `#[ignore]` |
| `a_non_gating_perf_kpis_job_runs_the_wall_clock_family` | US-02 | "new non-gating job sets the var, runs the whole family, breach does not fail the workflow" (continue-on-error + var + `--workspace`) | RED `#[ignore]` |
| `gate_1_test_still_runs_the_correctness_gating_invocation` | US-03 | "gate-1-test continues to run `cargo test --workspace --all-targets --locked`" | GREEN control |
| `adr_0070_records_the_durable_op_honesty_note` | US-04 | "ADR documents dev-indicative budgets, attributes cost to ADR-0049/0060, forbids threshold-raise" | GREEN control |

All four user stories (US-01..04) have at least one scenario — Dimension 8
Check A satisfied.

## Mandate-7 / falsifiable self-review checklist

- [x] **RED, not BROKEN** — the test reads an existing file; no production
  symbol is missing; it compiles and links; the RED assertions fail
  behaviourally on file content, not on setup. Verified: `--ignored` panics
  carry the assertion messages ("must NOT set KALEIDOSCOPE_PERF_TESTS",
  "a `perf-kpis` job must exist"), not I/O errors.
- [x] **Each RED assertion genuinely fails against today's `ci.yml`** — both
  fail under `--ignored` for the exact contract falsifier (env present;
  perf-kpis absent).
- [x] **Default `cargo test` GREEN** — 2 controls pass, 2 RED ignored.
- [x] **`--ignored` shows the RED ones FAILING** — 2 failed.
- [x] **No Fixture Theater** — the Given is the committed `ci.yml` /
  ADR-0070 (real preconditions). The test does not write the expected output;
  it reads the real, committed file. After DELIVER edits `ci.yml`, the RED
  assertions flip to GREEN with no test-logic change (only `#[ignore]`
  removed). The test cannot pass on the current tree without the production
  (workflow) edit — that is the proof the fixtures are not doing the feature's
  work.
- [x] **Assertions scoped per job block** — the US-01 env check is scoped to
  the `gate-1-test` block, so the post-DELIVER `perf-kpis` env cannot
  false-pass it.
- [x] **No spawned process** — only `std::fs` reads; no aperture, no ports.
- [x] **No new dependency** — no `serde_yaml`; string assertions only; nothing
  added to `Cargo.lock` (C8 preserved; Gates 2/3 unaffected).
- [x] **fmt / clippy clean.**

## Critique-dimensions self-review (acceptance-designer-reviewer)

Nested reviewer invocation unavailable in this autonomous context; applied the
critique-dimensions skill directly.

```yaml
review_id: "accept_rev_perf-kpi-ci-non-gating-v0"
reviewer: "acceptance-designer (review mode)"
strengths:
  - "Structural acceptance is the honest shape for a CI-config feature; no
     invented runtime port (no Testing Theater)."
  - "RED assertions carry precise, contract-named falsifiers and are scoped
     per job block, so the post-DELIVER perf-kpis env cannot false-pass the
     gate-1-test env check."
  - "GREEN control #3 is a real negative control: it guards that de-gating
     perf does not silently de-gate the correctness invocation."
dimensions:
  d1_happy_path_bias: "N/A for a structural CI-shape test; the four
     assertions are the contract, not a success/error split. The negative
     control (US-03) and the falsifier-on-today's-file ARE the error-path
     analogue. No finding."
  d2_gwt_format: "Scenarios are Given (committed file) / When (inspect job
     block) / Then (asserted shape), one behaviour each. Pass."
  d3_business_language: "Domain is CI-for-a-maintainer; terms (gate, job,
     env, continue-on-error) are the maintainer's own vocabulary on the run
     page, not leaked implementation. Acceptable for a structural CI feature."
  d4_coverage_completeness: "All four US (US-01..04) mapped to a scenario.
     Pass."
  d5_walking_skeleton_centricity: "The WS is the structural assertion on the
     committed YAML — declared in wave-decisions.md; user goal is the
     maintainer's trustworthy red. Pass."
  d7_observable_behavior: "Each Then asserts an observable property of the
     committed workflow (the surface a maintainer reads), not an internal
     test field. Pass."
  d8_traceability: "Check A: every US-id has >=1 scenario (table above).
     Check B (environments): N/A — no DEVOPS environments.yaml; the CI job
     execution contexts (gate-1-test, perf-kpis) are the 'environments' and
     both are asserted. No finding."
  d9_ws_boundary_proof: "Strategy declared (structural assertion on committed
     YAML). No adapter to fake; the real I/O is the fs read of the actual
     ci.yml. Litmus 'if I deleted the real adapter would the WS still pass?' —
     there is no fake adapter; deleting the real ci.yml panics the read. Pass."
nwave_order_note: "The absent ci.yml edit (DISTILL precedes DELIVER) is the
  EXPECTED state and the source of the falsifiable RED — not a finding."
approval_status: "approved"
blocking: 0
high: 0
iteration: 1
```
