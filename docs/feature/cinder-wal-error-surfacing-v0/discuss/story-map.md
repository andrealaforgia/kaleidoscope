# Story Map: cinder-wal-error-surfacing-v0

## User: Priya the platform operator

## Goal: trust that an acked tier placement or migration is actually durable on disk, and learn immediately when the disk cannot persist — instead of reading a placement that vanishes on the next restart

## Backbone

The operator's end-to-end journey through a tier operation that may hit a failing disk:

| Place an item | Drive ingest | Run a policy sweep | Trust the read |
|---|---|---|---|
| US-01: `place()` surfaces a WAL failure (cinder) | US-02: gateway `flush()` handles the failure | US-03: `evaluate_at()` surfaces a sweep failure | (read path: `get-tier` / `list-items` already exist; this feature makes their answers trustworthy) |
| US-04: sluice surfaces its three swallow sites (uniformity, unwired) | | | |

The trait signature change (place + evaluate_at -> Result) is the spine that runs UNDER the whole
backbone: every activity above the line depends on the error channel existing.

---

### Walking Skeleton

The thinnest end-to-end slice that makes the operator's read trustworthy on the live path:

- **US-01** — `TieringStore::place -> Result`, write-ahead ordering in `FileBackedTieringStore::place`,
  `InMemoryTieringStore::place` returns `Ok`, and the full caller ripple compiles. This is the
  load-bearing trait change.
- **US-02** — the LIVE gateway ingest path (`flush()`) handles the new `Result` per the D2 decision.

US-01 + US-02 together deliver a working, verifiable behaviour: a placement on a failing disk fails
loudly, a read does not return the un-persisted placement, and the live ingest path does not report a
falsely-green success. The trait change and the live-gateway caller handling are the load-bearing parts
and ship together (per the wave mandate). Everything else (the sweep, sluice) layers on top.

### Release 1 (Walking Skeleton): place fails loud on the live path

- Stories: **US-01**, **US-02**
- Target outcome: silent `place` WAL failures in the live ingest path move from always-silent to
  always-surfaced (0% -> 100%); a read never returns an un-persisted placement.
- KPI: KPI-1 (place surfacing), KPI-2 (ingest non-silent).
- Rationale: this is the live blast radius. cinder is in the running gateway path; the
  acked-but-not-durable lie is exploitable TODAY here. Validate it first, end-to-end, including the
  operator-visible D2 decision.

### Release 2: the policy sweep stops overstating durability

- Stories: **US-03**
- Target outcome: the reported migration count equals the durably-migrated count; sweep failures are surfaced.
- KPI: KPI-3 (sweep surfacing + count honesty).
- Rationale: `evaluate_at` is also live (via `evaluate-policy`), but it is a periodic operator action
  rather than the per-batch ingest hot path, and it carries the additional D3 partial-vs-fail-whole
  decision. It rides the same trait-change spine US-01 establishes, so it is cheaper once US-01 lands.

### Release 3: storage-pillar uniformity (sluice)

- Stories: **US-04**
- Target outcome: sluice's three swallow sites move from 3 to 0; the storage-pillar durability posture becomes uniform.
- KPI: KPI-4 (sluice uniformity).
- Rationale: sluice is UNWIRED (zero live blast radius today). Its value is preventing the cinder defect
  from being re-shipped under the queue pillar when sluice is eventually wired. Deliberately LAST so the
  live-value cinder work is never gated on uniformity work. Could even ship as a separate carpaccio
  follow-up if the cinder ripple proves large in DESIGN.

---

## Priority Rationale

Priority order: **US-01 -> US-02 -> US-03 -> US-04**, by outcome impact and dependency.

1. **US-01 first (walking skeleton, riskiest assumption)** — the trait signature change is the fatal
   assumption: can `place -> Result` be made to ripple cleanly through every caller (live gateway, CLI
   fns, InMemory impl, ~15 test files) without an unbounded blast radius? Validating the ripple is
   contained is the riskiest thing in the feature. It also unblocks everything else (the error channel
   must exist before any caller can handle it). Value 5 (moves the durability KPI on the live path),
   Urgency 5 (derisks the ripple), Effort 3.
2. **US-02 second (live-path value, same slice as US-01)** — the operator-visible D2 decision
   (fail-the-ingest vs log-and-continue) is load-bearing and belongs WITH the trait change per the wave
   mandate. Without it the trait change is inert on the live path. Value 5, Urgency 4, Effort 2.
3. **US-03 third** — live but lower-frequency (periodic sweep, not per-batch), and carries the extra D3
   decision. Cheaper once US-01's spine exists. Value 4, Urgency 3, Effort 2.
4. **US-04 last** — zero live blast radius (sluice unwired). Pure uniformity / landmine-removal. Ordered
   last so live work is never gated on it; splittable into a separate feature if cinder's ripple is
   large. Value 2, Urgency 1, Effort 2.

Tie-breaking applied: Walking Skeleton (US-01) > Riskiest Assumption (the ripple, also US-01) >
Highest Value (US-02). Every story traces to an outcome KPI (see `outcome-kpis.md`); no orphans.

---

## Scope Assessment: PASS — 4 stories, 2 modules (cinder, sluice), estimated 2-3 days

Elephant-Carpaccio gate evaluation against the oversized signals (oversized if 2+ true):

| Signal | Threshold | This feature | Over? |
|---|---|---|---|
| User stories | >10 | 4 | No |
| Bounded contexts / modules | >3 | 2 (cinder, sluice; the CLI is the caller, not a new context) | No |
| Walking skeleton integration points | >5 | 2 (the trait + the live `flush()` caller) | No |
| Estimated effort | >2 weeks | ~2-3 days | No |
| Independent shippable outcomes | multiple | The cinder fix (US-01..03) is one coherent outcome; US-04 is a separable uniformity outcome (already isolated to R3) | Borderline, already mitigated |

0-1 signals tripped (the multiple-outcomes signal is pre-mitigated by isolating sluice to R3 and noting
it is splittable). **Right-sized.** The trait change + live-gateway caller handling are the load-bearing
parts and stay together in the walking skeleton, exactly as the wave mandate directs. sluice (US-04) is
the natural carpaccio cut-line if DESIGN finds the cinder caller ripple larger than expected; the map
already places it in an independent final release so cutting it loses no cinder value.
