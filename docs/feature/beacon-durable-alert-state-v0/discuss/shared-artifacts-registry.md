# Shared Artifacts Registry — beacon-durable-alert-state-v0

```yaml
shared_artifacts:
  rule_state:
    source_of_truth: "durable rule-state store on disk (FileBackedRuleStateStore base path)"
    consumers:
      - "per-rule evaluator loop seed (replaces `let mut state = RuleState::Inactive`)"
      - "startup recovery log line"
      - "pure transition() input (previous state)"
    owner: "beacon-server orchestrator (slice 02 wiring)"
    integration_risk: "HIGH — if the persisted state diverges from the in-memory value, a firing alert can re-page or a resolved alert can be missed."
    validation: "Round-trip property test: persist(state) then recover() yields the identical RuleState including the `since` instant."

  pending_since:
    source_of_truth: "durable rule-state store on disk (carried inside RuleState::Pending/Firing)"
    consumers:
      - "pure transition() dwell calculation (now.duration_since(since))"
    owner: "beacon-server orchestrator"
    integration_risk: "HIGH — losing the `since` instant restarts the for_duration dwell clock, delaying legitimate alerts."
    validation: "Recovered Pending{since}/Firing{since} equals the pre-restart instant; dwell maths uses the preserved value."

  rules_recovered:
    source_of_truth: "count of entries returned by RuleStateStore::recover()"
    consumers:
      - "startup INFO log line `recovered alert state rules_recovered=N`"
    owner: "beacon-server orchestrator"
    integration_risk: "MEDIUM — a mismatch between rules_recovered and rules_loaded signals dropped or stale state; operator visibility prevents silent data loss."
    validation: "rules_recovered == rules_loaded when every recovered rule still exists in config; lower count is logged with the dropped rule names."

  store_base_path:
    source_of_truth: "beacon-server configuration / startup wiring (DESIGN wave decides the exact path key)"
    consumers:
      - "FileBackedRuleStateStore::open(base_path)"
      - "WAL path = base_path + .wal, snapshot path = base_path + .snapshot (mirrors lumen file_backed)"
    owner: "beacon-server orchestrator"
    integration_risk: "MEDIUM — path mismatch between restarts means recovery reads an empty store (silent reset). Single source of truth required."
    validation: "WAL and snapshot path derivation mirrors crates/lumen/src/file_backed.rs helpers exactly."
```

## Consistency check

- Every `${variable}` in the journey TUI mockups
  (`${rules_recovered}`, `${state}`, `${since}`) has a documented
  source above.
- No two steps display the same datum from different sources: all
  rule-state data flows from the single durable store.
- No hardcoded values: the recovery counts and per-rule states are
  read from the store, never assumed.

## CLI UX note

This feature introduces NO new CLI surface (per constraint). The only
operator-visible touchpoint is the startup recovery log line, which
follows the existing `tracing` INFO conventions already in
`beacon-server/src/main.rs`.
