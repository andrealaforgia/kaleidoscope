# Journey — alert state survives a beacon-server restart

Persona: **Priya Nair**, on-call platform operator. She owns the
Kaleidoscope alerting stack. When she restarts beacon-server (deploy,
config change, node drain, crash recovery) she must not lose the
in-flight alert state, and on-call must not be re-paged for an incident
that is already being handled.

## Emotional arc

Problem Relief: **anxious -> watchful -> relieved**.

- Start (anxious): "If I restart beacon-server now, will the firing
  payment-latency alert get re-sent and wake the whole rotation again?"
- Middle (watchful): she restarts and reads the startup log lines that
  report how many rule states were recovered.
- End (relieved): the firing alert stays firing silently, the pending
  clock keeps counting from where it was, and nobody is re-paged.

## ASCII flow

```
[Trigger: operator restarts beacon-server]
        |
        v
+-------------------+      +------------------------+      +-----------------------+
| BEFORE restart    | ---> | RESTART (process down) | ---> | AFTER restart         |
| rule "pay-latency"|      | local `let mut state`  |      | state recovered from  |
| is Firing{since}  |      | would reset to Inactive|      | the durable store     |
| rule "disk-fill"  |      | (the gap)              |      | Firing{since} intact  |
| is Pending{since} |      |                        |      | Pending{since} intact |
+-------------------+      +------------------------+      +-----------------------+
   Feels: anxious             Feels: watchful                Feels: relieved
   Sees: alerts active        Sees: recovery log line        Sees: no re-page,
                                                              dwell clock preserved
```

## Step 1 — Operator restarts beacon-server (durable adapter wired)

Command (unchanged surface; the durability is configured by where
state lives, not a new flag in this feature):

```
beacon-server --rules /etc/beacon/rules --backend http://localhost:9090/api/v1
```

Startup log mockup (the only operator-visible touchpoint):

```
+-- beacon-server starting ---------------------------------------+
| INFO beacon-server starting rules_loaded=42 backend=...         |
| INFO recovered alert state  rules_recovered=42                  |
|        firing=3 pending=5 inactive=34                            |
| INFO rule "pay-latency" recovered state=Firing since=...        |
| INFO rule "disk-fill"   recovered state=Pending since=...       |
+-----------------------------------------------------------------+
```

- emotional_state.entry: anxious (will I cause a re-page storm?)
- emotional_state.exit: relieved (the counts confirm nothing was lost)

Shared artifacts on this step:

- `${rules_recovered}` — count of rule states loaded from the store.
  Source of truth: the durable state store on disk. Consumer: this
  startup log line. Must equal `${rules_loaded}` when every rule still
  exists in config.
- `${state}` + `${since}` per rule — the recovered `RuleState` and its
  `since` instant. Source of truth: the durable store. Consumer: the
  per-rule evaluator loop (seeds the loop instead of `Inactive`).

## Step 2 — First evaluation cycle after restart

The per-rule loop now seeds from the store, not from `Inactive`.

```
+-- first tick after restart -------------------------------------+
| rule "pay-latency": prev=Firing{since=T0}                        |
|   query still Active -> stays Firing, NO new Firing emission      |
|   (operator NOT re-paged)                                         |
| rule "disk-fill":    prev=Pending{since=T1}                       |
|   dwell measured from T1 (preserved), not from now                |
+-----------------------------------------------------------------+
```

- emotional_state.entry: watchful
- emotional_state.exit: relieved

Failure modes (for DISTILL error-scenario generation):

- Store file corrupt / unreadable on startup -> recovery fails with
  `PersistenceFailed`; operator must see a clear error and a decision
  (start fresh vs abort), not a silent reset to Inactive.
- A rule present in the store no longer exists in config -> its
  recovered state is dropped, not resurrected.
- A rule firing-resolves while the process is down (condition cleared
  during downtime) -> on first tick the store still holds Firing, query
  is Inactive, the pure transition emits Resolved exactly once. The
  operator gets the Resolved they would otherwise never have seen.

## Integration checkpoint

`transition` is never modified. The store sits before and after the
pure call: read previous state -> `transition(prev, outcome, rule, now)`
-> persist next state. The `${state}`/`${since}` round-trip through the
store must reproduce the exact value the orchestrator held in memory
before the restart (correctness guardrail KPI).
