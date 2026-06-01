# DISTILL Decisions — cli-unknown-flag-rejection-v0

## Origin and conjunction

DISTILL reads DISCUSS (`user-stories.md`, US-01..US-04) and DESIGN
(`application-architecture.md`, `wave-decisions.md`) and translates them
into a subprocess acceptance suite that re-anchors EDD verifier defect
K11. The anchor was dropped in revert e3a8cad; these tests ARE the fresh,
non-reverted anchor K11 re-verifies against. No KPI contract file or
product SSOT (`docs/product/`) exists for this feature, so KPI
observability scenarios are skipped (soft gate; warning logged below).

## Reconciliation result

Reconciliation passed — 0 contradictions. DISCUSS and DESIGN agree on
every load-bearing decision: hand-rolled parser (no clap), exit code 2
for all rejections, stderr wording `unknown flag "<token>"` for the
subcommand case, strictly additive fix (US-04 regression guard), no ADR.
No DISCUSS assumption is contradicted by DESIGN.

## DWD-01 — Driving port: the binary argv entry point

Every scenario enters through the only driving port this feature touches:
the `kaleidoscope-cli` binary's argv entry (`fn main` in `src/main.rs`),
spawned as a subprocess via `env!("CARGO_BIN_EXE_kaleidoscope-cli")` with
`std::process::Command` and `Stdio`. This is the operator's real
invocation path. The observable contract is exit code plus stderr
(usage error). No library function is invoked directly except `ingest`,
used purely as a setup helper to seed a readable data directory (mirroring
`read_time_range.rs::seed`). This honours the Driving Adapter
Verification mandate: the CLI entry point is exercised via its real
protocol (subprocess + argv + exit code + stderr), not by calling a
service function.

## DWD-02 — Walking Skeleton Strategy: Strategy C (real local I/O)

Auto-detected strategy: C (real local). The feature has only local
resources (the binary itself plus a filesystem-backed Lumen store). There
are no costly external dependencies. The acceptance tests use the real
compiled binary and a real tmp filesystem (`temp_root`), with `ingest`
seeding a real Lumen store for the AC-02 and AC-04 read paths. No InMemory
doubles. No `@requires_external`. Container preference: none (real
adapters on host), consistent with the existing CLI test suite
(`read_time_range.rs`, `cli_binary_smoke.rs`).

Note on walking skeleton framing: this is a bug-fix / contract re-anchor
feature, not a greenfield capability. Per nw-bdd-methodology ("Features
only; optional for bugs") no dedicated `@walking_skeleton` scenario is
introduced; AC-04 already exercises the full real-I/O happy path
(spawn binary -> open real store -> read records -> append metric line ->
exit 0) end to end, which is the demo-able user-value slice.

## DWD-03 — US -> AC mapping (port-to-port)

| AC | US | Driving port | Observable outcome | Status today |
|----|----|--------------|--------------------|--------------|
| AC-01 | US-01 | binary argv (`--bogus`) | exit 2 + stderr names `--bogus` + usage block | GREEN (top-level arm already correct) |
| AC-02 | US-02 | binary argv (`read acme <seeded> --bogus`) | exit 2 + stderr `unknown flag "--bogus"` + empty stdout | RED (the silent-accept gap) |
| AC-03 | US-03 | binary argv (`bogus-subcommand`) | exit 2 + stderr names verb + usage block | GREEN (unknown-subcommand arm already correct) |
| AC-04 | US-04 | binary argv (`read acme <seeded> --observe-otlp <path>`) | exit 0 + record on stdout + metric line + `read ok: records=1` | GREEN (regression guard) |

All four US (US-01..US-04) have at least one covering scenario:
traceability complete.

## DWD-04 — Subprocess strategy and AC-02 data seeding

The contract under test (exit code + stderr) is observable only at the
process boundary, so every AC spawns the real binary (not a library
call). This mirrors `read_time_range.rs` tests #5/#6 (the OK4 fail-fast
subprocess tests) and `cli_binary_smoke.rs`.

AC-02 seeds one record into the data directory via `ingest` BEFORE
spawning `read`. Rationale: with a NON-existent data directory, today's
binary silently ignores `--bogus`, proceeds to open the store, and fails
with a lumen I/O error (exit 1) — which would mask the gap behind an
unrelated failure and would not cleanly distinguish RED-now from
GREEN-after. With a seeded, openable store the silent-accept gap surfaces
as its true shape: `read` succeeds, exit 0, records printed (`--bogus`
ignored). After the fix, `reject_unknown_flags` runs during the argv
parse BEFORE the store is opened (the fail-before-store-open invariant the
OK4 tests already assert), so AC-02 turns GREEN with exit 2 and empty
stdout even though the data directory is readable. Verified by reading
`main.rs`: each `run_*_with` wrapper parses argv (where DESIGN DD1 places
the `reject_unknown_flags(args, known)?` call) before any library call
that opens a store.

## DWD-05 — AC-04 case selection (regression guard)

AC-04 uses `read acme <seeded> --observe-otlp <path>` rather than a
`--help`-style case. Rationale: `--observe-otlp` is a value-taking known
flag, so this case positively exercises the consumed-value rule (DESIGN
DD-rule clause 1) — the value token `<path>` must be consumed by the
known flag and never re-classified as an unknown flag or a positional.
This is the load-bearing additive case the future helper must honour. A
no-flag or `--help` invocation would exercise nothing the helper changes,
so it would be a weaker guard. AC-04 asserts exit 0, the seeded record on
stdout, the `read ok: records=1` summary, and a `lumen.query.count` line
appended to the metric file (proving the flag was honoured, not skipped).

## DWD-06 — Mandate 7: RED-not-BROKEN, no scaffold

AC-02 is RED, not BROKEN. The binary compiles and runs; the test fails on
an assertion (`assertion left == right failed: got Some(0), expected
Some(2)`) — a clean assertion failure with the process exiting normally,
not an ImportError / panic / build break. No scaffold file is created:
the production change is Crafty's shared `reject_unknown_flags` free
function plus eight one-line call sites in `src/main.rs` (DESIGN DD1).
There is no new module for tests to import, so the Mandate 7 scaffold
mechanism (stub raising AssertionError) does not apply — the existing
binary IS the RED-ready surface. AC-01/03/04 are GREEN against the
shipped binary and pin the contract that must not regress.

## DWD-07 — AC-02 status: `#[ignore]` for pre-commit safety (DECIDED)

AC-02 is marked `#[ignore]` with the comment "RED gap; de-ignored by
Crafty in DELIVER once reject_unknown_flags lands". Decision rationale:
the deterministic pre-commit hook (p95 gated) runs the test suite, and a
RED-active AC-02 would fail the hook when these tests are committed ahead
of the fix. Because the DELIVER change is atomic via Crafty (helper +
de-ignore land together), `#[ignore]` is the safe carry mechanism: the
suite stays green at commit time, and Crafty removes the `#[ignore]`
attribute in the same DELIVER change that adds `reject_unknown_flags`,
turning AC-02 GREEN. The RED gate remains observable on demand via
`cargo test -p kaleidoscope-cli --test slice_17_unknown_flag_rejection
-- --ignored` (verified RED today, exit 0 observed).

## DWD-08 — K11 anchor note

This file (`tests/slice_17_unknown_flag_rejection.rs`) is the fresh K11
anchor. The verifier asserts, per DESIGN DD2/DD3:
- US-01 / US-03 rows: exit 2 and stderr names the offending token (the
  existing `unknown subcommand` arm already satisfies this; AC-01/AC-03
  re-anchor it).
- US-02 row: exit 2 and stderr substring `unknown flag "<token>"` (AC-02,
  RED until Crafty lands the helper).
- US-04 row: known invocations unchanged (AC-04 regression guard).

## Self-Review Checklist (Dimension 9 + Mandate 7)

- [x] 1. WS strategy declared (DWD-02: Strategy C, real local I/O).
- [x] 2. Scenarios use real adapters (real binary + real tmp filesystem);
      no `@in-memory`.
- [x] 3. Every driven resource (filesystem-backed Lumen store) has a
      real-I/O scenario (AC-02 seeds and AC-04 reads a real store).
- [x] 4. No InMemory doubles used; nothing to document as un-modelled.
- [x] 5. Container preference documented (none; real on host).
- [x] 6/7/8. Mandate 7: no production module imported that lacks an impl
      (the fix is a free function in the existing binary); no scaffold
      needed; AC-02 fails by assertion, not infra error.
- [x] 9. AC-02 is RED (assertion failure, exit 0 vs 2), not BROKEN —
      verified with `-- --ignored`.
- [x] 10. Driving Adapter: every scenario exercises the CLI via subprocess
      (argv + exit code + stderr), not a service call.
- [x] 11. Real-I/O adapter coverage: AC-02 / AC-04 exercise the real
      Lumen store on a real tmp filesystem.
- [x] Story coverage: US-01..US-04 all covered (DWD-03).
- [x] Error-path ratio: 3 of 4 scenarios are rejection / error paths
      (AC-01, AC-02, AC-03 = 75%), well above the 40% floor.
- [x] Business language: Gherkin-equivalent test names and doc comments
      use operator language (unknown flag, unknown subcommand, records);
      technical detail (exit code, argv, Stdio) lives in step bodies.
