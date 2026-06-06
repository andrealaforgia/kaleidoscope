# Outcome KPIs — aperture-presubscriber-probe-stderr-v0

## Feature: aperture-presubscriber-probe-stderr-v0

### Objective

Make aperture's startup honest: a fail-closed refusal due to a downstream
that is not accepting telemetry should TELL the operator why, not exit in
silence. Close the last silent-exit in aperture's startup path
(swallowed-errors family).

### Outcome KPIs

| # | Who | Does What | By How Much | Baseline | Measured By | Type |
|---|-----|-----------|-------------|----------|-------------|------|
| 1 | SREs triaging a non-starting aperture | identify the startup-refusal cause from stderr alone (downstream-not-accepting), without attaching a debugger / strace | 100% of probe-refusal starts emit an operator-visible `event=health.startup.refused` reason line | 0% (silent exit-1 today) | black-box assertion on the binary's stderr for a down-downstream start | Leading (Outcome) |
| 2 | aperture's startup path | exits silently on a fail-closed refusal | 0 silent startup exits (down from 1 known case) | 1 silent case (the probe refusal) | swallowed-errors-family audit + Bea's A19/A20 evidence | Leading (Secondary) |

### Metric Hierarchy

- **North Star**: zero silent startup exits — every fail-closed refusal
  carries an operator-visible reason line.
- **Leading Indicators**: probe-refusal starts that emit a
  `event=health.startup.refused` line naming the sink + error (target 100%).
- **Guardrail Metrics** (must NOT degrade):
  - Healthy-downstream startup still emits NO refusal line (no false positives).
  - Config-error pre-init line (`event=config_validation_failed`, exit 2)
    unchanged.
  - Post-init failures (drain deadline / serve-loop death, ADR-0066) still
    report through `tracing` (no regression on the post-subscriber path).
  - Fail-closed exit code stays non-zero; no listener bound on refusal.

### Measurement Plan

| KPI | Data Source | Collection Method | Frequency | Owner |
|-----|------------|-------------------|-----------|-------|
| 1 | binary stderr | new startup test asserting the line for a down-downstream start; Bea A19/A20 compose-harness evidence | per CI run + on verifier evidence pass | DELIVER tests + Bea Verifier |
| 2 | swallowed-errors audit list | family audit confirms aperture startup has no remaining silent exit | per audit cycle | platform / verifier |

### Hypothesis

We believe that surfacing the pre-subscriber probe refusal as a
structured stderr line for SREs operating aperture will achieve the
"see why the gateway refused" job. We will know this is true when an
operator triaging a down-downstream start reads the cause from stderr
alone (100% of probe-refusal starts carry the reason line) instead of
facing a silent exit-1.

## Handoff to DEVOPS (platform-architect)

- **Data collection**: assert the binary's stderr carries
  `event=health.startup.refused` with the sink identity + error on a
  down-downstream start; this is a black-box log assertion, no new runtime
  instrumentation beyond the line itself.
- **Alerting thresholds**: a startup that exits non-zero with NO refusal
  line and NO config-error line is a guardrail breach (the silence
  regressed).
- **Baseline**: today 0% of probe-refusal starts emit a reason line.
