# Definition of Ready Validation: cinder-wal-error-surfacing-v0

Validated by Luna (nw-product-owner). 9-item hard gate. All stories must PASS before DESIGN handoff.

## Story: US-01 — Cinder place() surfaces a WAL persistence failure

| DoR Item | Status | Evidence/Issue |
|----------|--------|----------------|
| Problem statement clear, domain language | PASS | Priya reads a placement that vanishes on restart because `place()` swallows the WAL error (`file_backed.rs:270-278`); acked-but-not-durable lie. |
| User/persona identified | PASS | Priya the platform operator running a live multi-tenant gateway with cinder in the ingest path. |
| 3+ domain examples with real data | PASS | Happy (`acme/trade-001` healthy), Error (`acme/trade-002` failing disk), Edge (`globex/batch-007` failed overwrite preserves Hot). |
| UAT scenarios (3-7) | PASS | 4 Gherkin scenarios (failing-disk fails loud, un-persisted not readable, failed overwrite preserves prior, healthy-disk negative control). |
| AC derived from UAT | PASS | 4 AC each traced to a scenario. |
| Right-sized (1-3 days, 3-7 scenarios) | PASS | 4 scenarios; the trait change + one adapter method + ripple. ~1 day core. |
| Technical notes: constraints/dependencies | PASS | C2 write-ahead ordering, C3 public-API change, C5 FsyncBackend seam, InMemory returns Ok, caller ripple listed. |
| Dependencies resolved or tracked | PASS | `MigrateError::PersistenceFailed` exists (`store.rs:49`); `open_with_fsync_backend` seam exists (ADR-0060). No external blockers. |
| Outcome KPIs defined with measurable targets | PASS | KPI-1: swallow sites 1->0, falsifiable failing-substrate AC. |

### DoR Status: PASSED

## Story: US-02 — Live gateway ingest handles a tier-placement persistence failure

| DoR Item | Status | Evidence/Issue |
|----------|--------|----------------|
| Problem statement clear, domain language | PASS | `flush()` (`lib.rs:265`) ignores the error channel; the ingest is reported green even when tier metadata was never persisted. |
| User/persona identified | PASS | Priya driving the live gateway ingest path. |
| 3+ domain examples with real data | PASS | Happy (healthy ingest, 4200 records / 5 batches), Error (fail-the-ingest on batch 4), Edge (log-and-continue with non-silent summary). |
| UAT scenarios (3-7) | PASS | 3 scenarios (healthy negative control, never-falsely-green, follows-D2-decision). |
| AC derived from UAT | PASS | 3 AC traced to scenarios. |
| Right-sized | PASS | One caller-handling site + the D2 policy. ~0.5 day once US-01 lands. |
| Technical notes | PASS | Depends on US-01; D2 is the flagged operator-visible decision; CLI fns ride the same change. |
| Dependencies resolved or tracked | PASS | Depends on US-01 (tracked, same walking skeleton). D2 flagged for DESIGN. |
| Outcome KPIs defined | PASS | KPI-2: silent ingest tier-persist failures 100%->0% surfaced. |

### DoR Status: PASSED

## Story: US-03 — Cinder evaluate_at() surfaces a sweep persistence failure

| DoR Item | Status | Evidence/Issue |
|----------|--------|----------------|
| Problem statement clear, domain language | PASS | `evaluate_at` swallows each migration's WAL error (`file_backed.rs:364`); the count overstates durability; migrations vanish on restart. |
| User/persona identified | PASS | Priya running periodic age-based sweeps via `evaluate-policy`. |
| 3+ domain examples with real data | PASS | Happy (30 acme items healthy), Error (fail-whole on 8th of 20), Edge (partial count 7 of 20). |
| UAT scenarios (3-7) | PASS | 3 scenarios (healthy count==durable negative control, never-overstate, follows-D3-decision). |
| AC derived from UAT | PASS | 3 AC traced to scenarios. |
| Right-sized | PASS | One adapter method + the D3 semantics. ~0.5-1 day on the US-01 spine. |
| Technical notes | PASS | C3 public change, C2 per-migration ordering, D3 flagged, caller `evaluate_policy()` listed. |
| Dependencies resolved or tracked | PASS | Depends on US-01's trait change (tracked). D3 flagged for DESIGN. |
| Outcome KPIs defined | PASS | KPI-3: per-migration swallow 1->0; count==durable count. |

### DoR Status: PASSED

## Story: US-04 — Sluice surfaces its three swallowed WAL failures (uniformity)

| DoR Item | Status | Evidence/Issue |
|----------|--------|----------------|
| Problem statement clear, domain language | PASS | sluice swallows at `dequeue`/`ack`/`nack` (`file_backed.rs:346,356,366`); same lie as cinder; UNWIRED (zero live blast radius, verified). |
| User/persona identified | PASS | The future operator who wires sluice + the platform maintainer keeping pillar posture uniform. |
| 3+ domain examples with real data | PASS | Happy (healthy dequeue+ack), Error (failing-disk dequeue), Edge (failing-disk ack/nack). |
| UAT scenarios (3-7) | PASS | 3 scenarios (healthy negative control, dequeue surfaced, ack/nack surfaced). |
| AC derived from UAT | PASS | 3 AC traced to scenarios. |
| Right-sized | PASS | Three swallow-site fixes in one adapter. ~0.5 day. Splittable to a separate feature if needed. |
| Technical notes | PASS | sluice unwired; D4 surfacing channel flagged; `EnqueueError::PersistenceFailed` exists; ADR-0060 §6 recovery note. |
| Dependencies resolved or tracked | PASS | Independent of cinder (separate crate). D4 flagged. Carpaccio cut-line if cinder ripple large. |
| Outcome KPIs defined | PASS | KPI-4: sluice swallow sites 3->0; pillar posture uniform. |

### DoR Status: PASSED

## Elevator Pitch Test (Dimension 0 — checked per story)

| Story | Before/After/Decision present? | Real entry point? | Concrete output? | Job connection? | Verdict |
|---|---|---|---|---|---|
| US-01 | Yes | `kaleidoscope place` + ingest path (user-invocable) | stderr `error: persistence failed: io: ...` + `get-tier` returns no placement | Priya investigates the failing disk now vs after a vanished-data restart | PASS |
| US-02 | Yes | `kaleidoscope ingest` (user-invocable) | ingest summary / stderr never falsely green | Priya decides retry vs fix-disk | PASS |
| US-03 | Yes | `kaleidoscope evaluate-policy` (user-invocable) | `evaluated migrated=N` honest count + surfaced failure | Priya trusts the count == durable | PASS |
| US-04 | Yes (`@uniformity`, no live entry point today — sluice unwired; explicitly flagged) | library/test seam (sluice unwired) | failing-substrate test surfaces rather than swallows | future operator inherits fail-loud queue | PASS with note |

Note on US-04: it is the single `@uniformity` story with no live operator entry point (sluice is
unwired). Per Dimension 0 §5, a slice where EVERY story is infrastructure is blocking; here US-04 is the
ONLY such story and it is isolated in its own release (R3), while R1/R2 (US-01..US-03) each carry a
user-visible operator outcome. The slice-level value test passes: the feature's live value rides US-01..
US-03; US-04 is a deliberate, separable uniformity addition, not the whole release.

## Overall: PASSED — all 4 stories meet the 9-item gate; peer review next.
