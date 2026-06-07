# Evolution archive — speed-up-local-precommit-v0

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
`aegis-ingest-auth-v0-evolution.md`,
`spark-ingest-auth-v0-evolution.md`,
`perf-kpi-ci-non-gating-v0-evolution.md` and
`aperture-presubscriber-probe-stderr-v0-evolution.md`, which established
the per-file convention: one file per feature, named
`<feature-id>-evolution.md`, with the sections below. This is an INFRA
feature (a local pre-commit hook edit, a new CI-watch shell script, plus
documentation, no production source), so the record is deliberately
proportionate to that scope.

## Status

- State: DELIVERED and pushed on `main`.
- Wave model: full nWave (DISCUSS, DESIGN, DEVOPS, DISTILL, DELIVER),
  every wave dispatched to its own agent.
- ADR: ADR-0072
  (`docs/product/architecture/adr-0072-fast-local-precommit-deep-tests-in-ci.md`),
  which records the local-fast / CI-deep split and SUPERSEDES nothing.
  It is the direct LOCAL-side sibling of ADR-0070
  (`perf-kpi-ci-non-gating-v0`): ADR-0070 moved the perf wall-clock
  assertions off the CI build gate because durability fsync made them
  slow/flaky, but explicitly left the durability test bodies running both
  locally and CI-side, paying full fsync cost (ADR-0070 §7 "No local hook
  change"). This ADR is the missing local-side move: it takes the slow
  tests' LOCAL blocking off the commit path while leaving them gating in
  CI. Same trunk-based "CI is feedback, not a gate" framing, same
  honesty-trade posture, different lever and location.
- Closes: the slow local pre-commit hook Andrea flagged as unacceptable.
  The hook's Step 4 took 10-20 minutes per commit (once a multi-hour wedge
  under leaked-process contention), pressuring the maintainer toward
  `git commit --no-verify`, which silently drops EVERY gate. Andrea said
  he would accept roughly 5 minutes.

## Commit ledger (in order, on `main`)

| Wave / step | SHA | Subject |
|---|---|---|
| discuss | `c264a17` | a 5-minute local gate, deep tests in CI with eyes |
| design | `818027d` | hook runs `--lib`, deep `--all-targets` stays in CI |
| devops | `759db8b` | measured: the fast hook is well under 5 min |
| distill | `c4e5f03` | structural RED tests for the fast hook and the eyes |
| deliver | `7e628d7` | fast `--lib` local hook; deep `--all-targets` stays in CI |
| docs | `f84ed5f` | narrative + slide closure |

The DISCUSS, DESIGN, DEVOPS and DISTILL artefacts landed on `main` ahead
of DELIVER, each from its own wave agent; the as-built facts below are
read from the DELIVER commit `7e628d7`.

## The problem, in Earned-Trust framing

`scripts/hooks/pre-commit` runs five steps: a toolchain symmetry check
(Step 0), `cargo fmt` (Step 1), `cargo clippy --all-targets` (Step 2),
`cargo deny` (Step 3), and `cargo test --workspace --all-targets --locked`
(Step 4). Step 4 was the slow part. `--all-targets --workspace` compiles
and runs every integration test binary across the workspace: 165 of them
(`crates/**/tests/*.rs`), 26 of which are the fsync-heavy durability
family (cinder, pulse, lumen, ray, strata, sluice, beacon,
log-query-api), plus the subprocess suites that spawn real binaries.

The root cause is not a regression and not a wrong test. It is a now-slow
op gating a courtesy hook. `place()` and the WAL append path became
durable, per-record `sync_all` operations when ADR-0049
(`earned-trust-fsync-probe-v0`) and ADR-0060 (`store-fsync-durability-v0`)
landed. Durability is the point of the Earned-Trust lineage, not a defect.
But it made `cargo test --workspace --all-targets` disk-bound: the 26
durability bins and the subprocess spawns turned Step 4 into a 10-20
minute wait, and one prior commit's hook wedged for HOURS under
leaked-process contention.

The harm was structural. A gate that slow is a gate that gets skipped: the
wait trains the maintainer (human or crafter agent) toward
`git commit --no-verify`, which silently drops fmt, clippy, deny AND the
tests in one stroke. The worst outcome for "main is socially always green"
is not a slow gate; it is a gate so slow it is bypassed entirely. And the
deep run already DUPLICATED the authoritative CI gate: `gate-1-test` in
`.github/workflows/ci.yml` (ci.yml:182) already runs the identical
`cargo test --workspace --all-targets --locked` on every push. The local
Step 4 was a slow duplicate of a gate that already ran in CI, not unique
coverage.

## The decision lineage

### ADR-0072: split the gate by who waits

ADR-0072 chose the simplest honest cut: the local hook's Step 4 changes
from `cargo test --workspace --all-targets --locked` to
`cargo test --workspace --lib --locked`. `--lib` runs every crate's
in-`src/` `#[cfg(test)]` unit tests across all 26 crates and ONLY those:
no `tests/*.rs` integration binary, none of the 26 fsync-bound durability
bins, no subprocess bin, no doctest. The slow surface is excluded
DETERMINISTICALLY BY CONSTRUCTION, not by a fragile deny-list of binary
names that drifts as new `tests/*.rs` land. The deep `--all-targets` suite
was already in CI gate-1 and stays there, untouched. A new
`scripts/ci-watch.sh` plus a documented CLAUDE.md cadence is the safety
net that gives the deep tests eyes again now that the local wait no longer
provides them. The ADR supersedes nothing; it is the local sibling of
ADR-0070's perf-gating change.

### The alternatives, rejected

- Keep Step 4 as the full `--all-targets` run (status quo). Rejected: this
  IS the problem (10-20 min per commit, a wedged-for-hours incident, the
  training toward `--no-verify`). The deep run already duplicates CI
  gate-1, so the local block buys nothing CI does not already enforce.
- Curated `--all-targets`-minus-slow set via deny-list / `--exclude`.
  Rejected for fragility: it must be maintained against a 165-binary
  inventory that grows every feature; a new slow `tests/*.rs` bin is
  silently mis-classified as fast and re-introduces the creep, or a new
  fast bin is forgotten and never runs locally. `--lib` is deterministic
  and needs zero maintenance. Recorded as a measured successor if a
  specific fast suite earns a local slot.
- Delete the slow durability / subprocess tests. Rejected, hard: that
  discards the entire durability acceptance surface, the
  `sync_all`/torn-tail/crash coverage that is the point of the
  Earned-Trust durability lineage (ADR-0049/0060). De-gating locally is
  not deleting (US-03).
- Parallelism only (keep `--all-targets`, raise `--test-threads` /
  `--jobs`). Rejected as insufficient and risky: the slow suites are
  I/O-bound on per-record `sync_all`, not CPU-bound; more threads contend
  on the same disk and, under the prior leaked-process incident, WORSENED
  the wedge.
- Faster test-fsync backend now. Deferred, not rejected: it is the right
  long-term fix but a substantive change to the durability test substrate
  deserving its own feature; this feature is the cheap, immediate relief
  that does not block on it (Decision 6, see follow-ups).

## The as-built shape

- `scripts/hooks/pre-commit` Step 4 changed from `--all-targets` to
  `--lib`; the Step 4 header and echoed summary lines were updated to
  match the narrower scope. Steps 0 (toolchain), 1 (fmt), 2 (clippy
  `--all-targets`, UNCHANGED), and 3 (deny) are untouched. The Decision-2
  clippy trim to `--lib` was NOT triggered (the measurement came in well
  under budget, so clippy stays `--all-targets`).
- New `scripts/ci-watch.sh` (chmod +x): a probe-first `gh` wrapper that
  reports the latest `main` run's conclusion, URL and short SHA and, on a
  red, classifies the failed jobs, explicitly name-checking `gate-1-test`
  (deep tests) and `gate-5-mutants*` (mutation), the two deep gate
  families the slim local hook no longer pre-runs. Earned-Trust honest
  degradation: it probes `gh` present then authed then reachable and exits
  non-zero with a remediation message rather than EVER printing a false
  green. Exit semantics are poll-loop scriptable (0 success / 0
  in-progress / 1 red / 1 unknown). Verified live: reported the latest
  `main` run in_progress, exit 0.
- CLAUDE.md gains a `## CI watch` section: what the script does, the
  cadence (run after every push to main + poll periodically while working;
  detect a deep-only regression within one cadence interval; fix-forward),
  and the honest-degradation note.
- The 2 previously-RED structural acceptance scenarios in
  `crates/integration-suite/tests/v0_fast_precommit_structure.rs` were
  un-ignored and are now green; the 2 GREEN controls stayed green. All 4
  pass, 0 ignored.
- `.github/workflows/ci.yml` `gate-1-test` is UNCHANGED (still
  `cargo test --workspace --all-targets --locked`, ci.yml:182). No test
  file is deleted (the `tests/*.rs` count stays 174). No `crates/*/src`
  source, no test body, no `Cargo.toml`/`Cargo.lock` change, no crate
  version change.

## The proof and its honest measurement

- The acceptance is STRUCTURAL, in
  `crates/integration-suite/tests/v0_fast_precommit_structure.rs`: 4 tests
  green, 0 ignored. DELIVER un-ignored the two RED tests (the hook reads
  `--lib`; `ci-watch.sh` exists) and the two GREEN controls stayed green
  (the deep CI gate is preserved; no test is deleted). Mirrors the
  ADR-0070 structural precedent
  (`v0_perf_kpi_ci_non_gating_structure.rs`).
- The `--lib` exclusion is empirically confirmed, not asserted:
  `cargo test --workspace --lib` emits 0 `Running tests/` lines and 26
  `unittests src` lines, i.e. it runs none of the 165 integration /
  durability / subprocess bins by construction.
- MEASURED wall-clock (warm, leaked procs swept, cargo 1.88.0): fmt 0.53s
  + clippy `--all-targets` 6.84s + deny 0.89s + `--lib` 1.52s, end-to-end
  real hook ~3s. The worst-case foundational-crate edit was measured at
  3m12s in the DEVOPS wave. The <= 5 min bar (US-01 timing AC): PASS with
  large margin, down from 10-20 min.
- The DELIVER commit's OWN pre-commit gate ran the new `--lib` hook and
  finished in ~3s: the fix proved itself on the very commit that landed it.
- A measurement-honesty note worth keeping: the DESIGN wave could NOT
  measure the seconds, because the solution-architect agent in that
  harness had no shell-execution tool (Read/Write/Edit/Glob/Grep only).
  Rather than fabricate seconds, DESIGN declared the gap and deferred the
  <= 5 min confirmation to DEVOPS/DELIVER, who have a shell and ran the
  numbers on real hardware. A test-don't-assume win: the bar was a
  DELIVER-confirmed measurement, not a DESIGN guess.

## The honesty trade, recorded citably

With the deep tests off the local blocking path, a local commit CAN reach
`main` carrying a deep-only regression (a durability / snapshot /
torn-tail / crash / subprocess / integration break) that the fast local
hook did not run. That regression is caught by CI `gate-1-test` (and
`gate-5-mutants`), plus the Decision-3 watch cadence, then fix-forwarded,
not stopped at commit. This is acceptable under the trunk-based "CI is
feedback, not a gate" posture
(`project_kaleidoscope_pure_trunk_based`), PROVIDED the cadence is real,
which Decision 3 makes it (a one-command script plus a written cadence).
It is the same trade ADR-0070 accepted for the perf signal: visible
feedback over a local block that costs more than it is worth. The
difference is the failure class (correctness/durability here, perf there)
and the catch mechanism (CI gate-1 + cadence here; non-gating `perf-kpis`
job + human trend there). The fix-forward posture
(`feedback_fix_forward_post_merge_correction`) is the remediation path
when the cadence surfaces a red.

## The boundary

- Durability was NOT weakened: no `sync_all` removed, no durability test
  body changed.
- The CI deep gate was NOT weakened: `gate-1-test` still runs the full
  `--all-targets` suite on every push; it is now the single home for deep
  gating, not a duplicate.
- No test was deleted (`tests/*.rs` count unchanged at 174). No threshold
  was raised.
- ONLY WHERE the slow tests gate changed: off the local commit path, kept
  in CI. SemVer (Gate 2 / Gate 3): none; no crate version bump; never
  1.0.0 (CLAUDE.md; `semver_one_zero_is_andreas_call`).

## Note for the operator

This feature adds no deployment precondition and changes no runtime
behaviour. Its only consequences are at the developer/CI boundary: a local
`git commit` now runs `toolchain + fmt + clippy + deny + --lib` in seconds
instead of 10-20 minutes, and a new courtesy command `scripts/ci-watch.sh`
summarises the latest `main` CI run and surfaces `gate-1-test` and
`gate-5-mutants` reds. The cadence (run after every push, poll while
working) lives in CLAUDE.md `## CI watch`. A deep-only red on CI is a
signal to fix-forward, not a merge blocker.

## The lesson

A gate slow enough to be skipped is not a gate. The 10-20 minute Step 4
did not protect `main`; it pressured the maintainer toward `--no-verify`,
which protects nothing. The honest fix was neither to delete the
durability coverage nor to weaken CI: it was to split the gate by who
waits, running the fast unit subset where a human stands at the keyboard
and keeping the slow deep suite where a machine can wait, with an explicit
watch so the deep tests still have eyes. And the quiet irony: delivering
the slow-commit fix meant living through several slow commits to land it.
Every wave commit through DISTILL ran the OLD `--all-targets` hook and paid
the full 10-20 minute wait; only the DELIVER commit ran the new `--lib`
hook and finished in 3 seconds. The cure was the first commit to enjoy it.

## Known follow-ups (open, carried forward across the project)

These are open across the project and carried forward; this feature
neither introduced nor closed them except where noted. The slow local
pre-commit hook Andrea flagged is CLOSED by this feature.

1. faster-test-fsync-backend-v0 (this feature's own carried-forward).
   This feature speeds the LOCAL gate, not the durability tests
   themselves. The 26 fsync-bound bins remain I/O-bound IN CI, paying the
   honest per-record `sync_all` of ADR-0049/0060 (that cost is the
   durability, not a defect). A future feature could speed them with a
   faster test-fsync backend or a batched-fsync test mode behind an env
   guard, mirroring the ADR-0058 guard pattern. Flagged in ADR-0072
   Decision 6; explicitly NOT fixed here. Open.

2. perf-KPI print-on-PASS. Today the p95 number prints only on a breach
   (the assert message). Emitting it on every run would need a uniform
   `eprintln!` before each of the 28 asserts, a 28-file edit deferred to
   keep the test bodies untouched. Open only if the maintainer wants the
   trend visible on green runs.

3. dedicated/self-hosted perf runner. A controlled runner could honour the
   durable-op budgets and restore a trustworthy perf gate, at the cost of
   a self-hosted runner the project deliberately avoids. Recorded as
   available future work, rejected for v0.

4. read-path auth (the next aegis wire). The query / log-query /
   trace-query read APIs are still unauthenticated; aperture-storage-sink
   reaches through `.inner` and read-path tenant authority is deferred.
   Open.

5. ingest role-gating. ingest auth is authentication-only: any valid
   catalogued token may ingest. Rejecting a valid `viewer` on the write
   path is the deferred authorization decision; the `TenantContext.role`
   is already threaded, so the follow-up is one
   `if ctx.role != Operator { reject }` gate with no re-plumbing. Open.

6. aegis "JWKS"-vs-HS256 doc-fix. `aegis/src/lib.rs` overstates "JWKS";
   the validator is HS256 pre-shared-key only. Disposition: a `docs:`
   fix-forward or a trivial micro-wave. Open.

7. sluice nack-past-cap. sluice's behaviour when a write is nacked past
   its cap needs its own slice. Open.

8. sluice wiring. sluice remains UNWIRED: no gateway/server `src` path
   constructs or drives `FileBackedQueue`. The wiring is a separate,
   still-open slice. Open.

9. sluice torn-tail migration. sluice still carries the inline
   parse-or-die recovery loop; its migration to the shared
   `replay_wal_tolerating_torn_tail` routine is the tracked ADR-0059 §5
   follow-up. Open.

10. ingest-dedup-v0. A re-run of a SUCCESSFUL, fully-valid ingest still
    doubles the store, because lumen has no idempotency key. The designed
    extraction (ADR-0064 DD-3): success-case dedup earns its own slice.
    Open.

11. ingest-bounded-memory. The buffer-all-then-flush design (ADR-0064)
    holds the whole input's records in RAM before commit. A future feature
    lifts it with a temp-WAL staging stage or a max-records streaming cap.
    Open.

12. ADR-0059 Decision 8 layer b, the AST structural check, remains
    UNWIRED. The structural pre-commit check asserting in-scope stores
    delegate to the shared wal-recovery routine and carry no `let _ =`
    swallow; the tool choice was deferred and remains deferred. It is
    feedback, not a gate, consistent with the pure trunk-based,
    no-required-checks posture; when wired it belongs in the local
    pre-commit stage (now the fast `--lib` stage). Open.

13. OTLP partial_success never populated. The OTLP `partial_success`
    response field is never populated, so partial-accept signalling is not
    surfaced to clients. Open.

14. The two claims-honesty DOCUMENT items remain future features if
    wanted. The actual Prometheus-stepped grid for `query_range` (a
    query-api feature) and real gRPC-prefix honouring for `harness`
    (`Framing::GrpcProtobuf`) were documented as v0 reality rather than
    built; each would retire its respective pin. Open only if wanted.

15. beacon non-30d error budget periods. v0 supports ONLY a 30d error
    budget period. Other windows (7d, 90d) would each need their own
    `MWMBR_TABLE` row set and earn their own slice. Open only if wanted.
</content>
</invoke>
