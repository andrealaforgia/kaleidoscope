# Wave Decisions — perf-kpi-ci-non-gating-v0 (DISTILL)

- **Wave**: DISTILL (nWave)
- **Agent**: Scholar (`nw-acceptance-designer`)
- **Date**: 2026-06-06
- **Mode**: Autonomous. **STRUCTURAL acceptance** wave — the acceptance for
  this CI-gating restructure is structural (assert on the committed
  `.github/workflows/ci.yml`), not a runtime behaviour. No commit (Andrea
  commits). No `crates/*/src` change; the only artefact under `crates/` is a
  meta-test in `integration-suite/tests`.

## Prior Wave Consultation (+/- checklist)

| Artefact | + (used) | − (gap / flag) |
|---|---|---|
| `adr-0070-perf-kpi-non-gating-ci.md` | The whole contract: §1 (remove env from `gate-1-test`); §2 (new `perf-kpis` job sets the var, runs the family); §3 (`continue-on-error: true`); §5 (keep asserts, p95 prints on breach); §6 (durable-op honesty note); the Verification section (structural + negative control). | − none. The absent `ci.yml` edit is the EXPECTED nWave-order state (DISTILL precedes DELIVER); it is the source of the falsifiable RED, not a gap. |
| `devops/wave-decisions.md` | The EXACT `perf-kpis` job spec (`continue-on-error: true`, `KALEIDOSCOPE_PERF_TESTS: "1"`, `cargo test --workspace --all-targets --locked`, sibling of `gate-1-test` off `gate-4-deny`); the gate-1-test env deletion diff (`ci.yml:140-141`); the single-setter grep proof; the DISTILL-seam note (the four structural assertions). | − none. |
| `design/wave-decisions.md` | F1 (exact restructure); F3 (keep asserts; the assert-message visibility seam at `cinder/tests/...:282-285`); F4 (ADR-only honesty note); F5 (whole family by env presence); the "the seam is the CI env" framing — which is why acceptance is structural. | − none; DESIGN peer-review verdict was APPROVED. |
| `brief.md` "For Acceptance Designer (DISTILL)" (`:5943`) | The authoritative four-part structural-acceptance list (gate-1 no perf env; perf-kpis exists+opts-in+continue-on-error; visibility-on-breach; negative control correctness still gates) + the honesty-note check; the explicit "structural, plus one behavioural negative control" framing. | − the behavioural negative control (a real correctness break still reds Gate 1) is encoded here STRUCTURALLY as US-03's GREEN control (the gating invocation is present and unchanged); the runtime throwaway-branch demonstration is a DELIVER/CI act, recorded below. |
| `discuss/user-stories.md` | US-01..04 and their AC; the four UAT scenario sets; the system constraints C1-C9. Each test fn carries its US tag in the doc-comment and the assert message. | − none; all four stories map to a scenario (see scenarios doc). |
| `.github/workflows/ci.yml` (current) | `gate-1-test` job (`:136-242`): the `env: KALEIDOSCOPE_PERF_TESTS: "1"` at `:140-141` (the US-01 falsifier — present today), the gating invocation `cargo test --workspace --all-targets --locked` at `:184` (the US-03 control — present today); confirmed NO `perf-kpis` job anywhere in the file (the US-02 falsifier). | − none. The current state is exactly what makes #1/#2 RED and #3 GREEN. |

## Headline

**One structural Rust test over the committed `ci.yml`, four assertions,
two RED (`#[ignore]`d until DELIVER) and two GREEN controls.** The RED ones
encode the DELIVER edit's target shape and FAIL against today's file (the
falsifier); the GREEN controls guard the invariants that must hold before AND
after DELIVER.

**nWave-order note (for the reviewer):** DISTILL precedes DELIVER, so the
`ci.yml` edit (remove the perf env from `gate-1-test`; add the `perf-kpis`
job) does NOT exist yet. The two `#[ignore]`d structural-RED assertions
failing under `--ignored` is the EXPECTED and CORRECT state — it is the
falsifiability evidence, not a defect.

## Walking-skeleton strategy — structural assertion on the committed YAML

This feature has no runtime user journey: the "user" is the maintainer
reading the GitHub Actions run page, and the observable outcome they want
lives entirely in the committed workflow definition. So the walking skeleton
is a **structural assertion on the committed `.github/workflows/ci.yml`**: the
test reads the real workflow file (the surface a maintainer reads) and asserts
the post-DELIVER shape. There is no service to stand up, no driving port, no
spawned process — the workflow file IS the driving surface. This is the
honest shape for a CI-config feature; inventing a runtime port would be
Testing Theater.

- **No `@in-memory` / no real-vs-fake adapter question.** There is no adapter:
  the test reads a real, committed file from disk via a path resolved from
  `CARGO_MANIFEST_DIR`. The "real I/O" is the `fs::read_to_string` of the
  actual `ci.yml` — there is nothing to fake and nothing to wire.
- **No spawned process.** Per the discipline: this is a YAML-parsing test; no
  aperture, no ports, no subprocess.

## Parsing approach — string/section assertions (no serde_yaml)

- **Decision: robust string/section assertions, NOT a YAML parser.**
  `grep -n "serde_yaml\|serde-yaml\|serde_yml\|yaml" Cargo.lock` returns
  **no match** — there is no YAML parser in the workspace lock. Per the
  discipline ("do NOT add a new external dep just for this — prefer
  string-based assertions over adding serde_yaml if it is not already a dep"),
  the test uses string/section assertions and adds **zero** dependencies.
- **Scoping (the load-bearing correctness point).** A naive
  `workflow.contains("KALEIDOSCOPE_PERF_TESTS")` would false-PASS the US-01
  check after DELIVER, because the new `perf-kpis` job legitimately sets that
  same variable. The test therefore extracts the **single job block** for the
  job under test (a `job_block()` helper that slices from `  <name>:` to the
  next two-space-indented top-level job header) and scopes each assertion to
  that block. The US-01 env check runs against the `gate-1-test` block only;
  the US-02 checks run against the `perf-kpis` block only. This is what makes
  the falsifier precise: removing the env from `gate-1-test` flips #1 to GREEN
  even though `perf-kpis` still carries the variable.
- **Path resolution.** `repo_root()` = `CARGO_MANIFEST_DIR` (which is
  `<repo>/crates/integration-suite`) walked up two parents to the repo root,
  then `.join(".github/workflows/ci.yml")`. Confirmed the path resolves
  (`ls <root>/.github/workflows/ci.yml` succeeds) and is independent of the
  caller's working directory.

## Test-home decision — `crates/integration-suite/tests/`

- **No existing structural/meta test home.**
  `grep -rln "ci.yml\|\.github\|workflows" crates/*/tests/` returns nothing;
  no test reads the repo structure today, so there is no convention to mirror.
- **`integration-suite` is the natural home.** Its own `Cargo.toml`
  description: "Kaleidoscope cross-crate integration suite. No library; only
  tests ... The first evidence the platform is one thing, not 18 disconnected
  libraries." A cross-cutting structural assertion about the platform's CI
  contract is exactly that genre. It is `publish = false`, uses explicit
  `[[test]]` blocks, and has no production surface to perturb.
- **Mechanics.** New file
  `crates/integration-suite/tests/v0_perf_kpi_ci_non_gating_structure.rs`,
  registered with a new `[[test]]` block in
  `crates/integration-suite/Cargo.toml`. The test pulls in **no** crate
  dependency (it uses only `std::fs`), so it adds nothing to `Cargo.lock` and
  cannot touch the public-API / SemVer gates (C8).

## Falsifiability note (the proven RED)

Each RED assertion FAILS against the committed `ci.yml` for the EXACT
falsifier the contract names:

- **#1 (US-01)** `gate_1_test_does_not_opt_into_wall_clock_perf_tests` —
  FAILS today because `gate-1-test` (`ci.yml:140-141`) DOES set
  `KALEIDOSCOPE_PERF_TESTS: "1"`. Panic message:
  *"the gate-1-test job must NOT set KALEIDOSCOPE_PERF_TESTS (US-01) ..."*
- **#2 (US-02)** `a_non_gating_perf_kpis_job_runs_the_wall_clock_family` —
  FAILS today because there is NO `perf-kpis` job in the file. Panic message:
  *"a `perf-kpis` job must exist in ci.yml (US-02) ..."*

Both are **RED, not BROKEN** (Mandate 7): the test compiles and links (it
reads an existing file; no production symbol is missing), and the assertions
fail behaviourally on the file content, not on a setup error.

### `#[ignore]`-until-DELIVER decision + evidence

- #1 and #2 carry `#[ignore = "RED until DELIVER: <reason>"]`; default
  `cargo test` is GREEN; `cargo test -- --ignored` shows them FAILING.
- DELIVER removes the two `#[ignore]` attributes when the `ci.yml` edit lands;
  the same four assertions then all pass on the edited file (no test logic
  change — only the `#[ignore]` lines are deleted).

**Proven-RED run evidence (2026-06-06):**

```
# default run — GREEN (controls pass, RED ignored)
running 4 tests
test a_non_gating_perf_kpis_job_runs_the_wall_clock_family ... ignored, RED until DELIVER: ...
test adr_0070_records_the_durable_op_honesty_note ... ok
test gate_1_test_does_not_opt_into_wall_clock_perf_tests ... ignored, RED until DELIVER: ...
test gate_1_test_still_runs_the_correctness_gating_invocation ... ok
test result: ok. 2 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out

# --ignored run — the two RED assertions FAIL against today's ci.yml
failures:
    a_non_gating_perf_kpis_job_runs_the_wall_clock_family
    gate_1_test_does_not_opt_into_wall_clock_perf_tests
test result: FAILED. 0 passed; 2 failed; 0 ignored; 0 measured; 2 filtered out
```

## The GREEN controls (guard against regression)

- **#3 (US-03; C2)** `gate_1_test_still_runs_the_correctness_gating_invocation`
  — un-ignored; asserts `gate-1-test` STILL runs
  `cargo test --workspace --all-targets --locked`. Passes today and MUST keep
  passing after DELIVER (the edit deletes only the env block; if it ever
  touched the invocation, this control reds). This is the structural form of
  the load-bearing negative control: de-gating perf must provably NOT de-gate
  correctness.
- **#4 (US-04)** `adr_0070_records_the_durable_op_honesty_note` — un-ignored;
  asserts ADR-0070 records the durable-op budgets as dev-indicative (not
  CI-contractual), attributes the cost to ADR-0049/0060 (not a regression),
  and forbids threshold-raising. A light docs-presence guard that the honesty
  record stays.

### The behavioural negative control (DELIVER/CI demonstration)

The brief calls for a behavioural demonstration that a deliberately failing
NON-perf correctness test still reds `gate-1-test` (US-03). The STRUCTURAL
form of that invariant is encoded here as control #3 (the gating invocation
is present and unchanged). The runtime demonstration — a throwaway-branch
correctness failure observed to red Gate 1 — is a CI act for the DELIVER wave
(it requires a pushed branch and a CI run), recorded here as the DELIVER
obligation, not performed at DISTILL (no spawned process at DISTILL per the
discipline).

## Constraints (all hold)

- **C1 / C3 / C8** — the test adds no dependency, no threshold literal, no
  durability change; it is a docs/CI-shape assertion only.
- **Mandate 7 (RED-not-BROKEN)** — proven above.
- **Discipline (trunk-green)** — default `cargo test` GREEN; RED behind
  `#[ignore = "RED until DELIVER: ..."]`; `--ignored` shows FAILING.
- **No spawned process** — the test only reads files; no aperture, no ports.
- **fmt / clippy clean** — `cargo fmt -p integration-suite` clean;
  `cargo clippy -p integration-suite --test
  v0_perf_kpi_ci_non_gating_structure` finished with no warnings.

## Peer-review verdict

acceptance-designer-reviewer applied as a structured self-review against the
critique-dimensions skill (nested sub-agent invocation unavailable in this
autonomous context; slim/structural-acceptance precedent). See the verdict
section in `distill/acceptance-test-scenarios.md`. Verdict: **APPROVED** —
0 blocking, 0 high. Iteration 1.
