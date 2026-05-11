# Slice 03 — Grouping and inhibition (US-BE-03)

## Goal

Storm-suppression primitives. Group emissions by rule labels;
suppress inhibited rules while the inhibitor is firing. Resolution
of the inhibitor emits the previously-suppressed state cleanly.

## IN scope

- `grouping_key` derivation from rule labels + matching query labels
- `inhibits` field in the CUE schema (per-rule list of rule names)
- Inhibition resolver: at each evaluator cycle, for each `Firing`
  rule, suppress emissions of rules in its `inhibits` list while
  the inhibitor is itself `Firing`
- Resolution emission: when the inhibitor transitions to
  `Resolved`, the previously-inhibited rules each emit their
  current state as of the resolution timestamp
- Property test: 50 randomly generated 20-rule sets, asserting
  byte-identical emission output for the same inputs
- Integration test `slice_03_grouping_and_inhibition.rs` exercising
  the 20-rule storm scenario

## OUT scope

- Multi-sink routing (slice 04)
- SLO burn-rate (slice 05)

## Learning hypothesis

Disproves "grouping and inhibition primitives collapse a 20-rule
storm into ≤ 2 emissions". Risk: the inhibition rule may interact
with grouping in non-obvious ways when multiple inhibitors exist
in a chain; the property test catches this.

## Acceptance criteria

US-BE-03 AC-3.1 through AC-3.4.
