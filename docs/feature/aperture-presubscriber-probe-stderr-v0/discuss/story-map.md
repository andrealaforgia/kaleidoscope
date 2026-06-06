# Story Map — aperture-presubscriber-probe-stderr-v0

## User: Priya Nair (SRE operating aperture as an OTLP forwarding gateway)
## Goal: Start aperture against a downstream and, if it refuses, learn WHY from stderr alone

## Backbone

| Start the gateway | Probe the downstream | Refuse if unhealthy | Surface the reason |
|-------------------|----------------------|---------------------|--------------------|
| run aperture w/ config | Earned-Trust probe runs | exit non-zero, bind nothing | structured stderr line (US-01) |

The first three activities already EXIST and work correctly in source
(`run()` → `wire_sink`/`probe_or_refuse` → fail-closed exit). The fourth
activity — "Surface the reason" — is the gap: it works for the config
path but is silent for the probe-refusal path. This story completes the
backbone's final activity for the probe-refusal case.

---

## Walking Skeleton

Not applicable — brownfield. The startup → probe → fail-closed path is
already a working end-to-end skeleton. This feature is a single thin
brownfield slice that completes the last activity ("surface the reason")
for the one case where it is currently silent. The slice is itself
end-to-end: binary start → probe refusal → stderr line → non-zero exit.

## Release 1: Operator sees the refusal reason (the only release)

- **Tasks**: US-01 (the pre-subscriber probe-refusal stderr line).
- **Target outcome**: probe-refusal starts emit an operator-visible reason
  line 100% of the time (from 0%); zero silent exit-1 startups.
- **KPI targeted**: KPI-1 (operator identifies startup-refusal cause from
  stderr alone) — see outcome-kpis.md.
- **Rationale**: single user outcome, single thin slice; no further
  release exists for this feature.

---

## Priority Rationale

There is exactly one story, so prioritisation is degenerate, but the
ordering logic is recorded for the handoff:

| Priority | Story | Value | Urgency | Effort | Score | Why |
|----------|-------|-------|---------|--------|-------|-----|
| 1 | US-01 | 4 | 4 | 2 | 8.0 | Closes a swallowed-errors-family honesty gap a verifier is actively blocked on (A19/A20); low effort (mirrors an existing precedent); high operator value (turns a silent exit into an actionable line). |

- **Value 4**: directly moves the north-star (no silent startup exits) and
  unblocks a verifier's evidence widening. Not 5 because it is observability,
  not a new capability.
- **Urgency 4**: a verifier flagged it and committed to confirming it on
  landing; it is in an actively-audited family (swallowed-errors).
- **Effort 2**: a narrow change against a path with an existing
  direct-stderr precedent; the riskier mechanism option (subscriber-earlier)
  is DESIGN's to weigh.
- **Dependencies**: none blocking.

---

## Scope Assessment: PASS — 1 story, 0 additional bounded contexts, estimated ~1 day

Elephant Carpaccio gate evaluated against the five oversized signals:

| Signal | Threshold | This feature | Trips? |
|--------|-----------|--------------|--------|
| User stories | >10 | 1 | No |
| Bounded contexts / modules | >3 | 1 (aperture startup path) | No |
| Walking-skeleton integration points | >5 | n/a (brownfield) | No |
| Estimated effort | >2 weeks | ~1 day | No |
| Independent shippable outcomes | multiple | 1 | No |

Zero of five signals trip. The feature is right-sized as a single thin
end-to-end slice. Splitting further would yield fragments with no
independent operator value (the AC are a coherent set describing one
observable behaviour). No split proposed; proceed.
