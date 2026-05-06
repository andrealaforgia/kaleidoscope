# ADR-0014 — `spark` flush-timeout mechanism

- **Status**: Accepted
- **Date**: 2026-05-06
- **Author**: `nw-solution-architect` (Morgan)
- **Feature**: `spark` v0
- **Supersedes**: none
- **Superseded by**: none

## Context

`SparkGuard::Drop` is the operationally load-bearing slice (Slice 06).
DISCUSS `wave-decisions.md > Q4` locks the contract:

- `Drop` flushes pending exports synchronously.
- Configurable timeout (`SparkConfig::with_flush_timeout`); default 5 s.
- Bounded — `Drop` never blocks indefinitely.
- Observable — clean flush emits `tracing::info!`; deadline expiry
  emits `tracing::warn!`; downed-downstream does not panic.

Slice 06 names two open DESIGN decisions:

1. **Sequential vs concurrent flush** across `TracerProvider`,
   `LoggerProvider`, `MeterProvider`. The OTel SDK's `force_flush` API
   is per-provider; calling it three times in sequence vs concurrently
   has different worst-case timing properties.
2. **Deadline division** across providers. Sequential = each provider
   sees `total_remaining_time`; concurrent = each provider sees the
   full `flush_timeout`. The total drop time is bounded by
   `flush_timeout_ms` either way.

Sentinel's review (`discuss/peer-review.md > Suggestions for Morgan §3`)
explicitly directs DESIGN: "Decide sequential-vs-concurrent
three-provider flush and per-provider deadline division. If OTel SDK
counters are unavailable, accept 'best-effort' drained/dropped counts
with a documented caveat."

## Decision

### 1. Sequential flush with a remaining-time budget

```text
Drop {
    1. Write tracing INFO "spark: shutdown initiated flush_timeout_ms=N".
    2. Compute deadline = Instant::now() + flush_timeout.
    3. For each provider in [tracer, logger, meter]:
       a. Compute remaining = deadline.saturating_duration_since(Instant::now()).
       b. If remaining is zero -> skip force_flush; record "deadline expired before this provider".
       c. Call provider.force_flush_with_timeout(remaining).
       d. Accumulate the per-provider outcome into a tally.
    4. Choose the WARN-vs-INFO event based on whether any provider hit
       the deadline OR whether any provider returned a non-Ok flush
       outcome.
    5. Write tracing INFO "spark: shutdown complete drained=K"
       OR
       Write tracing WARN "spark: flush deadline exceeded dropped=M flush_timeout_ms=N".
}
```

Sequential, with a **shared remaining-time budget** rather than
fixed-per-provider slices. The total drop time is bounded by
`flush_timeout_ms`; each provider sees as much of the budget as the
preceding ones did not consume.

### Why sequential

The OTel SDK's `force_flush` is a synchronous-from-the-caller API
that internally drives a batch processor's flush. Each provider's
batch processor is independent; running three flushes concurrently
would require either:

- **Tokio task spawning + `block_on`** — but Spark's `Drop` is
  synchronous from the application's seat (D5: Drop is the standard
  Rust resource-release idiom), and `Drop` running inside a Tokio
  runtime context that spawns and blocks on tasks risks deadlock if
  the runtime has only one worker thread.
- **OS thread spawning** — three `std::thread::spawn` plus
  `JoinHandle::join_with_timeout` would work, but introduces thread
  creation overhead at exit time and a third-party `JoinHandle`-with-
  timeout library (the standard library's `JoinHandle::join` has no
  timeout).

The sequential-with-budget shape avoids both complications. The cost
is correlated worst-case latency: if the tracer's flush exhausts most
of the budget, the logger and meter get little. In practice this is
the right behaviour — the three flushes share the SAME upstream OTLP
exporter and the SAME network endpoint, so a slow downstream affects
all three equally; serialising the calls means the network does one
thing at a time, which is friendlier to a backed-up downstream than
three concurrent calls competing for the same connection pool.

### Why a shared budget over per-provider slices

A fixed `flush_timeout_ms / 3` per provider would mean:

- A clean tracer flush that completes in 5ms wastes the leftover 1.66s
  of its slice; the meter (which might need more time) cannot reuse it.
- A WARN event would be possible even when the *total* time was
  well under the deadline.

The shared budget shape — `remaining = deadline - now()` per provider
— gives slow providers the headroom that fast ones did not need, while
keeping the *total* drop time bounded.

### 2. Drained-vs-dropped count derivation

The OTel SDK's `force_flush_with_timeout` returns `OTelSdkResult` (a
`Result<(), OTelSdkError>` alias in the 0.27 API); it does NOT return
a count of drained or dropped records. The internal counters of
`BatchSpanProcessor`, `BatchLogProcessor`, and `PeriodicReader` are
not exposed publicly at 0.27.

The decision: **best-effort known counts, with a documented caveat.**

- **Drained count (clean flush, INFO event)**: Spark's `Drop` reports
  `drained=N` where N is the OTel SDK's reported "items submitted to
  exporter and exporter returned Ok" count if the SDK exposes it, or
  the literal string `drained=unknown` otherwise. At v0.27 the count
  is **not exposed**, so the v0 INFO event reads `spark: shutdown
  complete drained=unknown` for the OTel SDK's current API surface.
  The vocabulary registry (`shared-artifacts-registry.md >
  spark_log_event_vocabulary`) accepts this caveat — the event
  emission is the contract; the count is informational.
- **Dropped count (deadline expiry, WARN event)**: Spark reports
  `dropped=unknown` in the WARN event for the same reason — the SDK
  does not expose the in-flight buffer count when force_flush returns
  on deadline. The WARN event still names the **deadline** explicitly
  (`flush_timeout_ms=N`) so the operator can correlate the WARN with
  the configured policy.

The exact log messages:

```text
INFO  target=spark message="shutdown initiated"  flush_timeout_ms=5000
INFO  target=spark message="shutdown complete"   drained=unknown
WARN  target=spark message="flush deadline exceeded" dropped=unknown flush_timeout_ms=500
```

If a future OTel SDK release exposes drained/dropped counters, Spark
v0.x can switch from `=unknown` to the exposed integer without
breaking the vocabulary contract. The journey-spark-visual.md mockups
say `drained=N` and `dropped=M` as illustrative; ADR-0014 reads them
as "literally `=N` where N is what the SDK reports, or `=unknown` if
it does not".

The tracing event field SHOULD be a structured `drained: u64` (or
`drained: &str = "unknown"`) on the `tracing::info!` invocation, so
that subscribers that consume structured fields (e.g. JSON-formatted
to stderr) preserve the typing distinction. The exact macro
invocation pattern is the crafter's call.

### 3. Panic safety in Drop

Spark's `Drop` MUST NOT panic. The OTel SDK's `force_flush` calls
**may** panic in the wild (a contract violation by the SDK, not
Spark's job to defend). The mitigation:

- **No `unwrap()` / `expect()` / panic-on-`Err`** in `Drop`. Every
  fallible call is matched on `Result` and converted into either an
  INFO accumulator (clean) or a WARN accumulator (deadline / error).
- **No `tracing` event emission failure handling** beyond what
  `tracing` itself swallows. `tracing::info!` and `tracing::warn!`
  are infallible by design (they fail silently if no subscriber is
  attached, which is the correct behaviour for a library).
- **The down-downstream test (Slice 06 Case C)** is the behavioural
  proof: an Aperture forcibly killed mid-test does not cause a panic
  in `Drop`; the integration test asserts the absence of panic via
  a normal test exit.

If the OTel SDK itself panics inside `force_flush` (e.g. an internal
`unwrap()` violation in 0.27), the panic propagates as a panic-during-
drop. Spark's `Drop` does NOT use `std::panic::catch_unwind` —
catching a panic in `Drop` is the kind of obscuring-rather-than-fixing
that DISCUSS `wave-decisions.md > Q4 rationale` deprecates ("idiomatic
Rust posture: panics in Drop are the developer's responsibility").

### 4. Drop is idempotent

A second drop on the same `SparkGuard` is a no-op. Mechanism: the
guard's internal state is held in `Option<Inner>`; the first drop
takes the `Some(inner)` and runs the flush; subsequent drops see
`None` and return immediately.

Equivalently, `drop(guard)` called explicitly produces the same
observable behaviour as letting the guard go out of scope (per the
journey-spark UAT: "drop(guard) called explicitly is equivalent to
scope-exit drop"). The mechanism is the same — `Drop::drop` runs
exactly once on the value.

### 5. Sub-second precision

`flush_timeout` is `std::time::Duration` (per ADR-0011 §"SparkConfig
API shape"). Drop's deadline arithmetic uses `std::time::Instant` for
monotonic clock semantics. Sub-second timeouts (e.g. `Duration::from_millis(500)`
in Slice 06 Case B) work without rounding.

## Alternatives Considered

### Option A — Sequential flush with shared remaining-time budget (RECOMMENDED, accepted)

Detailed above.

**Pros**:
- No async runtime or thread spawning at exit time.
- The total drop time is bounded by `flush_timeout` regardless of
  per-provider behaviour.
- A fast provider's leftover budget benefits a slow provider.
- Compatible with applications running in any (or no) async runtime.

**Cons**:
- Worst-case latency = sum of three flushes if all are slow. Bounded
  by the total deadline, but a slow tracer flush starves the meter
  flush of budget.

### Option B — Concurrent flush via `std::thread::spawn` + timed join

```rust
// illustrative; rejected
let h1 = std::thread::spawn(|| tracer.force_flush_with_timeout(t));
let h2 = std::thread::spawn(|| logger.force_flush_with_timeout(t));
let h3 = std::thread::spawn(|| meter.force_flush_with_timeout(t));
// join with overall deadline...
```

**Pros**:
- All three flushes get the full `flush_timeout` independently.
- Total wall-clock drop time = `max(t1, t2, t3)`, not `sum(t1, t2, t3)`.

**Cons**:
- Three OS threads created on every shutdown — non-trivial overhead
  at process exit.
- `std::thread::JoinHandle::join` has no timeout; would need
  `crossbeam-channel` or `parking_lot::Condvar` plumbing or a
  `cargo-mutex` library for the timed join. New dependency,
  non-trivial code.
- If the OTel SDK's batch processor is itself running on the same
  Tokio runtime as the application, three blocking threads competing
  for a `block_in_place` permit can deadlock the runtime. Subtle.
- All three flushes hit the SAME OTLP exporter / SAME tonic channel /
  SAME network endpoint; serialisation reduces network contention,
  not increases it. Concurrent flushes might have **worse** wall-clock
  performance than sequential against a backed-up downstream.

**Rejected**. The concurrency advantage is theoretical for the typical
case (one slow downstream affects all three); the engineering cost is
real.

### Option C — Concurrent flush via Tokio tasks

```rust
// illustrative; rejected
let (t1, t2, t3) = tokio::join!(
    tracer.force_flush_async(),
    logger.force_flush_async(),
    meter.force_flush_async(),
);
```

**Pros**:
- Idiomatic for OTel applications already running on Tokio.

**Cons**:
- Spark's `Drop` is synchronous; spawning Tokio tasks from `Drop`
  requires `tokio::runtime::Handle::current().block_on(...)` which
  panics if no runtime is present, OR `Handle::try_current()` + a
  fallback to sequential, which is the worst of both worlds.
- The OTel SDK's `force_flush` is the synchronous public API at 0.27;
  `force_flush_async` does not exist. The async-wrap would be a Spark-
  internal layer.
- Couples Spark to Tokio (constraint conflict: Spark does NOT own the
  runtime per ADR-0011 and the technology-choices.md "Rejected runtime
  alternatives" entry).

**Rejected** for the runtime-coupling and the absent async API.

### Option D — Per-provider fixed deadline slice

```rust
let per_provider = flush_timeout / 3;
tracer.force_flush_with_timeout(per_provider);
logger.force_flush_with_timeout(per_provider);
meter.force_flush_with_timeout(per_provider);
```

**Pros**:
- Each provider gets a guaranteed minimum.
- Deterministic worst-case timing per provider.

**Cons**:
- Wastes leftover budget when one provider finishes fast. The total
  drop time is still bounded by `flush_timeout`, but the WARN event
  fires unnecessarily often.
- Cognitive load: explaining "your 5s timeout is actually three
  ~1.67s timeouts" to operators is friction.

**Rejected**. The shared-budget shape (Option A) is the right answer
for the same total-deadline contract.

## Consequences

### Positive

- Drop completes within `flush_timeout_ms` for all paths (clean,
  deadline, panic, downed-downstream). The bound is structural: the
  remaining-time arithmetic is the gate.
- No new dependencies (no thread library, no mutex library, no async
  runtime).
- Sequential calls play well with backed-up downstreams: the OTLP
  exporter sees one in-flight request at a time, not three competing
  for the same connection.
- The `unknown` count caveat is documented and observable: an
  operator reading the WARN event sees `dropped=unknown
  flush_timeout_ms=500` and knows to investigate why the flush did
  not complete, rather than parsing a count that the SDK does not
  reliably produce.

### Negative

- A pathologically-slow tracer flush starves the logger and meter of
  their share of the budget. The total time is still bounded; the
  WARN event reports the *aggregate* outcome ("flush deadline
  exceeded"), not per-provider.
- The `drained=unknown` / `dropped=unknown` markers in the v0 events
  are a known limitation. Future SDK releases may expose the counts;
  Spark v0.x can switch verbatim if so.

### Trade-off ATAM

This decision is a sensitivity point for **Performance Efficiency —
Time Behaviour** (positive: bounded drop time) and for **Reliability
— Recoverability** (positive: no infinite-block exit path).

It is a trade-off point against **Functional Suitability — Completeness**
(negative: `drained=unknown` / `dropped=unknown` is less informative
than an integer count). Accepted because the SDK's API surface at v0
does not expose the count, and the behavioural-flush guarantee is
more load-bearing than the count value.

## Self-Application of Earned Trust (principle 12)

The bounded-flush invariant is enforced by:

1. **Subtype check (compile-time)** — `flush_timeout: Duration` is
   typed; the deadline arithmetic uses `Instant::saturating_duration_since`
   which cannot panic. Negative durations are impossible at the type
   level.
2. **Structural check (CI)** — Slice 06 Case A asserts `drop completes
   within the configured flush_timeout_ms` via a wall-clock measurement
   with a small tolerance. Slice 06 Case B asserts the WARN-event path
   completes within ~500 ms (no indefinite block). Slice 06 Case C
   asserts the down-downstream path does not panic.
3. **Behavioural check (CI)** — The `invariant_no_telemetry_on_telemetry`
   test (cross-cutting) asserts that the WARN event itself goes to the
   tracing facade, NOT to the OTel pipeline. This catches a
   regression where a future change tries to "report dropped count via
   OTel metrics" and accidentally violates D5.

The down-downstream no-panic invariant is enforced by:

1. **Subtype check** — Drop's body has no `unwrap()` / `expect()` on
   fallible calls (a code-review rule, enforceable via `clippy::unwrap_used`
   inside `guard.rs` if desired).
2. **Structural check** — Slice 06 Case C drives the integration test
   against a forcibly-killed Aperture port and asserts the test
   process exits zero.
3. **Behavioural check** — `cargo mutants` (Gate 5, ADR-0011) mutates
   `?` operators and `match` arms in `guard.rs`; any mutation that
   introduces a `panic!` path must be killed by Slice 06 Case C.

The probe contract for the flush mechanism is the integration-test
trio (Cases A/B/C). Each case answers a different question: "does the
clean path work?", "does the deadline path work?", "does the failure
path stay safe?". A single-layer bypass is caught by at least one of
the other two.
