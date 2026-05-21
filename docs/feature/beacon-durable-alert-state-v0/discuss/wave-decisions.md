# Wave Decisions — beacon-durable-alert-state-v0

DISCUSS wave (nw-product-owner / Luna). Backend feature in the
Kaleidoscope Rust workspace.

## Configuration decisions (decided, not asked)

| # | Decision | Value | Rationale |
|---|----------|-------|-----------|
| 1 | Feature type | Backend | No CLI/HTTP surface change; the seam is internal to beacon-server. |
| 2 | Walking skeleton | No (brownfield) | beacon and beacon-server already exist and ship value; this extends them. |
| 3 | UX research depth | Lightweight | Operator-facing behaviour only; persona is the on-call operator, no interactive UI. |
| 4 | JTBD | Skipped | Operational gap is verified and concrete; went straight to journey + story map + requirements. |

## DIVERGE artifacts

Absent. No `docs/feature/beacon-durable-alert-state-v0/diverge/`
directory. Per workflow, this is noted as a risk: stories are grounded
in the orchestrator-verified operational gap rather than a validated
DIVERGE job statement. The gap is concrete and code-confirmed, so the
risk is low.

## The operational gap (code-confirmed)

`crates/beacon-server/src/main.rs`, line 146, inside `run_rule`:

```rust
let mut state = RuleState::Inactive;
```

Each per-rule Tokio task owns its `RuleState` as a plain local
variable. It is re-seeded to `Inactive` every time the task starts,
which on a process restart is every rule. There is zero persistence.

`RuleState` (confirmed in `crates/beacon/src/state_machine.rs`) has
three variants: `Inactive`, `Pending { since: SystemTime }`,
`Firing { since: SystemTime }`. `Resolved` is an `Emission`, not a
state. The `since` field on `Pending` and `Firing` is the durable
payload that matters: losing it restarts the `for_duration` dwell clock
and forgets that a rule was firing.

Consequence on restart:

- A rule in `Firing` resets to `Inactive`: no `Resolved` emission is
  ever sent, and the rule re-fires from scratch on the next active
  observation, re-paging on-call.
- A rule in `Pending { since }` loses `since`: the `for_duration` dwell
  clock restarts, delaying a legitimate alert.

## Architectural constraint (ADR-0037, must not break)

`transition(state, outcome, rule, now) -> (next_state, emission)` stays
a pure, total, side-effect-free function. ADR-0037 (Accepted,
2026-05-11) mandates this. The feature does NOT touch `transition`. It
introduces a state-holding seam OUTSIDE the pure function: a
`RuleStateStore` trait that holds the per-rule `RuleState`. The
orchestrator reads previous state from the store, calls the pure
`transition`, and persists the next state. Purity is preserved because
the store is a separate port, exactly mirroring the storage pillars
where `LogStore` holds state and the record/predicate types stay pure.

## Shape precedent (confirmed)

`crates/lumen/src/store.rs` and `crates/lumen/src/file_backed.rs` are
the SHAPE precedent (one of six storage pillars: cinder, sluice, lumen,
pulse, ray, strata all ship `file_backed.rs`):

- `LogStore` trait + `InMemoryLogStore` (v0 test seam) +
  `FileBackedLogStore` (v1 durable: `open` recovers snapshot then
  replays WAL; `snapshot` truncates WAL; `ingest` appends to WAL).
- `LogStoreError::PersistenceFailed { reason: String }` — additive
  error variant the in-memory adapter never returns.

The new `RuleStateStore` follows this verbatim. A rule-state store is
far lighter than a log store: a small map of `enum + Option<SystemTime>`
keyed by rule name, not batches of OTLP records.

## Slicing decision (elephant carpaccio)

Two thin slices (see `slices/`):

1. `slice-01` — `RuleStateStore` trait + `InMemoryRuleStateStore`,
   rewire `run_rule` to hold state through the store rather than a local
   variable. Behaviour-preserving refactor. Ships NO operator-visible
   change on its own. Labelled `@infrastructure`.
2. `slice-02` — `FileBackedRuleStateStore` (WAL + snapshot + recovery),
   wired into beacon-server startup so state survives restart. THIS
   slice carries the Elevator Pitch and the operator value.

Slice 01 is honestly `@infrastructure` and does not block the feature
because slice 02 delivers the value end to end. Carpaccio taste tests
applied in `slices/`.

## CI realism (hard project lesson, 2026-05-19)

All latency and recovery budgets are set against GitHub Actions
`ubuntu-latest`, NOT a fast workstation. Budgets mirror the storage
pillars (ingest p95 low-millisecond, recovery p95 <= 2.5 s) but a
rule-state store is lighter, so budgets are tight with CI margin. A
correctness guardrail KPI (zero spurious re-fires, 100% pending-since
fidelity) sits alongside the latency budgets. See `outcome-kpis.md`.

## Risks surfaced (not managed here)

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| No DIVERGE job validation | Low | Low | Gap is code-confirmed and concrete. |
| `RuleState` not currently `Serialize`/`Deserialize` | Medium | Medium | DESIGN wave adds derives; flagged in slice-02 technical notes. |
| Stale recovered state (rule deleted from config between restarts) | Medium | Low | Recover only states whose rule still exists; flag in DESIGN. |
| Clock skew on recovered `since` across restart | Low | Medium | `since` is wall-clock `SystemTime`; dwell maths already uses `unwrap_or_default`. Flag for DESIGN. |
```