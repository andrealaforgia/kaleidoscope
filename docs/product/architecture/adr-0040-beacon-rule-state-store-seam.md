# ADR-0040 — Beacon rule-state store seam

**Status**: Accepted
**Date**: 2026-05-21
**Author**: Morgan (nw-solution-architect, DESIGN dispatch)
**Companion feature**: `docs/feature/beacon-durable-alert-state-v0/`

## Context

beacon-server holds each rule's alert state
(`Inactive` / `Pending { since }` / `Firing { since }`) as a plain
local variable in the per-rule Tokio task
(`crates/beacon-server/src/main.rs`, `run_rule`, line 146:
`let mut state = RuleState::Inactive;`). The local is re-seeded to
`Inactive` on every process start, so a restart loses all in-flight
state: firing rules reset and re-page on-call, and pending dwell clocks
restart from zero.

Two design forces are in tension:

1. **ADR-0037 mandates a pure transition.**
   `transition(state, outcome, rule, now) -> (next_state, emission)`
   is total and side-effect-free. No persistence may live inside it.
2. **Durability requires state holding to be persisted.** The platform
   already has a proven durable-adapter pattern in the six storage
   pillars (cinder, sluice, lumen, pulse, ray, strata): a `*Store`
   trait + an in-memory v0 seam + a `FileBacked` v1 adapter with WAL
   append, `snapshot()` truncation, and `open()` recovery, plus an
   additive `PersistenceFailed { reason }` error variant.

This ADR records two decisions that a future reader could otherwise get
wrong: (a) *where the persistence seam sits so ADR-0037 is preserved*,
and (b) *that beacon's recovery semantics differ from every storage
pillar*.

## Decision

### 1. The store sits beside the pure transition, not inside it

Introduce a `RuleStateStore` port in a new `beacon::state_store` module,
a sibling of `beacon::state_machine`. The orchestrator loop reads the
previous state, calls the unchanged pure `transition`, and persists the
next state through the store. The store holds values; it contains no
transition logic. ADR-0037 is preserved exactly as the storage pillars
preserve the purity of their record and predicate types: state holding
and state transition are different responsibilities in different
modules.

The trait is two methods:

- `load_all() -> Result<HashMap<String, RuleState>, RuleStateStoreError>`
  — recovery entry point, called once at startup.
- `put(&str, RuleState) -> Result<(), RuleStateStoreError>` — upsert,
  called by the loop only when `state != next`.

Keyed on the rule name `String` (`Rule.name` is the established
identity); no `RuleId` newtype, which would not earn its place. Two
adapters: `InMemoryRuleStateStore` (v0 test seam) and
`FileBackedRuleStateStore` (v1 durable, WAL NDJSON + JSON snapshot +
recovery). `RuleState` gains a plain `#[derive(Serialize, Deserialize)]`
— `SystemTime` serialises natively as a duration since `UNIX_EPOCH`, so
no custom conversion is needed and there is no `Instant` problem.

### 2. Recovery is keyed-latest-wins, not append-and-sort

The storage pillars treat each WAL record as an event in a time series:
recovery replays every record and **re-sorts** each bucket by
`time_unix_nano` (see `strata::file_backed`, the sort after replay).

beacon is different. A rule has exactly one current state, not a
history. The WAL record is `WalRecord::Put { rule_id, state }`
(`#[serde(tag = "op")]`). Recovery replays Put records **in file order**
and the **last Put per `rule_id` wins**; the snapshot is just the
current `HashMap<String, RuleState>`. **There is no sort step and no
time-ordering of values**, because the value *is* the state, not an
event in a series.

The FileBacked skeleton (`open` / `snapshot` / append) is reused from
the pillar pattern; only the replay rule changes from "push then sort"
to "insert overwrite".

### 3. Recover-then-refuse at the composition root

beacon-server opens the store at startup; a corrupt or unreadable state
file makes `open()` return `PersistenceFailed`, and the binary logs a
clear error and exits non-zero rather than starting with silently reset
state. States for rules no longer in config are dropped and logged, not
resurrected.

## Alternatives considered

### A. Persist inside `run_rule` directly with `serde_json` calls (no port)

Rejected. It couples the orchestrator loop to a concrete file format,
makes the loop untestable without a real filesystem, and tempts a
future edit to fold persistence into the transition, eroding ADR-0037.
The port keeps the durable adapter swappable for the in-memory test
double and keeps the pure function pure.

### B. Reuse a storage pillar's append-and-sort recovery verbatim

Rejected. The pillars sort replayed records by time because each is an
event in a series. Applying that to rule state would be wrong: a rule's
state is a single current value, not a series, and there is no
meaningful time-sort of "the current state". Copying a pillar's
recovery code unchanged would introduce a latent ordering bug. This ADR
exists partly to stop that copy-paste.

### C. Introduce a `RuleId(String)` newtype as the key

Rejected (for v0). `Rule.name` is already the single identity, already a
`String`, already used as a map key by `InhibitionResolver`. A newtype
adds ceremony without enforcing a new invariant. Revisit only if a
second competing string identity ever appears.

## Consequences

- **Positive**: durability with zero change to the pure transition;
  ADR-0037 preserved and made enforceable (a module-dependency test
  asserts `state_machine` never imports `state_store`). The recovery
  contrast is documented so it cannot be copy-pasted wrong. The
  in-memory seam keeps the loop unit-testable.
- **Positive**: reuses the platform's proven WAL + snapshot + recovery
  shape, so the durable adapter is familiar to any pillar reader.
- **Negative**: beacon now carries real durable logic (WAL replay,
  recovery, snapshot truncation, refuse-on-corrupt). beacon is **not**
  mutation-gated today (`gate-5-mutants-beacon` absent from CI). A
  mutation gate scoped to the new `state_store` source is now warranted;
  the decision is left to DEVOPS (Apex) but the expectation is
  annotated.
- **Negative**: an additive `RuleStateStoreError::PersistenceFailed`
  variant the in-memory adapter never returns — the same modest cost the
  storage pillars already pay.
