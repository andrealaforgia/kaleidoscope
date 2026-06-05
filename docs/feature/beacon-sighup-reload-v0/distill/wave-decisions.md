# Wave Decisions — beacon-sighup-reload-v0 (DISTILL)

British English throughout, no em dashes.

> **Author**: Scholar (`nw-acceptance-designer`), DISTILL wave, 2026-06-05.
> **Governing inputs**: ADR-0063; `discuss/user-stories.md` (US-01, US-02);
> `devops/wave-decisions.md` (the three conditions + determinism discipline).
> **Deliverable**: `crates/beacon-server/tests/sighup_reload.rs`
> (`[[test]] name = "sighup_reload"`), 9 scenarios, all `#[ignore]`d RED.

## nWave-order reminder (read before reviewing)

Order: **DISCUSS -> DESIGN -> DEVOPS -> DISTILL -> DELIVER**. This is
**DISTILL**. The SIGHUP handler is a **DELIVER** concern and does not exist
yet; the nine proving tests therefore FAIL today by design and are each
`#[ignore = "RED until DELIVER: beacon-sighup-reload-v0"]`. RED-and-ignored
with no DELIVER code is the **correct, expected** state at this wave. Do not
reject the wave for "failing tests" or "missing handler".

## Headline

One black-box proving test file drives the real `beacon-server` binary
through its only operator port (`kill -HUP` after editing the rules dir) and
observes the reload through the webhook sink + the two ADR-0063 structured
events. Nine scenarios cover US-01 (apply) and US-02 (refuse + carryover),
with the US-02 negative and the state-carryover property as first-class
safety tests.

## Decision 1 — slice name and target

Named `sighup_reload` (the next free slice under
`crates/beacon-server/tests/`, which previously held only `smoke.rs`).
Registered as a dedicated `[[test]]` target so it runs under Gate 1's
existing `cargo test --workspace --all-targets` (DEVOPS Decision 1; no new
CI job).

## Decision 2 — driving port and observables (Mandate 1)

The driving port is the POSIX signal; no new CLI/HTTP surface (DISCUSS
constraint). Observables: the webhook sink POSTs (firing presence +
`started_at` as the `since` proxy) and the stderr events
`beacon.reload.succeeded` / `beacon.reload.refused` (ADR-0063). No internal
beacon symbol is imported; the binary is reached only as
`CARGO_BIN_EXE_beacon-server`.

## Decision 3 — the three DEVOPS conditions, all honoured

1. **Determinism (event-sync, no p95).** Every scenario synchronises on the
   structured reload event as the happen-before anchor, then polls the
   sink/process under one `GENEROUS_BOUND` (20 s), returning on first
   appearance. The seeded `100ms` interval/for_duration is for SPEED only,
   never asserted. No percentile, no latency budget, no sleep-as-sole-sync.
2. **State carryover, first-class.** Two dedicated scenarios assert a rule
   Firing before the reload stays Firing with exactly ONE incident (no
   re-page) AND the same `started_at`, across BOTH a successful reload and
   a refused reload (ADR-0063 sub-decisions 2/3).
3. **Portability (`#[cfg(unix)]`).** Module gated `#![cfg(unix)]`; signal
   sent by pid via the safe `rustix::process::kill_process` (honours
   `forbid(unsafe_code)`).

## Decision 4 — safe signal surface (no `unsafe`)

beacon-server's test target inherits `[lints.rust] unsafe_code = "forbid"`.
A raw `libc::kill` needs an `unsafe` block, which the lint rejects. Chosen
`rustix::process::kill_process(Pid::from_child(&child), Signal::HUP)`:
fully safe (rustix does the FFI internally), already in the workspace lock
(1.1.4), dev-dep adds only the `process` feature. The DELIVER crafter
remains free to choose any production signal-install surface; the test does
not reference it.

## Decision 5 — tmp rules dir without a new dep

The project's existing subprocess tests hand-roll tmp dirs from
`env::temp_dir()` (e.g. `v1_slice_03_crash_durability.rs`) rather than
depend on `tempfile`. DISTILL follows that precedent with a `TmpRules`
RAII guard (unique `pid+nanos` subdir, `Drop` cleans the tree), so the
writable-rules-dir wrinkle (durable store under `<rules>/.beacon-state`,
DEVOPS Decision 3) is satisfied with zero new crates.

## Verification run (evidence)

- `cargo test -p beacon-server --test sighup_reload --no-run` — compiles
  clean against the existing public surface.
- `cargo test -p beacon-server --test sighup_reload` — `9 ignored;
  0 failed` (workspace green at the commit).
- `cargo clippy -p beacon-server --tests` — clean (no warnings, no
  unsafe-code violation).
- `cargo test ... added_rule_begins_firing_after_sighup_without_restart --
  --ignored` — FAILS at the `beacon.reload.succeeded` assertion, proving
  behavioural RED (not a missing symbol, not a setup error, not Fixture
  Theater).
- `cargo test --workspace --no-run` — whole workspace test build compiles.

## Self-review (structured, critique-dimensions Dims 1-9)

All nine dimensions PASS; full table in `mandate-compliance.md`. Highlights:

- **Dim 1 (happy-path bias)**: 5 of 9 scenarios are error/safety-negative
  (56%, >= 40%). The load-bearing US-02 negative and both carryover paths
  are first-class. PASS.
- **Dim 4 / Dim 8 (coverage + traceability)**: all 10 AC and all 10 DISCUSS
  UAT scenarios mapped (`ac-coverage.md`); every test carries an inline
  `(US-0x)` tag-comment; US-01 and US-02 each have >= 1 scenario. PASS.
- **Dim 5 (WS user-centricity)**: the single walking skeleton is titled as
  an operator goal with operator-observation Then steps; demo-able. PASS.
- **Dim 7 (observable assertions)**: every Then asserts a sink POST, a named
  stderr event, process liveness, or `started_at`; none asserts internal or
  private state. PASS.
- **Dim 9 (WS boundary proof)**: Strategy C declared (DEVOPS) and
  implemented with real binary + real signal + real I/O; no `@in-memory`;
  the driven surfaces (signal, sink, backend) have real-I/O coverage. PASS.

Residual: none blocking. The `since` proxy is asserted directly (the
`started_at` IS observable on the webhook body), and additionally backed by
the co-equal "exactly one Firing incident" assertion, so the carryover
property holds even if a future incident schema dropped `started_at`.

## Peer review

`@nw-acceptance-designer-reviewer` (Sentinel) dispatch via the Task tool is
**not invocable from this subagent context**. A rigorous structured
self-review against critique-dimensions Dims 1-9 was performed in lieu (above
+ `mandate-compliance.md`). A top-level `@nw-acceptance-designer-reviewer`
run is **FLAGGED for the parent to dispatch before DELIVER**, and MUST carry
the nWave-order reminder at the top of this file so the reviewer does not
reject on the (correct, expected) RED-and-ignored state with no DELIVER code.

## DELIVER handoff

1. Land the SIGHUP handler per ADR-0063 (single-orchestrator,
   build-new-then-swap-then-abort-old; the `InhibitionResolver::rebuild_from`
   seam; the validate-or-refuse path with the two structured events).
2. Honour the sub-decision-4 ordering invariant (new set live before old
   set aborted) in a code comment + the per-feature mutation gate.
3. **Remove the nine `#[ignore]` attributes** in `sighup_reload.rs`; the
   acceptance suite is the outer loop and flips GREEN as the handler lands.
4. Per-feature 100% mutation kill on the modified `main.rs` +
   `inhibition.rs` via the existing Gate 5 beacon jobs (DEVOPS Decision 1).

## Changelog

- 2026-06-05: DISTILL wave authored. Wrote `sighup_reload.rs` (9 scenarios,
  Strategy C real-I/O, all `#[ignore]`d RED). Honoured the three DEVOPS
  conditions (event-sync determinism, first-class state carryover on both
  paths, `#[cfg(unix)]` portability with safe rustix signal). Verified
  RED-not-BROKEN behaviourally. Authored acceptance-design.md,
  ac-coverage.md, io-strategy.md, mandate-compliance.md, this file. Flagged
  a top-level reviewer run with the nWave-order reminder. Did NOT proceed
  into DELIVER.
