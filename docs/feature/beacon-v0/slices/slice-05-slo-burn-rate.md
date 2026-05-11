# Slice 05 — SLO burn-rate alerting (US-BE-05)

## Goal

CUE SLO declaration → synthesised PromQL rule set per Google SRE
workbook §14.4 multi-window-multi-burn-rate Table 14-3. Byte-equal
firing decisions to a hand-authored reference.

## IN scope

- CUE schema `crates/beacon/cue/slo.cue` with required fields:
  `service`, `sli_good_events` (PromQL), `sli_total_events`
  (PromQL), `target_availability` (float in (0,1)),
  `error_budget_period` (Prometheus duration)
- Code generator that emits five MWMBR rules from one SLO
  declaration per Google SRE workbook §14.4 Table 14-3:
  - Page: 1h/5m windows, burn-rate threshold 14.4
  - Page: 6h/30m windows, burn-rate threshold 6
  - Ticket: 1d/2h windows, burn-rate threshold 3
  - Ticket: 3d/6h windows, burn-rate threshold 1
- Each synthesised rule carries `slo_service` and `slo_window`
  labels for correlation
- Synthesised PromQL is deterministic (byte-equal across runs for
  the same SLO inputs)
- Cross-validation acceptance test
  `tests/slice_05_slo_burn_rate.rs` asserting byte-equal firing
  pattern against a hand-authored reference rule, on a 24-hour
  synthetic trace

## OUT scope

- Anything beyond SLO synthesis (the synthesised rules flow through
  the same evaluator from slice 01 and sinks from slice 04 without
  modification)

## Learning hypothesis

Disproves "Beacon's SLO synthesis matches Google SRE's published
methodology byte-for-byte". Risk: an off-by-one in the PromQL
window expressions could produce firing pattern divergence on
slow-burn scenarios; the cross-validation against the reference
rule is the load-bearing test.

## Acceptance criteria

US-BE-05 AC-5.1 through AC-5.5.

## Dependencies

- The Google SRE workbook table is the authoritative source. The
  table values are public; the code generator inlines them as Rust
  constants with the workbook URL in a comment.
- The reference hand-authored PromQL rules for cross-validation
  live in the test fixture directory.
