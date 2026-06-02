# Wave Decisions — wal-torn-tail-recovery-v0 (DISTILL)

British English. No em dashes in body.

## Wave: DISTILL (Quinn, nw-acceptance-designer)

## Autonomous run note

Autonomous overnight run. All interactive decisions were made by the
agent per the standing instruction; nothing was deferred to the user.
DISTILL only; does NOT proceed into DELIVER.

## DISTILL Wave Decisions (DWD)

### DWD-1: I-O strategy = Strategy C (real local I/O)

The walking skeleton and every scenario use REAL local I/O: real WAL
files on a real tmp directory (`std::env::temp_dir()` + PID + nanos, the
established v1 file-backed convention), real `open` reopen, a real child
process for the binary path, and a real TCP query. No external services,
no containers, no filesystem test doubles. The torn tail is seeded by
writing real partial-JSON bytes with NO trailing newline directly to the
on-disk WAL, exactly the residue a `kill -9` between `write_all(bytes)`
and `write_all(b"\n")` leaves (ADR-0049 / ADR-0059 Context). All such
scenarios are tagged `@real-io`.

Rationale: the behaviour under test IS a filesystem-recovery behaviour;
an in-memory double could not catch the trailing-byte `ends_with_newline`
discrimination, the line-index-vs-last comparison, or the WAL path
resolution that the feature turns on. Strategy C is the only honest
choice. No `@in-memory` walking skeleton exists (Dim 9 clean).

No new dev-dependency (`tempfile`) was introduced: the established pillar
convention uses `std::env::temp_dir()` with manual cleanup, and a new
crate would touch `Cargo.lock` and risk Gate 4 churn for zero benefit.

### DWD-2: AC-7 (cinder doc) recorded as a DELIVER-verified doc criterion

AC-7 (the cinder module doc at `crates/cinder/src/file_backed.rs:36-38`
and the `open` doc at `:104-106` no longer make the false "detected and
ignored" claim) is NOT asserted as a runtime test. A unit test that
string-matched doc-comment prose would be brittle Fixture Theater
(asserting the implementation's comments, not behaviour). Instead AC-7 is
discharged two ways: (a) the BEHAVIOUR the corrected doc describes is
proven by cinder's `reopen_recovers_the_intact_prefix_after_a_torn_tail`
(torn final line dropped with a warning) plus its two negatives
(every other parse failure surfaced as `PersistenceFailed`); (b) the
crafter corrects the prose in the same DELIVER commit and the reviewer
reads the corrected doc against AC-1..AC-6 (ADR-0059 Verification; brief
"For Acceptance Designer" AC-7). This is the agent's call, stated
explicitly per the instruction.

### DWD-3: AC-1 headline driven through the compiled binary, store-reopen as the per-pillar port

The verifier-D04 headline (AC-1 + AC-3) is driven through the COMPILED
`log-query-api` binary launched as a child process with a real HTTP query
(`crates/log-query-api/tests/slice_08_torn_tail_recovery.rs`), because
`CARGO_BIN_EXE_log-query-api` is set only for that crate's tests and the
operator-visible behaviour is the binary recovering, binding, and
serving. The per-pillar coverage (ray, cinder, pulse, and lumen's own
N>=1 and negatives) is driven through the store-reopen driving port
(`FileBacked*Store::open` then a trait read), which is also a primary
port: the operator-visible behaviour is the store starting and serving
the durable prefix. No test enters the internal `crates/wal-recovery`
function (it does not exist yet and is a driven leaf).

### DWD-4: structured WARN assertion via stderr-grep subprocess, not an in-process subscriber

AC-3 is asserted by spawning the binary, draining its stderr on a
dedicated thread under a wall-clock deadline, and parsing each line as
JSON for `event="wal.recovery.torn_tail_dropped"` with `pillar`, `line`,
`dropped_bytes` (the exact shape `log-query-api` slice 07 established for
`health.startup.refused`). The subscriber is process-global and
`try_init`-guarded and writes to the real stderr fd, so an in-process
test cannot observe it; only a spawned process can. The line number
assertion (line 4 = three acked records then the torn tail) pins the
1-based `idx+1` convention ADR-0059 Decision 3 mandates; the
`dropped_bytes` assertion pins the byte length of the seeded torn line.

### DWD-5: RED-not-BROKEN, all scenarios #[ignore]d, zero scaffold needed

All 15 scenarios are `#[ignore = "RED until DELIVER: wal-torn-tail-
recovery-v0 slice NN ..."]`. Confirmed by `cargo test --workspace
--all-targets --locked` returning exit 0 with zero live failures (the
pre-commit hook stays green; `--no-verify` is never used). NO scaffold
was required: every test compiles against today's public APIs. Mandate 7
RED-ready scaffolding (`panic!("__SCAFFOLD__")` + `// SCAFFOLD: true`) was
NOT needed and is absent. DELIVER removes the `#[ignore]` one scenario at
a time (Outside-In); the natural order is the priority-rationale order
from `story-map.md`: AC-1 positive first (lumen reopen, then the binary
D04), then AC-5/AC-6 negatives, then ray AC-4 and pulse cardinality,
across the four pillars.

## Soft gates

- **KPI contracts**: checked. The feature's single emittable signal is
  the structured WARN, covered by AC-3; K1/K2/K4/K5 map onto the AC-1 /
  AC-5 / AC-6 / AC-7 scenarios. No `@kpi`-only scenario warranted. No
  warning.
- **DISCUSS delta present**: yes (`user-stories.md`, `story-map.md`,
  `outcome-kpis.md`). Full story traceability.
- **DEVOPS delta present**: yes (`environments.yaml`, DEVOPS
  `wave-decisions.md`). Environments `clean` + `ci`, no deploy surface,
  both run the same `cargo test --workspace` validation.

## Handoff to DELIVER (software-crafter)

- 15 acceptance scenarios across 5 files, all `#[ignore]`d and RED-ready.
- Walking skeleton = the binary D04 path
  (`operator_restart_serves_the_intact_acked_prefix_after_a_torn_tail`).
- One-at-a-time de-ignore order: AC-1 lumen reopen -> AC-5 lumen mid-file
  -> AC-6 lumen newline-malformed -> AC-1 D04 binary -> AC-3 WARN ->
  ray AC-1/AC-4/AC-5 -> cinder AC-1/AC-5/AC-6 (+ doc correction AC-7) ->
  pulse AC-1/AC-5/AC-6.
- Mandate compliance evidence: see `mandate-compliance.md`.
- DELIVER also creates `crates/wal-recovery` (ADR-0059 Decision 4), wires
  the four pillars' `open` to call it, corrects the cinder doc (AC-7), and
  adds the `gate-5-mutants-wal-recovery` job in the SAME commit that
  creates the crate (DEVOPS A1).

## Peer review

- Intended reviewer: `nw-acceptance-designer-reviewer` (Sentinel). See
  `peer-review.md` for the outcome and the self-review fallback if the
  reviewer subagent is not dispatchable from this context.
