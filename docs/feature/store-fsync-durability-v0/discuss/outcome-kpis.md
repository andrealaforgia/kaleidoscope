# Outcome KPIs — store-fsync-durability-v0

## Feature: store-fsync-durability-v0

### Objective

Make the "survives a restart" durability promise TRUE and DEMONSTRABLE
across all seven durable stores: after a power loss or `kill -9`, every
acked write is recoverable and the store opens cleanly even if the crash
hit mid-snapshot — proven by an out-of-process crash test, not asserted
by the README.

### North Star

**Fraction of durable stores for which an out-of-process kill-9 proving
test demonstrates: (a) every acked write is present after reopen, and
(b) the store opens cleanly after a mid-snapshot crash.**

- Baseline: **0 of 7** stores provably crash-durable (no test in the
  1194-test suite simulates an out-of-process crash; all reopen
  in-process, exercising only graceful restart — the "false-confidence"
  finding).
- Target: **7 of 7** stores with a passing out-of-process kill-9 proving
  test (lumen, ray, strata, cinder, sluice, beacon state, pulse-snapshot).

This is a correctness-hardening feature, so the north star is a coverage
ratio over the durability invariant, not a usage-behaviour rate. The
"behaviour change" being measured is the platform's own provable honesty:
the move from "claims durable" to "proven durable under a real crash".

### Outcome KPIs

| # | Who | Does What | By How Much | Baseline | Measured By | Type |
|---|-----|-----------|-------------|----------|-------------|------|
| K1 | The 7 durable stores | have a passing out-of-process kill-9 proving test for acked-write-survival | 7 of 7 (100%) | 0 of 7 | CI proving-test gate per store | Leading |
| K2 | Crashed-with-acked-tail stores | recover every acked write on reopen | 100% (no acked write lost) | 0% provable | proving-test assertion: acked write present after `SIGKILL`+reopen | Leading |
| K3 | Mid-snapshot-crashed stores | open cleanly (no torn snapshot blocks open) | 100% open | 0% (today a mid-snapshot crash refuses to open — total loss) | proving-test assertion: `open()` succeeds after mid-snapshot `SIGKILL` | Leading |
| K4 | The substrate-lies refusal path | refuses to start on a lying-fsync substrate, per store | 7 of 7 emit `health.startup.refused` and exit non-zero | 1 of 7 (pulse only, ADR-0049) | gold-test on `LyingFsyncBackend` + composition-root emission | Leading |
| K5 | Store trait signatures | stay byte-identical to the prior tag | 0 trait-signature regressions | n/a | Gate 2 `cargo public-api` | Guardrail |

### Metric Hierarchy

- **North Star**: K1 — provably crash-durable stores (0/7 → 7/7).
- **Leading Indicators**:
  - K2 (acked writes recovered after crash) — the write-side fsync paying
    off.
  - K3 (mid-snapshot crashes open cleanly) — the atomic-snapshot paying
    off.
  - K4 (lying-substrate refusal extended to all stores) — the
    Earned-Trust probe generalised per ADR-0049 §8.
- **Guardrail Metrics** (must NOT degrade):
  - K5 — no store trait signature changes (Gate 2 `cargo public-api`
    byte-identity).
  - **G1 — tolerance stays narrow**: a torn-tail recovery never swallows
    mid-file corruption or a newline-terminated malformed final line
    (inherits ADR-0059 AC-5 / AC-6; this feature produces the torn tail,
    must not widen the tolerance). Measured by the existing ADR-0059
    fail-closed gold-tests continuing to pass.
  - **G2 — mutation kill stays 100%** on modified files (ADR-0005 Gate 5;
    CLAUDE.md per-feature mutation). The fsync calls, the rename, and the
    parent-dir fsync must each be non-deletable without a surviving test.
  - **G3 — proving test is deterministic, not timing-based**: zero
    flakiness; no wall-clock p95 assertion (avoids the overnight p95-flake
    class recorded in project memory).

### Measurement Plan

| KPI | Data Source | Collection Method | Frequency | Owner |
|-----|------------|-------------------|-----------|-------|
| K1 | per-store `tests/*kill9*` / proving-test target | CI pass/fail count across the 7 stores | per CI run, per slice merge | DEVOPS (platform-architect) |
| K2 | proving-test assertion output | acked-write-present assertion result | per CI run | DEVOPS |
| K3 | proving-test assertion output | `open()`-succeeds-after-mid-snapshot-crash result | per CI run | DEVOPS |
| K4 | `LyingFsyncBackend` gold-test + composition-root emission capture | `event=health.startup.refused` assertion per store | per CI run | DEVOPS |
| K5 | `cargo public-api` (Gate 2) | trait-signature diff vs prior tag | per CI run | DEVOPS |
| G2 | `cargo mutants --in-diff` (Gate 5) | mutation kill rate on modified files | per slice merge | DEVOPS |

### Hypothesis

We believe that adding per-record `sync_all` on WAL append and atomic
(temp+rename+fsync-parent) snapshots to the six un-hardened stores (and
the snapshot to pulse), each proven by an out-of-process kill-9 test,
will make the durability promise true and demonstrable. We will know this
is true when all 7 durable stores have a passing out-of-process kill-9
proving test (K1: 0/7 → 7/7) and every acked write survives a simulated
crash (K2: 100%) and every mid-snapshot crash opens cleanly (K3: 100%),
with no trait-signature regression (K5) and 100% mutation kill (G2).

### Handoff to DEVOPS (instrumentation requirements)

The platform-architect needs, from this file:

1. **Proving-test CI wiring**: each store's out-of-process kill-9 proving
   test must be a CI gate (K1/K2/K3). The test spawns a child process,
   acks a write, `SIGKILL`s it, reopens in the parent. DEVOPS decides the
   harness (a small test-only binary per store vs `std::process::Command`
   driving an existing entry point) — but it must be out-of-process (C5)
   and deterministic (G3).
2. **Mutation gate scope**: extend `cargo mutants --in-diff` coverage to
   the new fsync/rename/parent-dir-fsync call sites per store (G2; ADR-0005
   Gate 5).
3. **Gate 2 coverage**: `cargo public-api` must assert byte-identity of
   all five store traits across every slice (K5).
4. **No new dashboard / no new event**: the refusal reuses
   `event=health.startup.refused`; the torn-tail recovery reuses
   `event=wal.recovery.torn_tail_dropped`. K4 rides the existing
   structured `tracing` stream (ADR-0049 §7, ADR-0059 §3). No new metric.
