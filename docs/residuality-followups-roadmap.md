# Residuality follow-up roadmap

British English. No em dashes.

Origin: `docs/product/architecture/residuality-analysis.md` (commit
50e20b5). That analysis surfaced three small, real defects in the
current substrate. This file pins the order in which to address them,
each as its own feature through full nWave.

## Order and rationale

The three follow-up features land one after another, in this order:

### 1. earned-trust-fsync-probe-v0 (M-1)

The platform claims Earned-Trust at startup (ADR-0042 Decision 8 and
its reproductions in ADR-0047 and ADR-0048), but the existing probes
verify open-and-read, not survive-a-crash via fsync. Five storage
pillars (pulse, lumen, ray, cinder, strata, sluice) and the beacon
rule-state store all rely on the fsync that the WAL recovery
discipline (ADR-0040) implicitly assumes; none probe it. This is a
claim contradicted by code. It goes first: closing the gap restores
the meaning of the Earned-Trust principle before we add anything else.
Likely a new ADR; the change is the binary-startup path of the
storage adapters.

### 2. honest-read-caps-v0 (M-2)

The three read APIs (query-api, log-query-api, and trace-query-api
once it ships) have no per-request window cap nor result-size cap. An
unbounded window is a self-DoS surface, exactly the class the residual
analysis flagged. One small slice puts both caps on all three crates,
with honest 400s for over-spec requests rather than a silent timeout
or OOM. The three crates share the cap pattern even though they keep
their own time-range types (which is why this is one feature, not
three separate slices).

### 3. pulse-cardinality-watermark-v0 (M-4)

The series index in pulse has no per-tenant cap. A label-cardinality
bomb at ingest pushes the process to OOM, because every distinct full
label set creates a new entry (ADR-0045). A soft watermark in
`apply_ingest` refuses further new label sets above a per-tenant
threshold and surfaces a recordable event, while existing series keep
ingesting normally.

## What this roadmap does NOT include

- M-3 in the analysis (implement trace-query-api) is already in flight
  as `ray-query-api-v0` and lands before this roadmap begins.
- M-5 (extract a `query-http-common` shared crate, the rule-of-three
  call from ADR-0048) was deliberately deferred as a separate future
  feature, not a residuality follow-up.
- Multi-region, sharding, write quorum and similar production
  concerns are out of scope for v0/v1, as the analysis stated.

## Ground rules

Each feature runs through FULL nWave, every wave dispatched to its
own agent (memory `feedback_never_hand_author_nwave_waves`):

- DISCUSS via Luna
- DESIGN via Morgan, with an ADR if the change is load-bearing
- DEVOPS via Apex (slim when there is no new crate or dependency)
- DISTILL via Scholar, RED tests uncommitted and atomic with DELIVER
- DELIVER via Crafty, atomic commit, push, verified

Narrative and slide closure for each feature is written by Bea
(presentation prose is the documented exception). Throttle 2-5
minutes between agent tasks via ScheduleWakeup. No 1.0.0 bump under
any circumstance: that remains Andrea's call
(memory `feedback_semver_one_zero_is_andreas_call`).
