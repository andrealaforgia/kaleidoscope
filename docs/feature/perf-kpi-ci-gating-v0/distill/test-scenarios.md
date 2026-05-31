# Test Scenarios: perf-kpi-ci-gating-v0

Author: `nw-acceptance-designer` (Scholar), DISTILL wave, 2026-05-31.
British English; no em dashes in body text.

These are behavioural acceptance checks (DISTILL decision D1, option B). No
`.feature` file, step definitions, or new Rust test code are produced: the
feature is a mechanical guard added to 28 existing tests plus one CI env block,
and the observable outcome is verified by running cargo two ways and inspecting
the diff, grep, and ci.yml. Crafty executes these checks by hand in DELIVER
after applying the atomic change.

The verification commands below mirror
`design/application-architecture.md` section "Verification". Run all commands
from the workspace root.

## Acceptance scenarios

| id | Description | Command | Expected |
|----|-------------|---------|----------|
| AC-01 | Local skip: variable absent means every perf test skips and passes (US-01, US-04) | `cargo test --workspace` (no `KALEIDOSCOPE_PERF_TESTS` set) | All 28 perf tests print `perf test skipped: set KALEIDOSCOPE_PERF_TESTS=1 to run` to stderr and report `ok`; no `Instant` measurement runs; workspace run exits 0, even under parallel-build load. No `--no-verify` needed. |
| AC-02 | CI run: variable present means the confirmed flaker runs and enforces (US-01 example 2, US-02) | `KALEIDOSCOPE_PERF_TESTS=1 cargo test -p lumen --test v1_slice_01_wal_durability` | `ingest_p95_latency_under_three_milliseconds` performs its full 1000-sample measurement and asserts `p95 <= 3_000`; no skip note appears for it; test passes on an idle machine. |
| AC-03 | Thresholds unchanged: only the guard preamble is added (US-03) | `git diff` over the 27 gated test files | Per test, the ONLY added lines are the four-line guard preamble at the top of the body. No threshold literal (`3_000`, `10_000`, and the rest), sample count, warm-up loop, or percentile index (`samples[950]`, `samples[190]`, and the rest) is changed. |
| AC-04 | Complete coverage: all 28 tests gated, no straggler (US-04) | `grep -rl "KALEIDOSCOPE_PERF_TESTS" crates/{lumen,pulse,ray,strata,cinder,sluice,beacon,augur,aegis}/tests` for file count; `grep -rho "KALEIDOSCOPE_PERF_TESTS" crates/{lumen,pulse,ray,strata,cinder,sluice,beacon,augur,aegis}/tests \| wc -l` for occurrence count | `grep -rl` lists 27 distinct files (beacon's two perf tests share one file); the occurrence count is 28. No functional (non-temporal) test is altered; `aperture slice_06_forwarding_sink` is NOT in the result set. |
| AC-05 | CI env set: gate-1-test job opts in with a literal value (US-02) | inspect `.github/workflows/ci.yml` gate-1-test job | The `gate-1-test` job has a job-level `env:` block setting `KALEIDOSCOPE_PERF_TESTS: "1"` (hardcoded literal, NOT `${{ env.X }}`), placed between `needs: gate-4-deny` and `steps:`. No other gate job (2, 3, 4, 5) is changed; the pre-commit hook is NOT modified. |

## Notes on the two primary commands (D3)

- **AC-01 is the local-skip outcome.** It demonstrates US-01 (hook skips) and
  US-04 (uniform coverage) at once: the whole workspace run is deterministic
  because every gated test returns early. The stderr note is the observable
  signal that the guard fired.

- **AC-02 is the opt-in / CI run-and-enforce outcome.** It demonstrates that
  presence of the variable restores the full measurement and the unchanged
  threshold. Running it scoped to the single confirmed flaker
  (`lumen v1_slice_01_wal_durability`) keeps the check fast and targeted; the
  same semantics apply to all 28 when CI sets the variable workspace-wide.

## Self-review checklist (adapted for this atypical feature)

- [x] 1. WS strategy declared: N/A, recorded in wave-decisions.md D4 (no
      adapter, no I/O port, no walking skeleton).
- [x] 2. WS scenarios tagged correctly: N/A, no WS scenario exists.
- [x] 3. Every driven adapter has a `@real-io` scenario: N/A, zero new driven
      adapters.
- [x] 4. InMemory doubles documented for what they cannot model: N/A, no
      doubles introduced.
- [x] 5. Container preference documented: N/A, no services.
- [x] 6. Mandate 7 scaffold files for imported production modules: N/A, no new
      production module; see wave-decisions.md D2.
- [x] 7. Mandate 7 `__SCAFFOLD__`/`SCAFFOLD: true` marker present: N/A, no
      scaffold (D2).
- [x] 8. Mandate 7 scaffold methods raise assertion error: N/A, no scaffold
      (D2).
- [x] 9. Mandate 7 tests RED not BROKEN against scaffolds: ADAPTED. The red
      state is behavioural (hook flakes under load pre-DELIVER); the green
      state is deterministic skip locally plus enforced run in CI. See
      wave-decisions.md "RED/GREEN framing".
- [x] 10. Driving adapter CLI/endpoint/hook exercised via its protocol: N/A,
      no driving adapter; entry is `cargo test` invocation, covered by AC-01
      and AC-02.
- [x] 11. `@real-io @adapter-integration` per driven adapter: N/A, no adapter.
- [x] Coverage: 28 of 28 tests listed and cross-checked against
      `design/application-architecture.md` Test inventory (AC-04).
- [x] Thresholds untouched: AC-03 enforces diff shows guard preamble only.
- [x] ci.yml env: AC-05 enforces the job-level literal env block on gate-1-test.
- [x] Business-language note: this is infrastructure tooling, not an end-user
      domain; the "user" is the maintainer and the surfaces are the pre-commit
      hook and the CI job. Acceptance checks are expressed as commands and
      observable outcomes rather than Gherkin, matching the feature's nature.

## Out of scope (explicitly not gated, must stay unchanged)

- `aperture slice_06_forwarding_sink::forwarding_sink_accepted_event_includes_downstream_latency_ms_field`
  asserts a field is present, not a wall-clock threshold.
- Any test using `Instant`/`elapsed()` only for timeout or wait-until-ready
  scaffolding with no p95 threshold assertion.
- Any test asserting functional recovery correctness (identical state) rather
  than recovery latency.
