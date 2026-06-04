# Outcome KPIs — claims-honesty-pass-v0

## Feature: claims-honesty-pass-v0

### Objective

A reader of any Kaleidoscope claim — README, crate doc, codename, Cargo.toml
description, test header — finds the claim matches the code, or is clearly marked
future/roadmap. The project's structural-honesty thesis holds against its own
prose.

### Outcome KPIs

| # | Who | Does What | By How Much | Baseline | Measured By | Type |
|---|-----|-----------|-------------|----------|-------------|------|
| 1 | README-table evaluators | Read four component rows + the cost-table line with zero present-tense capability overstatements | 4/4 rows + 1 cost line corrected; 0 residual present-tense overstatements | 4 overstated rows + 1 cost line | grep/doc-lint guard: 4 false phrases absent, 4 corrected phrases present | Leading |
| 2 | codex evaluators | Read a crate status that matches the delivered green code | 0/7 codex doc surfaces still declare a stub (was 7/7) | 7 stale stub declarations | grep guard on Cargo.toml + 5 headers + common; codex suite green | Leading |
| 3 | read-side source readers | Read module/handler docs that match the green bodies | 2/2 stale scaffold doc blocks corrected; 0 in-flight RED markers touched | 2 stale scaffold blocks | bidirectional grep guard (stale phrase absent in the 2 loci; in-flight markers still present) | Leading |
| 4 | harness trusters | Understand the harness proves structural decode, not semantic conformance | 3/3 depth loci + 1 status block corrected; 0 "wire specification" semantic overclaim | 3 depth overclaims + 1 stale status | grep guard + 1 acceptance test (semantically-invalid-but-structurally-valid body accepted) | Leading |
| 5 | query-range tooling integrators | Form a correct expectation of `step` before querying | black-box (2 step values) result == documented claim; 0 gap | README implies stepped grid; behaviour returns raw points | black-box test + doc guard | Leading |
| 6 | gRPC-framing harness users | Strip (or not) the gRPC length prefix correctly | framing claim == behaviour; 0 confusing decode failures from inert framing | `GrpcProtobuf` presented as supported but inert | doc guard + both-framings acceptance test | Leading |

### Metric Hierarchy

- **North Star**: **Claim-to-code match rate** across the corrected surfaces —
  the fraction of touched claims whose prose matches the live code. Target: 100%
  of the in-scope overstatements (6 clusters / 11 inventory items) reconciled,
  measured by the guard suite.
- **Leading Indicators**: per-slice grep/doc-lint guards passing (false string
  absent, corrected string present); the harness semantic-boundary test; the
  query-api black-box `step` test; the harness both-framings test.
- **Guardrail Metrics** (must NOT degrade):
  - No genuinely-RED / `#[ignore]`d in-flight scaffold marker altered (the US-02,
    US-03 guards assert in-flight markers remain present).
  - No store/handler/validator BEHAVIOUR changed, except where DESIGN explicitly
    picks "implement" for US-05 / US-06.
  - Existing test suites for the six touched crates stay green.
  - The already-true README durability claim is not re-corrected.

### Measurement Plan

| KPI | Data Source | Collection Method | Frequency | Owner |
|-----|------------|-------------------|-----------|-------|
| 1 | `README.md` | grep/doc-lint guard in the feature's guard suite | Per slice + at DELIVER | DELIVER (crafter) |
| 2 | codex Cargo.toml + headers | grep guard; `cargo test -p codex` | Per slice + at DELIVER | DELIVER |
| 3 | query-http-common + trace-query-api lib.rs | bidirectional grep guard | Per slice + at DELIVER | DELIVER |
| 4 | harness lib.rs/README/Cargo.toml + decode.rs | grep guard + acceptance test | Per slice + at DELIVER | DELIVER |
| 5 | query-api endpoint + README | black-box step test + doc guard | At DELIVER | DELIVER (+ verifier black-box) |
| 6 | harness framing + lib.rs/README | doc guard + both-framings test | At DELIVER | DELIVER |

### Hypothesis

We believe that correcting each overstated Kaleidoscope claim to the truth (or to
explicit future tense) for an evaluator like Devin will achieve a 100% claim-to-
code match across the in-scope surfaces. We will know this is true when the guard
suite shows every targeted false string absent, every corrected string present,
and the behaviour-touching items (harness semantic boundary, query-api `step`,
harness framing) assert behaviour that matches the corrected prose — with zero
genuinely-RED in-flight marker altered and zero already-true claim re-corrected.

### Handoff to DEVOPS (platform-architect)

The platform-architect needs from this file to plan instrumentation:

1. **Data collection**: the guard suite is the instrumentation — grep/doc-lint
   guards per slice + three behaviour tests (harness semantic boundary, query-api
   `step` black-box, harness both-framings). These run in the existing test plane;
   no new telemetry pipeline is required.
2. **Dashboard/monitoring**: none required — a doc-honesty feature's KPIs are
   verified by tests in CI (CI-is-feedback per project memory), not by a runtime
   dashboard.
3. **Alerting thresholds**: the guardrail "no in-flight RED marker altered" and
   "no unintended behaviour change" are enforced by the guard suite + the existing
   per-crate suites staying green; a regression is a red test, not an alert.
4. **Baseline**: the baselines in the table above (counts of overstated surfaces)
   are captured today by Luna's verified inventory in `wave-decisions.md`.
