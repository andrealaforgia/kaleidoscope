# Story Map — `consolidated-runtime-v0`

## User: Andrea (and any Kaleidoscope contributor / CI) running the stack to experiment with it
## Goal: send telemetry in and immediately query it back out, in one process, without restarting anything

## Backbone (the experiment loop)

| A. Bring the runtime up | B. Send telemetry | C. Query it back live | D. Trust it for real use |
|-------------------------|-------------------|-----------------------|--------------------------|
| Start consolidated runtime (one command) | POST one OTLP metric | GET metric back from same process | Tenant isolation still holds |
| Bind ingest + all query ports on one process | POST one OTLP log | GET log back from same process | Auth/durability not regressed |
| Build one shared store per signal | POST one OTLP trace | GET trace back (window + by-id) | Empty store => empty success, not error |
| | | | All three signals live, one command (capstone) |

---

## Walking Skeleton (feature-level, = Slice 1 core)

The thinnest end-to-end slice that closes the loop for ONE signal:

> Start the consolidated runtime with an empty store → POST one OTLP metric to the ingest
> endpoint → GET `/api/v1/query_range` from the SAME process → the metric comes back, no
> restart.

That is **US-01**. It crosses all four backbone activities for metrics and validates the
riskiest assumption (a shared `Arc<Store>` gives live visibility, and ingest + query ports
co-bind on one runtime). Per D2 there is no greenfield skeleton — C1 composes proven
components — but US-01 is the feature's walking skeleton.

---

## Slice / Release plan

### Slice 1 — "I can send a metric and immediately see it" (metrics live loop)

- **US-01** — Live metric visibility in one process (walking skeleton; the north-star loop).
- **US-02** — Tenant isolation holds in the consolidated process (the safety guardrail that
  makes the consolidated shape trustworthy).
- **Target outcome (KPI)**: the send-then-see loop succeeds for metrics with NO restart, 100%
  of attempts; freshness within 1s p95. Cross-tenant reads stay empty.
- **Why first**: validates the load-bearing architecture assumption end to end on the simplest,
  already-Prism-visualised signal. Until this works, nothing else in the consolidation roadmap
  (C2 run story, C3 generator, C4 docs) has anything real to run.

### Slice 2 — "the whole signal set is live in one command" (logs + traces + capstone)

- **US-03** — Live log visibility in the consolidated process (`/api/v1/logs`, :9091).
- **US-04** — Live trace visibility in the consolidated process (`/api/v1/traces` window +
  `/api/v1/traces/by_id` lookup, :9092).
- **US-05** — One-command three-signal experiment loop (all three signals live, all five ports
  bound, one process — the demonstrable "consolidated runtime").
- **Target outcome (KPI)**: the same live-visibility property holds for all three signals; the
  experimenter brings the whole stack up with one command and exercises every signal without a
  restart.
- **Why second**: it is the SAME composition pattern from Slice 1 applied to two more signals
  (build store once, `Arc::clone` to sink + router, bind one more query port each), plus the
  capstone that asserts the three together. Lower architectural risk; higher completeness.

---

## Priority Rationale

Prioritised by outcome impact and the riskiest-assumption-first rule (Maurya), not by feature
grouping or ease.

| Priority | Story | Slice | Target outcome | Value x Urgency / Effort | Rationale |
|----------|-------|-------|----------------|--------------------------|-----------|
| 1 | US-01 | 1 | Metric send-then-see, no restart | 5 x 5 / 2 | Walking skeleton; derisks the whole single-process bet; the literal loop that fails today. |
| 2 | US-02 | 1 | Tenant isolation preserved | 4 x 4 / 1 | Guardrail: "is it safe to consolidate?" must be answered before the shape is trusted; cheap (reuses aegis). |
| 3 | US-03 | 2 | Log send-then-see, no restart | 4 x 3 / 1 | Repeats the proven pattern for logs; completes a second of three signals. |
| 4 | US-04 | 2 | Trace send-then-see, no restart | 4 x 3 / 2 | Repeats the pattern for traces; two query routes (window + by-id) make it slightly larger. |
| 5 | US-05 | 2 | Whole stack live, one command | 5 x 3 / 2 | Capstone: the demonstrable consolidated runtime; depends on US-01/03/04 being green. |

Tie-breaking applied: Walking Skeleton (US-01) > Riskiest Assumption / guardrail (US-02) >
highest-value completeness (US-03/04/05). Every story traces to the live-visibility north-star
KPI in `outcome-kpis.md`; there are no orphan stories.

Dependency chain: US-01 → US-02 (shares the metrics composition) → US-03, US-04 (independent
of each other, both depend on the US-01 pattern) → US-05 (depends on US-01, US-03, US-04).

---

## Scope Assessment: PASS — 5 stories, 1 feature (composition over already-wired crates), estimated ~3-5 days

Elephant Carpaccio oversize signals checked (oversized if any 2+ true):

- More than 10 user stories? **No** (5).
- More than 3 bounded contexts / modules? **No** — touches only already-wired crates
  (the gateway/composition + the three query-api crates over the existing pulse/lumen/ray
  stores); it adds a composition root, it does not introduce new domains.
- Walking skeleton needs more than 5 integration points? **No** — US-01 needs exactly one
  shared store + ingest bind + one query bind.
- Estimated effort over 2 weeks? **No** — composition of proven components; the seams already
  exist (`router`/`router_with_auth` accept an injected store; the gateway already `Arc`-shares
  the metric store into the sink).
- Multiple independent user outcomes that could ship separately? **Partially** — metrics-only
  (Slice 1) is independently demonstrable from logs+traces (Slice 2); that is exactly why they
  are two slices within one feature, not a reason to split into two features. C1 is defined by
  the roadmap as one spine.

Zero oversize signals tripped beyond the deliberate two-slice structure. **Right-sized; no
split. Proceeding as a single feature with two outcome slices.**
