# Outcome KPIs: aperture-storage-sink-v0

## Feature: aperture-storage-sink-v0

### Objective
The Kaleidoscope platform runs end to end: OTLP sent to the gateway is faithfully
persisted into the durable pillars and is still there, unchanged, after a restart.

### North Star
**Translation fidelity plus durability**: an OTLP payload accepted by the gateway
is queryable from the pillar after a process restart, with zero loss and faithful
field mapping (round-trip identity).

### Outcome KPIs

| # | Who | Does What | By How Much | Baseline | Measured By | Type |
|---|-----|-----------|-------------|----------|-------------|------|
| KPI-1 | Operator running the gateway with storage sink | Sends OTLP logs and finds them in lumen after a restart, field-faithful | 100% of accepted log records queryable post-restart with byte-stable field mapping (zero loss) | 0% (ray and pulse have no production consumer; nothing persists today) | Round-trip integration test: export -> restart -> query, assert field equality | Leading (outcome) |
| KPI-2 | Operator running the gateway with storage sink | Sends OTLP traces and finds them in ray after a restart, field-faithful | 100% of accepted spans queryable post-restart; trace/span ids, parent, kind, status, events, links faithful | 0% | Round-trip integration test for traces | Leading (outcome) |
| KPI-3 | Operator running the gateway with storage sink | Sends OTLP metrics and finds them in pulse after a restart, field-faithful | 100% of accepted gauge/sum points queryable post-restart; name, unit, kind, value, attributes faithful | 0% | Round-trip integration test for metrics | Leading (outcome) |
| KPI-4 | Operator under steady load | Experiences accept latency (translate + persist per payload) within budget | p95 translate+persist <= 50 ms per payload on GitHub Actions ubuntu-latest | none (new path) | Bench/integration timing harness in CI, ubuntu-latest | Guardrail |
| KPI-5 (correctness guardrail) | Any record the gateway accepts | Is never silently lost or partially persisted | 0 accepted-but-absent records; an untranslatable record is refused (not partially written), with a reason naming the field | none | Property test: for every accepted record, query returns it; refused records write nothing | Guardrail |

### Metric Hierarchy
- **North Star**: round-trip fidelity + durability (KPI-1/2/3 at 100%).
- **Leading Indicators**: per-signal post-restart queryability (KPI-1, KPI-2, KPI-3).
- **Guardrail Metrics**: accept latency budget on CI hardware (KPI-4); no silent
  loss / no partial persistence (KPI-5).

### CI realism (the 2026-05-19 lesson)
KPI-4's latency budget is set against **GitHub Actions ubuntu-latest**, not a fast
workstation. The threshold is asserted in CI on that runner class so the budget is
honest from the first commit. KPI-5 is the correctness guardrail: fidelity is
worthless if records can vanish, so "every accepted record is queryable, and every
refused record writes nothing" is enforced as a property.

### Measurement Plan
| KPI | Data Source | Collection Method | Frequency | Owner |
|-----|------------|-------------------|-----------|-------|
| KPI-1/2/3 | Pillar query results pre/post restart | Round-trip integration test per signal | Per CI run | crafter (DELIVER) |
| KPI-4 | Per-payload translate+persist timing | Timing harness on ubuntu-latest | Per CI run | crafter (DELIVER) |
| KPI-5 | Accepted vs queried record sets; refused-record side effects | Property test | Per CI run | crafter (DELIVER) |

### Hypothesis
We believe that adding a storage `OtlpSink` for the operator running the gateway
will achieve a platform that runs end to end. We will know this is true when an
operator sends OTLP to the gateway and finds 100% of accepted records queryable
from the pillar after a restart, field-faithful, within the CI latency budget.
