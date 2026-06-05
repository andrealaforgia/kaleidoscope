# Acceptance Design — beacon-sighup-reload-v0 (DISTILL)

British English throughout, no em dashes.

> **Author**: Scholar (`nw-acceptance-designer`), DISTILL wave, 2026-06-05.
> **Governing inputs**: ADR-0063 (atomic swap + state carryover, the two
> named events); `discuss/user-stories.md` (US-01, US-02 and their AC);
> `devops/wave-decisions.md` (the three non-negotiable conditions).
> **Test file**: `crates/beacon-server/tests/sighup_reload.rs`
> (`[[test]] name = "sighup_reload"`).

## nWave-order reminder (read before reviewing)

The order is **DISCUSS -> DESIGN -> DEVOPS -> DISTILL -> DELIVER**. This is
the **DISTILL** wave. The SIGHUP handler is a **DELIVER** concern and **does
not exist yet**. The proving tests therefore FAIL today by design and are
each marked `#[ignore = "RED until DELIVER: beacon-sighup-reload-v0"]` so
`cargo test --workspace` stays GREEN at the DISTILL commit. RED-and-ignored
with no DELIVER code is the **correct, expected** state at this wave. A
reviewer must not reject this wave for "the tests fail" or "the handler is
missing".

## The driving port

The operator entry point is the **POSIX signal**: edit the `--rules`
directory of a running `beacon-server`, then `kill -HUP <pid>`. There is
**no new CLI and no new HTTP surface** (DISCUSS system constraint). Every
scenario invokes through this single driving port and observes the result
only through operator-visible outputs:

- the **webhook sink** the binary POSTs incidents to (firing / not firing,
  and the incident's `started_at` as the `since` proxy);
- the two **structured `tracing` events** on the child's stderr,
  `beacon.reload.succeeded` (INFO) and `beacon.reload.refused` (WARN)
  (ADR-0063 "Observables").

No internal component (loader, state machine, resolver, store) is invoked
directly. This is Mandate 1 (hexagonal boundary) satisfied by construction:
the test spawns the real binary and speaks to it only as an operator would.

## I/O strategy: Strategy C (real I/O), tagged `@real-io`

Every scenario drives the **real** `beacon-server` binary as a **real child
process** (`CARGO_BIN_EXE_beacon-server`), sends a **real POSIX signal** by
pid, edits a **real writable tmp rules directory**, and observes a **real
mock PromQL backend + real webhook catcher** (both `wiremock`, the harness
shape `smoke.rs` already uses). No InMemory double appears anywhere: an
InMemory double cannot catch the wiring this feature is about (signal
install order, the atomic catalogue swap, the durable-store-under-the-
rules-dir wrinkle, and the structured-event format). See `io-strategy.md`.

## The three DEVOPS conditions, honoured

1. **Determinism — event-synchronised, never p95.** The happen-before
   anchor in every scenario is the structured reload EVENT on the child's
   stderr (`wait_for_event(&stderr, "beacon.reload.succeeded" | "beacon.
   reload.refused")`). Only after the event is seen does the test poll the
   sink / process for the downstream observable, under a single
   `GENEROUS_BOUND` (20 s), returning on first appearance. The seeded rule
   `interval`/`for_duration` is `100ms` for SPEED only; it is never
   asserted. There is no `sleep`-as-sole-sync, no latency budget, no
   percentile anywhere. This is the explicit guard against the project's
   overnight p95-flake class.

2. **State carryover — first-class.** Two dedicated scenarios assert the
   safety property that a rule Firing before a reload stays Firing with the
   same state afterwards and does NOT re-page:
   `surviving_firing_rule_keeps_state_and_does_not_repage_on_successful_reload`
   (success path) and `surviving_firing_rule_does_not_repage_across_refused_reload`
   (refused path). The co-equal observable is "exactly ONE Firing incident
   for the surviving rule across the reload" (no second Firing = state kept,
   no re-page), AND the incident's `started_at` is unchanged (the `since`
   proxy, asserted directly because it IS externally observable on the
   webhook POST body). This exercises ADR-0063 sub-decisions 2/3
   (name-matching carryover + InhibitionResolver rebuild), not merely "a new
   rule fires".

3. **Portability — `#[cfg(unix)]`.** The whole module is
   `#![cfg(unix)]`-gated so a future Windows CI does not fail on the absent
   signal. The signal is sent by pid via the SAFE
   `rustix::process::kill_process(Pid::from_child(&child), Signal::HUP)` so
   the crate's `forbid(unsafe_code)` lint holds (no direct `libc::kill`
   `unsafe` block).

## Scenario inventory (9)

| # | Test fn | Story | Category | Tags |
|---|---------|-------|----------|------|
| 1 | `added_rule_begins_firing_after_sighup_without_restart` | US-01 | walking skeleton | `@walking_skeleton @driving_port @real-io` |
| 2 | `successful_reload_emits_structured_event_naming_what_changed` | US-01 | happy (observability) | `@driving_port @real-io` |
| 3 | `removed_rule_stops_evaluating_after_sighup` | US-01 | edge | `@driving_port @real-io` |
| 4 | `no_change_sighup_swaps_cleanly_with_no_spurious_emissions` | US-01 | boundary | `@driving_port @real-io` |
| 5 | `surviving_firing_rule_keeps_state_and_does_not_repage_on_successful_reload` | US-02 carryover (success) | safety property | `@driving_port @real-io @property` |
| 6 | `malformed_reload_keeps_previous_catalogue_and_does_not_crash` | US-02 | error (the negative) | `@driving_port @real-io` |
| 7 | `surviving_firing_rule_does_not_repage_across_refused_reload` | US-02 carryover (refused) | safety property | `@driving_port @real-io @property` |
| 8 | `reload_to_empty_catalogue_is_refused_daemon_keeps_alerting` | US-02 | error (boundary) | `@driving_port @real-io` |
| 9 | `partly_broken_catalogue_applies_valid_rules_and_surfaces_diagnostic` | US-02 | boundary | `@driving_port @real-io` |

**Error / safety-negative ratio**: scenarios 5, 6, 7, 8, 9 (5 of 9 = **56%**)
exercise refusal, no-re-page, empty-catalogue refusal, partly-broken, or the
carryover safety property. Comfortably above the 40% mandate; appropriate
because US-02 (the negative) is the load-bearing story of this feature.

## Walking skeleton (1)

Scenario 1 is the single walking skeleton: it expresses the operator's whole
goal end to end ("edit the rules, signal, the new alert goes live, no
restart") and is demo-able to a stakeholder. One skeleton is right for a
single-port signal feature; the remaining 8 are focused scenarios on the
same real port, each pinning one business rule or safety property. This
respects the 2-3-skeleton guidance scaled to a feature with exactly one
driving port and one observable surface.

## Why these assertions are observable (Mandate / Dim 7)

Every `Then` asserts an operator-observable outcome: a firing incident at
the sink, the presence of a named structured event on stderr, the child
process still alive, the incident `started_at` value. None asserts internal
state, private fields, or method-call counts. The durable store at
`<rules>/.beacon-state` is a side effect we deliberately do NOT assert on;
we assert the operator-visible consequence (no re-page) instead.

## RED-not-BROKEN evidence

- `cargo test -p beacon-server --test sighup_reload --no-run` compiles
  clean against the existing public surface only.
- `cargo test -p beacon-server --test sighup_reload` reports
  `9 ignored; 0 failed` (workspace stays green at the commit).
- `cargo test ... -- --ignored` on scenario 1 FAILS at the
  `beacon.reload.succeeded` assertion (the event never appears today),
  proving the test is RED on **behaviour**, not broken on a missing symbol
  or a setup error. No fixture supplies the expected output; the Given steps
  set preconditions only (the initial catalogue + the edit), so the test
  cannot pass without the DELIVER handler. This is a valid outer-loop RED,
  not Fixture Theater.

## DELIVER handoff

DELIVER lands the SIGHUP handler (ADR-0063 single-orchestrator
build-new-then-swap-then-abort-old) plus the `InhibitionResolver::rebuild_from`
seam, honours the sub-decision-4 ordering invariant in a code comment, then
**removes the nine `#[ignore]` attributes**. The acceptance suite is the
outer loop: the nine scenarios flip GREEN as the handler, the validate-or-
refuse path, and the state carryover land. The per-feature 100% mutation
gate on the modified `main.rs` + `inhibition.rs` (existing Gate 5 jobs)
guards the orchestrator sequencing the acceptance suite pins black-box.
