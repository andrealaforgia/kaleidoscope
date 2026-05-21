<!-- markdownlint-disable MD024 -->

# User Stories — beacon-durable-alert-state-v0

## System Constraints

These cross-cutting constraints apply to every story below.

- **ADR-0037 is inviolable.** The `transition(state, outcome, rule, now)
  -> (next_state, emission)` function in
  `crates/beacon/src/state_machine.rs` stays pure, total, and
  side-effect-free. No story may add I/O inside it. State holding lives
  entirely in the new `RuleStateStore` seam, outside the pure function.
- **No new CLI or HTTP surface.** The wiring point is the existing
  `run_rule` evaluation loop and beacon-server startup. The only new
  operator-visible output is a startup recovery log line.
- **Shape precedent.** The new `RuleStateStore` trait + adapters mirror
  the storage-pillar pattern in `crates/lumen/src/store.rs` (trait +
  `InMemoryLogStore`) and `crates/lumen/src/file_backed.rs`
  (`FileBackedLogStore` with WAL append, `snapshot()`, `open()`
  recovery, additive `PersistenceFailed` error variant).
- **RuleState variants** (confirmed): `Inactive`,
  `Pending { since: SystemTime }`, `Firing { since: SystemTime }`.
  `Resolved` is an `Emission`, not a state. The `since` instant is the
  durable payload that matters.
- **British English, no em dashes** in prose.

---

## US-01: Hold per-rule alert state through a store seam (slice 01)

`@infrastructure`

### Problem

beacon-server holds each rule's alert state in a plain local variable.
In `crates/beacon-server/src/main.rs` line 146, inside `run_rule`, the
code reads `let mut state = RuleState::Inactive;`. This local is owned
by the per-rule Tokio task and cannot be persisted or recovered. Before
durability can exist, the state must be held through a seam that a
durable adapter can later implement, without touching the pure
`transition` function.

### Who

- beacon-server orchestrator | per-rule evaluation loop | needs a
  state-holding port that mirrors the storage-pillar trait pattern.

### Solution

Introduce a `RuleStateStore` trait (load state for a rule, persist the
next state for a rule, recover all states) and an
`InMemoryRuleStateStore` adapter. Rewire `run_rule` so the loop reads
the previous state from the store, calls the unchanged `transition`,
and writes the next state back. Behaviour is identical to today: state
still lives only in memory and is still lost on restart. This is a
behaviour-preserving refactor that creates the seam.

### Domain Examples

#### 1: Happy Path — steady-state evaluation unchanged

Rule "pay-latency" is Inactive. The query returns Active. Through the
`InMemoryRuleStateStore`, the loop reads Inactive, calls `transition`,
gets `Pending { since: now }`, writes it back. Identical to the current
local-variable behaviour.

#### 2: Edge Case — multiple rules isolated in one store

Rules "pay-latency" and "disk-fill" both run. Each reads and writes its
own entry keyed by rule name; "pay-latency" going Pending never changes
"disk-fill"'s stored state. Mirrors the per-tenant isolation of
`InMemoryLogStore`.

#### 3: Error/Boundary — restart still loses state (expected at this slice)

beacon-server restarts. The `InMemoryRuleStateStore` starts empty, so
every rule seeds Inactive, exactly as today. This slice deliberately
does NOT fix the durability defect; it only relocates where state is
held.

### UAT Scenarios (BDD)

#### Scenario: State is held through the store, behaviour unchanged
Given rule "pay-latency" is Inactive in the InMemoryRuleStateStore
When the evaluator processes an Active query result
Then the store holds Pending for "pay-latency"
And the emitted incidents are identical to the pre-refactor behaviour

#### Scenario: Per-rule state is isolated in the store
Given rules "pay-latency" and "disk-fill" both run through one store
When "pay-latency" transitions to Pending
Then "disk-fill"'s stored state is unchanged

#### Scenario: The pure transition is never modified
Given the RuleStateStore seam is in place
When the evaluator runs a cycle
Then transition() is called with the same signature and remains side-effect-free
And the existing pure-function property tests pass unchanged

### Acceptance Criteria

- [ ] `run_rule` no longer owns a `let mut state` local; state is read
      from and written to a `RuleStateStore`.
- [ ] `InMemoryRuleStateStore` keys state by rule name with per-rule
      isolation.
- [ ] `transition` signature and purity are unchanged; existing tests
      pass.
- [ ] Steady-state emissions are byte-for-byte identical to pre-refactor.

### Outcome KPIs

- **Who**: beacon-server orchestrator
- **Does what**: holds rule state through a store seam instead of a local
- **By how much**: 100% of `run_rule` state access goes through the trait (zero `let mut state` locals remain)
- **Measured by**: code review + the unchanged steady-state behaviour tests
- **Baseline**: 0% (state is a local variable today)

### Technical Notes

- `@infrastructure`: ships no operator-visible change. Justified
  because slice 02 (US-02, US-03) delivers the value end to end.
- Trait shape mirrors `LogStore` in `crates/lumen/src/store.rs`.
- Dependency: none. Must land before US-02.

---

## US-02: Firing alerts survive a restart without re-paging on-call (slice 02)

### Elevator Pitch

- **Before**: Priya restarts beacon-server during a live payment-latency
  incident; every firing rule resets to Inactive and re-fires, paging
  the whole on-call rotation a second time for an incident already
  being handled.
- **After**: Priya runs `beacon-server --rules /etc/beacon/rules
  --backend ...`; the startup log prints `recovered alert state
  rules_recovered=42 firing=3 pending=5`, the still-active "pay-latency"
  rule stays Firing silently, and nobody is re-paged.
- **Decision enabled**: Priya can restart beacon-server during a live
  incident (to deploy a fix or drain a node) without weighing it against
  the cost of a re-page storm.

### Problem

Priya Nair is an on-call operator who must sometimes restart
beacon-server while alerts are firing (to deploy a fix, drain a node, or
recover from a crash). Today every firing rule resets to Inactive on
restart, so it re-fires from scratch and re-pages the rotation. She
finds it stressful to restart during an incident and works around it by
delaying necessary restarts, which is itself an operational risk.

### Who

- On-call platform operator (Priya) | restarting beacon-server during a
  live or near-live incident | motivated to deploy fixes promptly
  without causing alert noise.

### Solution

Add a `FileBackedRuleStateStore` (WAL append on every transition,
`snapshot()` truncates the WAL, `open()` recovers snapshot then replays
WAL) and wire `open()` into beacon-server startup. Each per-rule loop
seeds from the recovered state. A rule recovered as Firing whose
condition is still active produces no new Firing emission, so no
re-page.

### Domain Examples

#### 1: Happy Path — Priya restarts during a live incident

"pay-latency" is Firing since 14:02. At 14:10 Priya restarts
beacon-server to deploy a config fix. On startup the store recovers
"pay-latency" as Firing since 14:02. The query is still Active. The
pure transition keeps it Firing and emits nothing. Priya is not paged.

#### 2: Edge Case — condition cleared while the process was down

"disk-fill" is Firing before restart. While beacon-server is down for
40 seconds, disk usage drops below threshold. On the first tick after
restart, the recovered state is Firing, the query is Inactive, and the
pure transition emits exactly one Resolved incident. Priya gets the
Resolved notification she would otherwise never have seen.

#### 3: Error/Boundary — store file corrupt on startup

The snapshot file is truncated by a full disk during a previous
shutdown. `open()` returns `PersistenceFailed { reason: ... }`.
beacon-server logs a clear operator-facing error stating what happened
and that it will not silently reset alert state, rather than starting
fresh and losing every firing alert.

### UAT Scenarios (BDD)

#### Scenario: A firing alert is not re-paged after a restart
Given rule "pay-latency" was Firing since 14:02 and was persisted to the durable store
And its condition is still active after the restart
When beacon-server restarts and runs its first evaluation cycle
Then no new Firing incident is emitted for "pay-latency"
And the operator is not re-paged

#### Scenario: A condition that cleared during downtime resolves exactly once
Given rule "disk-fill" was Firing and persisted before the restart
And its condition cleared while beacon-server was down
When beacon-server restarts and runs its first evaluation cycle
Then exactly one Resolved incident is emitted for "disk-fill"
And no Firing incident is emitted

#### Scenario: Corrupt durable state surfaces a clear error, never a silent reset
Given the durable state snapshot is corrupt on disk
When beacon-server attempts to recover state at startup
Then startup fails with a persistence error that names the cause
And alert state is not silently reset to Inactive

#### Scenario: A rule removed from config is not resurrected
Given the durable store holds Firing state for rule "legacy-check"
And "legacy-check" no longer exists in the rules directory
When beacon-server recovers state at startup
Then "legacy-check" state is dropped, not loaded
And the dropped rule name is logged

### Acceptance Criteria

- [ ] A Firing rule whose condition stays active emits no new Firing
      after restart (zero re-page).
- [ ] A Firing rule whose condition cleared during downtime emits
      exactly one Resolved on the first post-restart cycle.
- [ ] Corrupt or unreadable durable state produces a
      `PersistenceFailed` error and a clear log line, never a silent
      reset.
- [ ] State for a rule no longer in config is dropped and logged, not
      recovered.

### Outcome KPIs

- **Who**: on-call operator (via beacon-server)
- **Does what**: avoids re-paging for a still-firing alert after a restart
- **By how much**: 0 spurious re-fires, down from 1 per firing rule per restart
- **Measured by**: restart-survival test asserting no Firing emission for a recovered, still-active Firing rule
- **Baseline**: 1 re-page per firing rule per restart today

### Technical Notes

- `FileBackedRuleStateStore` mirrors `crates/lumen/src/file_backed.rs`:
  WAL path = base + `.wal`, snapshot path = base + `.snapshot`.
- `RuleState` likely needs `Serialize`/`Deserialize` derives — DESIGN
  wave decision (flagged risk in wave-decisions.md).
- Recovery latency budget: p95 <= 1.5 s on ubuntu-latest (see KPI 4).
- Dependency: US-01 (the trait seam) must land first.

---

## US-03: Pending dwell clocks are preserved across a restart (slice 02)

### Elevator Pitch

- **Before**: A rule that has been Pending for 90 of its required 120
  seconds loses its pending-since when Priya restarts; the dwell clock
  restarts from zero, so a legitimate alert is delayed by up to two more
  minutes.
- **After**: After the restart the startup log shows the rule recovered
  as `state=Pending since=...`; the dwell continues from the preserved
  instant and the rule fires on schedule, roughly 30 seconds later.
- **Decision enabled**: Priya can trust that restarting beacon-server
  does not blunt alert timing, so she does not avoid restarts for fear
  of silently delaying a brewing incident.

### Problem

When a rule's condition has held but not yet for its full
`for_duration`, beacon-server holds it as `Pending { since }`. On
restart the `since` instant is lost with the local variable, so the
`for_duration` dwell clock restarts from zero. A near-ready alert is
silently delayed, which is exactly when timing matters most. Priya has
no way to know an alert was delayed by her restart.

### Who

- On-call operator (Priya) | restarting beacon-server while rules are
  mid-dwell | motivated to keep alert timing accurate across restarts.

### Solution

Persist the `since` instant inside the durable `RuleState` (it is
already part of `Pending`/`Firing`). On recovery, the per-rule loop
seeds with the preserved `since`, and the unchanged pure `transition`
measures dwell from that instant. No change to `transition`; the fix is
purely that the durable store round-trips the `since` value faithfully.

### Domain Examples

#### 1: Happy Path — dwell continues across restart

"disk-fill" has been Pending since 14:00:00 with a 120 s
`for_duration`. At 14:01:30 (90 s in) Priya restarts. Recovery restores
Pending since 14:00:00. On the next tick at 14:02:00 the dwell is 120 s,
so the rule fires on schedule rather than 90 s late.

#### 2: Edge Case — condition cleared during a pending dwell

"pay-latency" is Pending since 14:05. While beacon-server is down the
condition clears. On the first post-restart tick the query is Inactive,
so the pure transition returns Inactive with no emission. No spurious
Pending-to-Firing fire occurs.

#### 3: Error/Boundary — pending-since is in the future after clock change

The recovered `since` is slightly after `now` due to a clock
adjustment. The existing dwell maths uses `now.duration_since(since)
.unwrap_or_default()`, yielding a zero dwell, so the rule simply waits
the full `for_duration` from the next observation. No panic, no negative
dwell.

### UAT Scenarios (BDD)

#### Scenario: Pending dwell clock is preserved across restart
Given rule "disk-fill" was Pending since 14:00:00 with a 120 second for_duration
And beacon-server restarts at 14:01:30
When the durable store recovers "disk-fill"
Then the recovered pending-since is 14:00:00
And the rule fires at 14:02:00, not at 14:03:30

#### Scenario: A pending condition that cleared during downtime does not fire
Given rule "pay-latency" was Pending and persisted before the restart
And its condition cleared while beacon-server was down
When beacon-server runs its first evaluation cycle
Then "pay-latency" returns to Inactive
And no Firing incident is emitted

#### Scenario: A recovered pending-since in the future does not panic
Given the recovered pending-since for a rule is slightly after the current time
When the evaluator measures dwell time
Then the dwell is treated as zero
And the rule waits the full for_duration from the next observation

### Acceptance Criteria

- [ ] Recovered `Pending { since }` carries the exact pre-restart
      `since` instant.
- [ ] Dwell is measured from the preserved instant; the rule fires on
      its original schedule, not a restarted clock.
- [ ] A pending condition that cleared during downtime returns to
      Inactive with no emission.
- [ ] A future-dated recovered `since` yields a zero dwell, no panic.

### Outcome KPIs

- **Who**: on-call operator (via beacon-server)
- **Does what**: keeps pending dwell timing accurate across a restart
- **By how much**: 100% of pending-since instants recovered correctly (0% today)
- **Measured by**: round-trip test asserting recovered Pending{since} == pre-restart value, and a fires-on-schedule test
- **Baseline**: 0% (pending-since is lost on every restart today)

### Technical Notes

- `since: SystemTime` must serialise and deserialise without precision
  loss adequate for dwell maths (sub-second is irrelevant against
  multi-second `for_duration`).
- Reuses the same `FileBackedRuleStateStore` from US-02; no separate
  storage path.
- Dependency: US-01 (seam) and US-02 (durable adapter) land first; US-03
  is the pending-specific behaviour on the same adapter.
