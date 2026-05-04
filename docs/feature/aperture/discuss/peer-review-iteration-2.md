# Peer Review — Iteration 2 — `aperture` v0 (DISCUSS)

> **Wave**: DISCUSS — Phase 4 (peer review gate, second pass).
> **Reviewer mode**: `nw-product-owner` in independent-reviewer persona, applying `nw-po-review-dimensions`.
> **Date**: 2026-05-04.
> **Iteration**: 2 of max 2.
> **Scope**: re-checking the 14 specific revisions required at iteration 1. Dimensions that PASSED at iteration 1 are not re-litigated.

---

## Verification of iteration-1 revisions

For each of the 14 required revisions, the reviewer verifies whether the applied change resolves the iteration-1 finding.

| # | Iteration-1 finding | Required revision | Applied? | Verdict |
|---|---|---|---|---|
| 1 | Concurrency-cap default (1024) repeated without rationale | Add 50-pod cluster math to `wave-decisions.md > D7` | YES — D7 now contains: "1024 chosen as a placeholder large enough to absorb realistic burst traffic from a 50-pod application cluster (50 pods × 16 concurrent OTel exporters per pod / 1 Aperture replica = 800 concurrent in flight, rounded up to a power of two)" | RESOLVED |
| 2 | Drain-deadline default (30 s) without rationale | Add k8s `terminationGracePeriodSeconds` anchor to `D8` | YES — D8 now contains: "30 s default chosen to match Kubernetes' default `terminationGracePeriodSeconds` (30 s); k8s sends SIGKILL after that period regardless, so deadlines longer than 30 s have no effect under k8s anyway" | RESOLVED |
| 3 | ForwardingSink timeout default (5 s) without rationale | Add OTel SDK 10 s anchor to US-AP-08 Technical Notes | YES — US-AP-08 Technical Notes now contains: "5 s chosen because the OTel SDK's default exporter timeout is 10 s; Aperture's ForwardingSink should fail before the SDK times out so the SDK's retry budget is not consumed by a hung Aperture-to-downstream call" | RESOLVED |
| 4 | OtlpSink trait shape: no rejected-alternatives section | Add three rejected-alternatives subsection to D2 | YES — D2 now enumerates synchronous, channel-based, and callback-based alternatives with explicit rejection reasons | RESOLVED |
| 5 | No UAT for stderr-write failure | Add scenario to `journey-aperture.feature` step 5 | YES — new scenario "Aperture continues serving traffic when stderr writes fail" added immediately after the no-telemetry-on-telemetry scenario | RESOLVED |
| 6 | No memory-bound NFR | Add to `wave-decisions.md` (chose D7 location, since concurrency cap is the load-bearing variable) | YES — D7 now contains a "Memory-bound NFR (derived)" subsection with the 1024 × 4 MiB × 2 = 8 GiB worst-case calculation and operator guidance to size pods accordingly | RESOLVED |
| 7 | SIGTERM-mid-receive boundary AC missing | Add to US-AP-09 AC | YES — US-AP-09 now has a final AC bullet covering the (a) complete-if-fully-received OR (b) TCP-reset-on-listener-close cases, naming "deterministic outcome, never half-acknowledged" | RESOLVED |
| 8 | Overlapping bind-address misconfiguration | Add UAT to US-AP-01 | YES — new UAT scenario "Identical bind addresses for grpc and http are rejected at config validation" added; corresponding AC bullet added to US-AP-01 | RESOLVED |
| 9 | Sieve-shaped sink UAT missing | Add scenario to US-AP-03 | YES — new UAT scenario "A custom OtlpSink implementation (Sieve-shaped) plugs in without crate-level changes" added; corresponding AC bullet added | RESOLVED |
| 10 | max_recv_msg_size default vs UAT example inconsistency | Add explicit note about test-time vs default | YES — `journey-aperture.feature` body_too_large scenario now carries a `# NB:` comment explaining the 1 MiB test-time value vs the 4 MiB v0 default | RESOLVED |
| 11 | KPI 3 was survey-based without instrument | Either downgrade to structural KPI or specify survey instrument | YES — KPI 3 now has BOTH a structural primary part (CI test on the readiness-state machine) AND the survey as a secondary check with an explicit five-question instrument; the Measurement Plan row reflects both data sources | RESOLVED |
| 12 | KPI 1 not test-defended | Add Slice-01 demo runs in CI to US-AP-03 AC | YES — US-AP-03 now has a final AC bullet: "The Slice-01 demo command sequence in `slices/slice-01-walking-skeleton.md` runs end-to-end in CI without manual intervention; this is the structural test that defends KPI 1" | RESOLVED |
| 13 | KPI 8 CI time-budget unspecified | Add wall-clock estimate to KPI 8 measurement plan | YES — note added below the KPI 8 row: "1000 restarts plus drain time per restart fits in roughly 5–10 minutes of wall-clock CI time. DEVOPS calibrates the trigger cadence" | RESOLVED |
| 14 | /v1/profile vs /v1/profiles tightening | Update to /v1/profiles (actual OTel-canonical path) | YES — `journey-aperture.feature` updated; US-AP-02's third domain example untouched (it still uses /v1/profile but that example refers to a generic unknown path, and the UAT scenario is the contract-defending text) | RESOLVED |
| 15 (low) | Story-ID vs priority-label clarification | Add note to `prioritization.md` Backlog suggestions | YES — note added: "Story IDs (`US-AP-NN`) are stable across the feature's lifecycle. Priority labels (`Pn`) in the table below are derived from the slice each story lands in and may shift if slice ordering changes during DESIGN" | RESOLVED |

All 14 (15 with the low-severity numbering note) iteration-1 findings are RESOLVED.

---

## Re-check of dimensions

| Dimension | Iteration 1 | Iteration 2 |
|---|---|---|
| 0 — Elevator Pitch | PASS_WITH_NOTE (Slice 07 @infrastructure justified) | PASS — no change required |
| 1 — Confirmation Bias | 4 issues (3 placeholder defaults + missing rejected-alternatives) | RESOLVED — all four addressed |
| 2 — Completeness | 5 issues (stderr-backpressure UAT, memory NFR, SIGTERM-boundary, overlapping-bind, Sieve UAT) | RESOLVED — all five addressed |
| 3 — Clarity | 3 issues (max_recv_msg_size inconsistency, story-ID vs priority-label, /v1/profile) | RESOLVED — all three addressed |
| 4 — Testability | 3 issues (KPI 3 instrument, KPI 1 not test-defended, KPI 8 time-budget) | RESOLVED — all three addressed |
| 5 — Priority Validation | PASS — q1 YES, q2 ADEQUATE, q3 CORRECT, q4 JUSTIFIED, verdict PASS | PASS — no change required, q2 strengthened by the OtlpSink rejected-alternatives subsection |

---

## Final iteration-2 verdict

```yaml
review_id: "req_rev_20260504_124500"
reviewer: "product-owner (review mode)"
artifact: "docs/feature/aperture/discuss/* (full DISCUSS package, post-revisions)"
iteration: 2

verdict: "APPROVED"

critical_issues_count: 0
high_issues_count: 0
medium_issues_count: 0
low_issues_count: 0

approval_status: "approved"

dimensions:
  - dimension: 0
    name: "Elevator Pitch Test"
    verdict: "PASS"
  - dimension: 1
    name: "Confirmation Bias Detection"
    verdict: "PASS"
  - dimension: 2
    name: "Completeness Validation"
    verdict: "PASS"
  - dimension: 3
    name: "Clarity and Measurability"
    verdict: "PASS"
  - dimension: 4
    name: "Testability"
    verdict: "PASS"
  - dimension: 5
    name: "Priority Validation"
    verdict: "PASS"

handoff_recommendation: |
  The DISCUSS package is approved for handoff to:
    - Morgan (nw-solution-architect, DESIGN wave) — primary recipient.
    - Pat (nw-platform-architect, DEVOPS wave) — receives outcome-kpis.md and the named CI invariants.
    - Quinn (nw-acceptance-designer, DISTILL wave) — receives journey-aperture.yaml, journey-aperture.feature, integration points.

  No CRITICAL or HIGH issues remain. The 9 user stories pass DoR. The eight slices pass the six Elephant-Carpaccio taste tests. The KPIs are consumer-measurable and test-defended. The locked scope (Andrea's six Q&A items + Slice 01 shape) is honoured throughout.

  DISCUSS wave: CLOSED.
```

---

## Reviewer's closing remarks

The author addressed every iteration-1 finding additively — no structural rework was required, no contracts were re-litigated. The package's posture before iteration 1 was already fundamentally sound; iteration 1's job was to round out the rationale trail and tighten a handful of testability gaps, and iteration 2's job was to verify that work landed cleanly. Both are done.

The most material improvements from iteration 1 to iteration 2 are:

1. **The OtlpSink rejected-alternatives subsection in D2** — this is what Morgan needs at DESIGN time, and having it in DISCUSS means DESIGN can move forward without re-deriving the contract.
2. **The memory-bound NFR derived from the cap × body × transport product** — this is the kind of constraint that bites operators in production if it is not flagged in DISCUSS.
3. **KPI 3's restructure from soft-survey to hard-structural-with-soft-secondary** — this is the kind of testability fix that pays dividends every CI run.

The rest are small but not insignificant: rationale-completion turns "the team picked 1024" into "the team picked 1024 because of these specific traffic assumptions, which DESIGN can now revisit on the same evidence basis", which is the difference between an artefact and a working agreement.

DISCUSS wave is approved. Hand off to DESIGN.
