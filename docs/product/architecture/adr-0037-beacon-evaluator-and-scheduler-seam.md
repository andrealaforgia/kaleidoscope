# ADR-0037 — Beacon evaluator and Scheduler seam

**Status**: Accepted
**Date**: 2026-05-11
**Author**: Bea (autonomous DESIGN dispatch)

## Context

The Beacon evaluator must:

1. Tick each rule at its configured `interval` (default 30 s)
2. Issue PromQL fetches concurrently across rules (don't serialise)
3. Process responses serially (so the per-rule state machine has a
   linear event order)
4. Be testable without a real clock and without a real HTTP backend

The pattern is established by Prism's reducer + Scheduler seam
(ADR-0029). Beacon adapts the same shape.

## Decision

Three layers, sharply separated:

### Layer 1 — pure evaluator

```rust
pub async fn evaluate<F, Fut>(
    rules: &RuleSet,
    fetch: F,
    now: SystemTime,
    prev_state: &mut HashMap<RuleId, RuleState>,
) -> EvaluationResult
where
    F: Fn(&Rule) -> Fut,
    Fut: Future<Output = Result<PromResponse, FetchError>>,
{ ... }
```

`evaluate` is the load-bearing pure function. Inputs: the rule set,
a fetch function (the IO seam), the current time, the previous
state. Output: the evaluation result (fired, resolved, inhibited
incidents).

Property-tested with fake `fetch` closures that return deterministic
responses. Unit-tested with `prev_state` mutations.

### Layer 2 — Scheduler trait (test seam)

```rust
pub trait Scheduler: Send + Sync {
    fn now(&self) -> SystemTime;
    async fn sleep_until(&self, deadline: SystemTime);
}

pub struct RealScheduler;

impl Scheduler for RealScheduler {
    fn now(&self) -> SystemTime { SystemTime::now() }
    async fn sleep_until(&self, deadline: SystemTime) {
        if let Ok(d) = deadline.duration_since(self.now()) {
            tokio::time::sleep(d).await;
        }
    }
}
```

The binary uses `RealScheduler`. Tests use `FakeScheduler` that
captures `sleep_until` deadlines and advances a virtual clock on
demand. Same shape as Prism's `Scheduler` interface.

### Layer 3 — orchestrator binary

The `beacon-server` binary owns:

- The `tokio` runtime
- The `reqwest` HTTP client (wrapped in a `fetch` closure that
  honours `AbortController`-equivalent cancellation via Tokio's
  `tokio::time::timeout`)
- The `SIGHUP` signal handler that triggers rule-set reload
- The OTLP telemetry exporter (env-gated)
- The graceful shutdown sequence: drain the in-flight sink
  emissions, then exit

The orchestrator loop in pseudocode:

```rust
loop {
    let now = scheduler.now();
    let due_rules = rules.due_at(now);
    let fetched = futures::stream::iter(due_rules)
        .map(|r| (r, http_client.fetch(r)))
        .buffer_unordered(MAX_CONCURRENT_FETCHES)
        .collect::<Vec<_>>()
        .await;
    let result = beacon::evaluate(&rules, &fetched_map, now, &mut state).await;
    for incident in result.fired {
        emit_to_sinks(&incident).await;
    }
    let next_tick = rules.next_due_after(now);
    scheduler.sleep_until(next_tick).await;
}
```

## State management

Per-rule state lives in a `HashMap<RuleId, RuleState>` held by the
orchestrator. The state machine transitions are pure functions in
`beacon::state_machine`:

```rust
pub enum RuleState {
    Inactive,
    Pending { since: SystemTime },
    Firing { since: SystemTime },
    Resolved { at: SystemTime, prev_firing_since: SystemTime },
}

pub fn transition(
    state: &RuleState,
    query_result: &QueryResult,
    rule: &Rule,
    now: SystemTime,
) -> RuleState { ... }
```

This is the same shape as Prism's auto-refresh reducer (ADR-0029):
pure transitions, tested as a pure function.

## Concurrency model

`MAX_CONCURRENT_FETCHES` (default 16) caps simultaneous outbound
HTTP calls. Beyond that, fetches queue. The cap is operator-tunable
via `--max-concurrent-fetches`.

Sink emissions are spawned as fire-and-forget Tokio tasks with a
bounded channel back to the orchestrator for retry tracking. A
slow sink does NOT block the evaluator's next tick.

## Consequences

- The evaluator is pure and property-testable
- The Scheduler seam allows deterministic clock-driven tests
- The orchestrator is small and testable independently with mocks
- The shape mirrors Prism's reducer + Scheduler seam, reducing
  cognitive load for cross-feature readers
