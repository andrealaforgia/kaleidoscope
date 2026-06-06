# Evolution archive — perf-kpi-ci-non-gating-v0

British English. No em dashes. This is the archival evolution record for
the feature. It is the factual ledger of what changed, why, and what is
left open. The narrative prose for this feature lives in
`docs/presentation/narrative.md`; this file does not duplicate it.

Sibling to `wal-torn-tail-recovery-v0-evolution.md`,
`store-fsync-durability-v0-evolution.md`,
`tls-config-reject-v0-evolution.md`,
`claims-honesty-pass-v0-evolution.md`,
`beacon-sighup-reload-v0-evolution.md`,
`cli-ingest-atomic-v0-evolution.md`,
`cinder-wal-error-surfacing-v0-evolution.md`,
`aperture-serve-loop-error-surfacing-v0-evolution.md`,
`beacon-slo-operator-path-v0-evolution.md`,
`aegis-ingest-auth-v0-evolution.md` and
`spark-ingest-auth-v0-evolution.md`, which established the per-file
convention: one file per feature, named `<feature-id>-evolution.md`, with
the sections below. This is an INFRA feature (CI workflow plus
documentation, no production source), so the record is deliberately
proportionate to that scope.

## Status

- State: DELIVERED and pushed on `main`.
- Wave model: full nWave (DISCUSS, DESIGN, DEVOPS, DISTILL, DELIVER),
  every wave dispatched to its own agent.
- ADR: ADR-0070
  (`docs/product/architecture/adr-0070-perf-kpi-non-gating-ci.md`), which
  SUPERSEDES ADR-0058 §3 (the CI-gating clause) while PRESERVING ADR-0058's
  guard mechanism and its no-threshold-chasing stance, and which records
  the durable-fsync cause ADR-0058 did not foresee.
- Closes: the CI failure Andrea flagged, where
  `place_p95_latency_under_two_hundred_microseconds` (and the rest of the
  28-test wall-clock perf-KPI family) reds the build-gating Gate 1 on
  shared CI hardware, training the team to discount red builds. The fix is
  at the gating layer, not by hiding the perf reality.

## Commit ledger (in order, on `main`)

| Wave / step | SHA | Subject |
|---|---|---|
| deliver | `0d5e8bc` | de-gate wall-clock perf, add non-gating `perf-kpis` job |
| docs | `1eda9d9` | narrative + slide closure |

The DISCUSS, DESIGN, DEVOPS and DISTILL artefacts landed on `main` ahead
of DELIVER, each from its own wave agent; the as-built facts below are
read from the DELIVER commit `0d5e8bc`.

## The problem, in Earned-Trust framing

Andrea flagged that `place_p95_latency_under_two_hundred_microseconds` was
failing on CI. The root cause is not a regression and not a wrong budget:
it is a wrong GATE. `place()` became a durable, per-record `fsync`
operation when ADR-0049 (pulse WAL `sync_all`) and ADR-0060 (the same
per-record `sync_all` plus atomic snapshot generalised across all seven
file-backed stores, `cinder.place` and `sluice.enqueue` included), plus
the write-ahead work, landed AFTER ADR-0058 was accepted. The 200 us p95
budget was written for a NON-DURABLE `place`. On GitHub Actions shared,
virtualised storage, fsync p95 is routinely milliseconds, so the budget is
unreachable for a now-durable op. The operation became durable underneath
the budget, and the gate did not move with it.

The harm was structural, not numeric. ADR-0058's §3 set
`KALEIDOSCOPE_PERF_TESTS=1` in the build-gating `gate-1-test` job, so the
whole 28-test wall-clock perf-KPI family HARD-GATED the build. A perf
breach turned `gate-1-test` red and was indistinguishable, on the run
page, from a real correctness regression. A build that flakes red on
hardware variance trains the maintainer and contributors to mentally
discount red builds: the very habit that destroys a CI signal under pure
trunk-based development, where `main` has no required-status-checks and CI
is feedback, not a gate (`project_kaleidoscope_pure_trunk_based`).

## The decision lineage

### ADR-0070 supersedes only ADR-0058's gating clause

ADR-0070 opens by quoting ADR-0058 §3 verbatim and superseding it. The
clause it reverses is the GATING decision (perf rides inside `gate-1-test`);
what it PRESERVES is everything else ADR-0058 got right: the
presence-based `KALEIDOSCOPE_PERF_TESTS` self-skip guard at all 28 test
sites, and the explicit rejection of threshold-raises as the fix. The ADR
also records the durable-fsync root cause ADR-0058 could not have foreseen,
because ADR-0049 and ADR-0060 landed later. ADRs in this repository are
immutable, so ADR-0058 is superseded, not edited; ADR-0049, ADR-0060 and
ADR-0005 (the five-gate contract) are cited as precedents and NOT modified.
ADR-0070 adds a non-gating job ALONGSIDE the five gates; it does not add or
amend a sixth gate.

### The fix is CI-gating, never a threshold-raise

Per the standing project guidance (`project_p95_wallclock_flakes_overnight`
forbids threshold-raises) and ADR-0058's still-valid rejection, the budget
literal is correct for the op's intent and the guard mechanism is correct.
The only thing wrong was WHERE and HOW the perf tests run. The fix is the
gating semantics and the location, nothing else. This matches the
trunk-based posture: a wall-clock perf KPI on shared CI hardware is exactly
the kind of signal that should be visible feedback, not a blocking gate.

## The as-built shape

### Gate 1 stops opting in; the family self-skips there

The two-line job-level `env` block (`env:` plus its single
`KALEIDOSCOPE_PERF_TESTS: "1"` entry) is DELETED from `gate-1-test`. With
the variable absent, every one of the 28 wall-clock KPI tests hits the
ADR-0058 early-return preamble and self-skips with no measurement taken.
The gating invocation `cargo test --workspace --all-targets --locked` is
UNCHANGED, so every non-perf test still executes and asserts. `gate-1-test`
now goes green iff the correctness suite passes: a red Gate 1 means a
correctness regression and nothing else. One env key removed de-gates all
28 tests at once.

### A new non-gating `perf-kpis` job

A new `perf-kpis` job (sibling of `gate-1-test`, `needs: gate-4-deny`) is
added to `ci.yml`. It is marked `continue-on-error: true`: the job RUNS, a
perf assertion failure marks the JOB with a red X visible on the run page,
but the overall workflow conclusion stays success and nothing downstream is
blocked. It sets its own job-level `KALEIDOSCOPE_PERF_TESTS: "1"` as a
hardcoded literal (per the ADR-0058 §3 note on the GitHub Actions
job-level env evaluation quirk, the same reason gates 2/3 inline the
nightly pin), and runs the identical `cargo test --workspace
--all-targets --locked` invocation, mirroring `gate-1-test`'s pinned
checkout/toolchain/cache step SHAs and the shared cargo-stable cache. The
env-var presence runs the WHOLE guarded family across all 11 crates, so a
future guarded test is picked up for free with no per-test enumeration.

### Asserts kept; the honesty note in the ADR

The tests stay ASSERTING, not converted to a report-only logging mode. The
asserts ARE the KPI definition, and the existing assert message already
prints the measured p95 on a breach
(`"...got {p95_us} µs ..."`), so visibility-on-breach holds with NO test
change. Print-on-PASS is recorded as a deferred successor (it would need an
`eprintln!` in each of the 28 tests, contradicting the no-test-edit
constraint). The durable-op honesty note lives once, citably, in ADR-0070
§6: the durable-op budgets (`place` 200 us, `enqueue` 300 us, the WAL
`ingest` family) reflect per-record fsync cost since ADR-0049/0060 and are
DEV-INDICATIVE, not CI-contractual; a breach on shared CI storage reads as
honest durable-fsync cost, not a regression. In-memory budgets
(`cinder get_tier`, `augur observe`, `sluice enqueue_and_dequeue`) are NOT
caveated and remain legitimate even on CI. The local pre-commit hook is
UNCHANGED, because it never set the variable: the perf tests already
self-skip locally, and adding the variable would re-introduce the local
flake ADR-0058 fixed.

## The proof and its boundary

- The acceptance is STRUCTURAL, in
  `crates/integration-suite/tests/v0_perf_kpi_ci_non_gating_structure.rs`:
  4 tests green. DELIVER un-ignored the two RED tests
  (`gate_1_test_does_not_opt_into_wall_clock_perf_tests`,
  `a_non_gating_perf_kpis_job_runs_the_wall_clock_family`); the two GREEN
  controls stayed green: `gate_1_test_still_runs_the_correctness_gating_invocation`
  (the negative control proving de-gating perf did NOT de-gate correctness)
  and `adr_0070_records_the_durable_op_honesty_note`. All 4 pass, 0
  ignored.
- `ci.yml` was validated well-formed via `pyyaml` after the two edits
  (delete the gate-1 env block; add the `perf-kpis` job).
- Mutation (ADR-0005 Gate 5): N/A. There is no `crates/*/src` change and no
  test-body change beyond the env lever, so there is no production surface
  to mutate; the eleven `gate-5-mutants-*` `--in-diff` jobs path-filter on
  `crates/<name>/**` and short-circuit on an empty diff.
- SemVer (Gate 2 / Gate 3): no crate version change. The feature touches
  `.github/workflows/ci.yml` and documentation only; Gate 2
  (`cargo public-api`) and Gate 3 (`cargo semver-checks`) see no surface
  change. No crate bumped; never 1.0.0 (CLAUDE.md;
  `semver_one_zero_is_andreas_call`).
- The boundary: durability was NOT weakened (no `sync_all` removed), the
  correctness gating was NOT loosened (`cargo test --workspace` still
  reds Gate 1 on a real break), no threshold was raised, and no test was
  deleted. ONLY where and how the perf tests run changed.

## The lesson

A failing test must always mean something. The moment a red light lies,
every red light afterwards is easier to ignore, and on a pure trunk-based
project where CI is feedback rather than a hard gate that erosion is the
whole cost. The honest fix was not to make the durable-op budget pass on
CI by inflating it, and not to delete the perf coverage: it was to move the
wall-clock KPIs off the build gate onto a tracked, non-gating signal, so a
red `gate-1-test` once again means exactly one thing. This fixes Andrea's
flagged CI failure at the gating layer, not by hiding the perf reality: the
durable cost is recorded honestly in ADR-0070, the budgets are kept as
dev-indicative targets, and the breach stays visible as a non-blocking red
X one click away.

## Note for the operator

This feature adds no deployment precondition and changes no runtime
behaviour. Its only operational consequence is a CI run-page change: the
28 wall-clock perf KPIs no longer red `gate-1-test`; they run in the
separate, non-gating `perf-kpis` job, which can show a red X on a perf
breach while the overall workflow conclusion stays success. A red X on
`perf-kpis` is a signal to READ (follow it to the assert message's printed
p95 and to ADR-0070 §6), not a merge blocker. A durable-op breach on shared
CI hardware is expected fsync cost, not a regression; the correct response
is never a threshold-raise.

## Known follow-ups (open, carried forward across the project)

These are open across the project and carried forward; this feature
neither introduced nor closed them except where noted. The CI-gating defect
Andrea flagged is CLOSED by this feature.

1. perf-KPI print-on-PASS. Today the p95 number prints only on a breach
   (the assert message). Emitting it on every run would need a uniform
   `eprintln!("{kpi} p95 = {p95_us} µs")` before each of the 28 asserts,
   a 28-file edit deferred to keep the test bodies untouched. Open only if
   the maintainer wants the trend visible on green runs.

2. dedicated/self-hosted perf runner. A controlled runner could honour the
   durable-op budgets and restore a trustworthy gate, at the cost of a
   self-hosted runner (operational cost, security surface) the project
   deliberately avoids. Recorded as available future work, rejected for v0.

3. read-path auth (the next aegis wire). The query / log-query /
   trace-query read APIs are still unauthenticated; aperture-storage-sink
   reaches through `.inner` and read-path tenant authority is deferred.
   Open.

4. ingest role-gating. ingest auth is authentication-only: any valid
   catalogued token may ingest. Rejecting a valid `viewer` on the write
   path is the deferred authorization decision; the `TenantContext.role`
   is already threaded, so the follow-up is one
   `if ctx.role != Operator { reject }` gate with no re-plumbing. Open.

5. aegis "JWKS"-vs-HS256 doc-fix. `aegis/src/lib.rs` overstates "JWKS";
   the validator is HS256 pre-shared-key only. Disposition: a `docs:`
   fix-forward or a trivial micro-wave. Open.

6. sluice nack-past-cap. sluice's behaviour when a write is nacked past its
   cap needs its own slice. Open.

7. sluice wiring. sluice remains UNWIRED: no gateway/server `src` path
   constructs or drives `FileBackedQueue`. The wiring is a separate,
   still-open slice. Open.

8. sluice torn-tail migration. sluice still carries the inline
   parse-or-die recovery loop; its migration to the shared
   `replay_wal_tolerating_torn_tail` routine is the tracked ADR-0059 §5
   follow-up. Open.

9. ingest-dedup-v0. A re-run of a SUCCESSFUL, fully-valid ingest still
   doubles the store, because lumen has no idempotency key. The designed
   extraction (ADR-0064 DD-3): success-case dedup earns its own slice.
   Open.

10. ingest-bounded-memory. The buffer-all-then-flush design (ADR-0064)
    holds the whole input's records in RAM before commit. A future feature
    lifts it with a temp-WAL staging stage or a max-records streaming cap.
    Open.

11. ADR-0059 Decision 8 layer b, the AST structural check, remains
    UNWIRED. The structural pre-commit check asserting in-scope stores
    delegate to the shared wal-recovery routine and carry no `let _ =`
    swallow; the tool choice was deferred and remains deferred. It is
    feedback, not a gate, consistent with the pure trunk-based,
    no-required-checks posture; when wired it belongs in the local
    pre-commit stage. Open.

12. OTLP partial_success never populated. The OTLP `partial_success`
    response field is never populated, so partial-accept signalling is not
    surfaced to clients. Open.

13. The two claims-honesty DOCUMENT items remain future features if wanted.
    The actual Prometheus-stepped grid for `query_range` (a query-api
    feature) and real gRPC-prefix honouring for `harness`
    (`Framing::GrpcProtobuf`) were documented as v0 reality rather than
    built; each would retire its respective pin. Open only if wanted.

14. beacon non-30d error budget periods. v0 supports ONLY a 30d error
    budget period. Other windows (7d, 90d) would each need their own
    `MWMBR_TABLE` row set and earn their own slice. Open only if wanted.
