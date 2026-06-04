# Outcome KPIs — tls-config-reject-v0

## Feature: tls-config-reject-v0

### Objective

Within this release, no aperture v0 collector ever ships telemetry in plaintext when
its operator requested transport encryption or SPIFFE auth — the collector refuses to
start and says so, instead of silently downgrading.

### Outcome KPIs

| # | Who | Does What | By How Much | Baseline | Measured By | Type |
|---|-----|-----------|-------------|----------|-------------|------|
| 1 | aperture operators starting v0 with a requested-but-unimplemented security knob (`tls.enabled` or `auth.spiffe.enabled` = true) | get a refuse-to-start (non-zero exit + structured refusal event naming the knob + zero plaintext listeners bound) instead of a silent plaintext downgrade | 100% of such startups (target 1.0) | 0% — today 100% warn-and-bind-plaintext (`compose.rs:127`) | Integration tests across the two-knob truth table asserting exit code 2, refusal event presence + named-knob field, and absence of any bound listener | Leading (Outcome) |
| 2 | aperture operators starting v0 with both security knobs off / absent (the common path) | continue to start-and-bind exactly as before — no regression, no spurious refusal | 100% of such startups start and bind (target 1.0; guardrail) | 100% today | Negative-control integration tests asserting `event=startup`, both listeners bound (`:4317`, `:4318`), and no refusal event | Guardrail |
| 3 | a code reader / security reviewer inspecting the security-knob handling | finds source comments that accurately describe the refusal | 0 false "validator rejects it" comments remain (target 0) | 1 false comment at `sinks.rs:94-95` | Code review / grep assertion that no comment claims a non-existent rejection | Secondary (Leading) |

### Metric Hierarchy

- **North Star**: fraction of security-knob-set startups that refuse-to-start with no
  plaintext bind. Target 1.0. (KPI 1.)
- **Leading Indicators**: refusal event emitted on the `tls`-true, `spiffe`-true, and
  both-true paths; exit code 2 on each.
- **Guardrail Metrics**: both-off / absent startups still start and bind (KPI 2) — must
  NOT degrade; no false refusals on the common path.

### Measurement Plan

| KPI | Data Source | Collection Method | Frequency | Owner |
|-----|------------|-------------------|-----------|-------|
| 1 | aperture integration test suite (truth-table + stderr-event capture) | assert exit code, refusal event + named-knob field, no bound listener | every CI run (per-feature gate) | aperture maintainer (crafter) |
| 2 | aperture integration test suite (negative controls) | assert `event=startup`, both listeners bound, no refusal event | every CI run | aperture maintainer (crafter) |
| 3 | source-tree grep / review | assert no "validator rejects it ahead of this sink" style claim remains | at review, then on every CI run if encoded as a doc-lint | aperture maintainer (crafter) |

### Hypothesis

We believe that refusing to start aperture v0 when `tls.enabled` or
`auth.spiffe.enabled` is requested-but-unimplemented, for platform/SRE operators
deploying in regulated fleets, will achieve the objective of zero silent plaintext
downgrades. We will know this is true when 100% of security-knob-set startups exit
non-zero with a refusal event naming the knob and bind no plaintext listener, while
100% of knob-off startups continue to start and bind unchanged.

### Handoff to DEVOPS (instrumentation notes)

- **Events to recognise**: the refusal event DESIGN selects (candidate
  `config_validation_failed` or `health.startup.refused`) on stderr, carrying a field
  that names the offending security knob. This is the fail-closed signal the fleet
  should alert on (an operator-error, not a system failure — a crashloop on this exit
  code 2 means "fix the config", not "page the on-call for a bug").
- **Guardrail alert**: a spike in exit-code-2 refusals after a fleet config rollout is
  the signal that a config template set a security knob; surface it so operators catch
  the misconfig fast rather than discovering an outage.
- **No new metric pipeline required**: exit codes and structured stderr events are
  already collected; this feature adds one more refusal cause to an existing shape.
