# DoR Validation — sluice-v0

| # | Item | Evidence | Status |
|---|---|---|---|
| 1 | Personas explicit | Sasha (platform engineer wiring the queue between Sieve and downstream) and Riley (SRE investigating backlog incidents) named. | ✓ |
| 2 | Jobs-to-be-Done articulated | Two stories with Elevator Pitches. The job is "decouple Sieve from downstream consumers so a brief outage absorbs without data loss". | ✓ |
| 3 | Acceptance criteria testable | 4-7 numbered ACs per story. ACs reference typed Result variants + observable queue depth. | ✓ |
| 4 | KPIs quantitative | Two outcome KPIs (enqueue/dequeue latency p95 ≤ 50µs; depth O(1)). | ✓ |
| 5 | Slices are elephant carpaccio | Two slices implied by stories 01-02. Each ≤ 1 day. | ✓ |
| 6 | External dependencies enumerated | aegis::TenantId as the queue key; std collections (VecDeque, HashMap) for the in-memory adapter; no Kafka / NATS / Redpanda at v0. | ✓ |
| 7 | Constraints documented | System constraints 1-9; D1-D10 in wave-decisions.md. | ✓ |
| 8 | Architectural anchor identified | Architecture doc names Sluice's role in the OTLP-OTLP-OTLP pipeline between Sieve and the storage plane. | ✓ |
| 9 | Definition of Done articulated per story | Each story names its KPI anchor. | ✓ |

## Outcome

All 9 DoR items pass. DISCUSS → DESIGN authorised. DESIGN
collapses into the implementation commit per the Loom slice 01
+ Aegis precedents — Sluice v0 is small enough that a separate
DESIGN artefact would be ceremony.
