# Story Map — beacon-durable-alert-state-v0

## User: Priya Nair — on-call platform operator
## Goal: Restart beacon-server without losing alert state or re-paging on-call

## Backbone

| Hold state through a seam | Persist state durably | Recover state at startup | Evaluate from recovered state |
|---------------------------|-----------------------|--------------------------|-------------------------------|
| RuleStateStore trait      | WAL append on transition | open() replays snapshot+WAL | seed loop from store |
| InMemoryRuleStateStore    | snapshot + WAL truncate  | report recovery counts      | pure transition() unchanged |
| rewire run_rule loop      | PersistenceFailed error  | drop states for deleted rules | no re-page for still-firing |

---

## Note on the walking skeleton

This is a brownfield feature (Decision 2: walking skeleton = No).
beacon and beacon-server already ship an end-to-end evaluate-and-emit
pipeline. There is no fresh skeleton to draw; instead the two slices
extend the existing pipeline. Slice 01 introduces the seam
(behaviour-preserving), slice 02 makes it durable and delivers the
operator value.

## Slice 01 (`@infrastructure`): introduce the state-holding seam

Tasks: `RuleStateStore` trait + `InMemoryRuleStateStore`; rewire
`run_rule` so the per-rule loop reads/writes state through the store
instead of the `let mut state = RuleState::Inactive` local. Behaviour
is identical to today (in-memory, lost on restart). Ships no
operator-visible change. Target outcome: a clean seam that the durable
adapter can drop into without touching `transition`.

## Slice 02 (value): durable persistence + recovery wired into startup

Tasks: `FileBackedRuleStateStore` (WAL append on each transition,
`snapshot()` truncates WAL, `open()` recovers snapshot then replays
WAL); `PersistenceFailed` error variant; wire `open()` into
beacon-server startup; seed each per-rule loop from recovered state;
emit the recovery-count log line; drop states for rules no longer in
config. Target outcome: **alert state survives restart, firing stays
firing, pending clock preserved, on-call not re-paged.**

## Priority Rationale

Priority order: **Slice 01 -> Slice 02**.

1. Slice 01 first because it is the riskiest-assumption-low,
   dependency-zero refactor that creates the seam. It must land before
   slice 02 can plug in a durable adapter without breaking ADR-0037.
   It is `@infrastructure`: necessary, but delivers no standalone
   operator value, so it is never shipped alone as the feature's value.
2. Slice 02 second because it carries the entire operator outcome (the
   Elevator Pitch). It depends structurally on slice 01's trait. It
   targets the north-star KPI (alert-state durability completeness) and
   the re-page guardrail. Highest value, must ship to close the gap.

Outcome impact dominates: slice 02 is the only slice that moves the
KPIs, so the sequence exists purely to make slice 02 safe to build.

## Scope Assessment: PASS — 3 stories, 2 contexts (beacon-server + beacon-state-store seam), estimated 3-4 days

Oversized signals checked (none triggered at the 2+ threshold):

- Stories: 3 (well under 10).
- Bounded contexts/modules: 2 — the new state-store seam and the
  beacon-server orchestrator. Under the 3 threshold.
- Integration points: 1 — the existing `run_rule` evaluation loop.
  Far under 5.
- Estimated effort: 3-4 days. Under 2 weeks.
- Independent shippable outcomes: 1 (durability). Slice 01 is
  infra-only, not an independent outcome.

Right-sized. No split required.
