# ADR-0020 — `sieve` summary aggregator and timer task

- **Status**: Accepted
- **Date**: 2026-05-06
- **Author**: `@nw-solution-architect` (Morgan)
- **Feature**: `sieve` v0
- **Supersedes**: none
- **Superseded by**: none

## Context

DISCUSS Q8 locks the observability contract: per-decision DEBUG events
plus a periodic INFO summary every 60 seconds carrying `kept`,
`dropped`, and `rate` fields. Sentinel's APPROVED peer review's
Finding 2 closes the tick-interval ambiguity: 60 seconds in
production, parameterisable down for tests. Slice 06's brief mentions
a `tokio` timer task and "Mutex<Counters>", "RwLock<Counters>", or
"atomic counters per outcome" as candidates.

DESIGN must close two questions:

- **D4**: synchronisation primitive for the aggregator. Three
  candidates: `Mutex<Counters>`, `RwLock<Counters>`, atomic counters.
- **D5**: timer-task ownership. Owned by Sieve, or driven by Aperture's
  runtime?

The hot path is every call to `SamplingSink::accept(SinkRecord::Traces(...))`.
Aperture is a high-throughput pipeline component (the harness's KPIs
target tens of thousands of spans per second on a single core). Lock
contention on the aggregator would visibly slow Sieve.

## Decision

### 1. Counter type — three `AtomicU64`s

```rust
// crates/sieve/src/aggregator.rs
pub(crate) struct Counters {
    kept_total: AtomicU64,
    kept_error_bearing: AtomicU64,
    dropped: AtomicU64,
}

impl Counters {
    pub(crate) fn record_kept_error_bearing(&self) {
        self.kept_total.fetch_add(1, Ordering::Relaxed);
        self.kept_error_bearing.fetch_add(1, Ordering::Relaxed);
    }
    pub(crate) fn record_kept_sampled(&self) {
        self.kept_total.fetch_add(1, Ordering::Relaxed);
    }
    pub(crate) fn record_dropped(&self) {
        self.dropped.fetch_add(1, Ordering::Relaxed);
    }
    /// Snapshot and reset. Returns (kept_total, kept_error_bearing,
    /// dropped) at the time of the call. Safe to call concurrently
    /// with the recorder methods; the only ordering guarantee is that
    /// each counter's swap is atomic. A concurrent record between the
    /// three swaps lands in the next window — acceptable for a 60s
    /// window because the operator's ask is "approximate aggregate",
    /// not "exact partition between windows".
    pub(crate) fn snapshot_and_reset(&self) -> (u64, u64, u64) {
        let kept = self.kept_total.swap(0, Ordering::Relaxed);
        let kept_err = self.kept_error_bearing.swap(0, Ordering::Relaxed);
        let dropped = self.dropped.swap(0, Ordering::Relaxed);
        (kept, kept_err, dropped)
    }
}
```

(Pseudocode; software-crafter writes the production implementation.
The shape locks the contract: three counters, `Relaxed` ordering on
the hot path, `swap` for the snapshot-and-reset pattern.)

`AtomicU64` on the hot path means **no lock contention**. Every
`fetch_add` is a single CPU instruction on x86_64 and aarch64 (`lock
add` and `ldaddal` respectively). Every aggregation update is wait-free.

The three counters are independent because the relationships are
derivable: `kept_sampled = kept_total - kept_error_bearing`. Slice 06's
brief explicitly names "`kept`, `dropped`, `error_bearing`, `rate`" as
the INFO event fields; the rendered output is therefore:

```
kept_total = K, kept_error_bearing = E, kept_sampled = K - E,
dropped = D, rate = R
```

The implementation can choose either to expose `kept_sampled` directly
in the INFO event or to derive it. The DISCUSS slice-06 brief shows the
rendered format: "kept N traces (E error-bearing, S sampled at 0.10
rate), dropped M traces over the last 60s". The decorator computes
`S = K - E` at render time.

### 2. Timer task — owned by Sieve

```rust
// crates/sieve/src/aggregator.rs
pub(crate) struct SummaryTask {
    counters: Arc<Counters>,
    rate: f64,
    cancel: CancellationToken,
    interval_ms: u64,  // from SIEVE_SUMMARY_TICK_MS, default 60_000
    join: Option<JoinHandle<()>>,
}

impl SummaryTask {
    pub(crate) fn spawn(counters: Arc<Counters>, rate: f64, interval_ms: u64) -> Self {
        let cancel = CancellationToken::new();
        let join_cancel = cancel.clone();
        let counters_for_task = counters.clone();
        let join = tokio::spawn(async move {
            let mut ticker = tokio::time::interval(Duration::from_millis(interval_ms));
            ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
            // First tick fires immediately by default; consume it.
            let _ = ticker.tick().await;
            loop {
                tokio::select! {
                    _ = join_cancel.cancelled() => break,
                    _ = ticker.tick() => emit_summary(&counters_for_task, rate),
                }
            }
            // Final flush so the operator sees accounting up to shutdown.
            emit_summary(&counters_for_task, rate);
        });
        Self { counters, rate, cancel, interval_ms, join: Some(join) }
    }

    pub(crate) async fn shutdown(&mut self) {
        self.cancel.cancel();
        if let Some(join) = self.join.take() {
            let _ = join.await;
        }
    }
}
```

The task is owned by Sieve, not by Aperture. The reasons:

1. **Lifecycle alignment**: the task observes `Counters` that live
   inside `SamplingSink`. The natural owner of a `Counters`-observing
   task is the type that owns the `Counters`.
2. **Encapsulation**: Aperture's runtime should not know about Sieve's
   timer task. ADR-0007's minimal-`OtlpSink`-contract posture extends
   here: the integration boundary is `OtlpSink + Probe`, not
   "Aperture's runtime drives Sieve's tasks".
3. **Test seam**: the `__test_summary_tick_now` doc-hidden function
   (per ADR-0018) fires the snapshot path synchronously without
   waiting for the timer. This works because Sieve owns the timer; if
   Aperture owned it, the test would have to drive Aperture's runtime
   to fire Sieve's tick, which is awkward.

The task spawns on the **ambient Tokio runtime** at `SamplingSink::new`.
Aperture's binary always runs on a Tokio runtime (per ADR-0006), so
the runtime is always available at `SamplingSink::new` time. The
slice tests use `#[tokio::test]` to provide the runtime.

### 3. Cancellation and lifecycle

`SamplingSink` holds a `SummaryTask`; on `Drop` of `SamplingSink`, the
`SummaryTask::shutdown` future cannot run synchronously (Drop is sync,
shutdown is async). Two options:

- **Option A — sync cancel + abandon join**: Drop calls
  `self.cancel.cancel()` synchronously and abandons the JoinHandle.
  The task completes its current iteration (which may include a
  final summary emission) and exits.
- **Option B — explicit `shutdown()` method on SamplingSink that the
  composition root calls**: the composition root calls
  `sampling_sink.shutdown().await` before dropping the sink. Drop is
  a fail-safe.

The chosen path is **Option A** (sync cancel + best-effort join via
the same pattern Aperture's `Handle::Drop` uses for listener
shutdown — see `crates/aperture/src/lib.rs` lines 170-190). The
`CancellationToken::cancel` call is sync (it just toggles an atomic);
the JoinHandle is dropped by the Drop body, abandoning the future.
The task observes the cancel and exits at the next `select!` await
point. The final-flush summary in the task body fires inside the
loop's exit branch, so even an abandoned-join scenario emits the
final summary (it just runs in the background after Drop returns).

This matches Aperture's `Handle::Drop` pattern: sync signal,
best-effort cleanup, no awaiting from sync context.

The "async shutdown" path is also exposed as a `pub(crate) async fn
shutdown_async(&mut self)` method on `SamplingSink` for the test path
that wants deterministic completion — it cancels and awaits. The
public API does NOT expose this method at v0; if Aperture's
composition root needs it (it does not at v0; the listener-shutdown
path Aperture already runs is the operator-facing graceful shutdown,
and Sieve's own shutdown rides on top), it is a non-breaking addition
in a future minor.

### 4. Tick interval

The tick interval is read at `SamplingSink::new` time from the env
var:

```
SIEVE_SUMMARY_TICK_MS=<positive integer>
```

- Unset: defaults to `60_000` (60 seconds, per Q8).
- Set to a positive integer: the integer is the tick interval in
  milliseconds.
- Set to a non-numeric or zero value: `SieveConfigError::SummaryTickUnparseable`
  (zero is rejected because `tokio::time::interval(Duration::ZERO)`
  panics; the parse function rejects zero before the constructor
  reaches `interval`).

The integration tests set `SIEVE_SUMMARY_TICK_MS=100` (or smaller, via
`tokio::time::pause` + `advance` in the test-util feature) so the
slice-06 assertion fires within a test wall-clock budget. The
`SIEVE_SUMMARY_TICK_MS` env var is **not** part of the consumer-facing
contract — it exists for test infrastructure and operational override.
The Sieve docs document it as such.

### 5. Tracing emission — the INFO summary

```rust
// crates/sieve/src/observability.rs
pub(crate) fn emit_summary(counters: &Counters, rate: f64) {
    let (kept, kept_err, dropped) = counters.snapshot_and_reset();
    let kept_sampled = kept.saturating_sub(kept_err);
    tracing::info!(
        target: "sieve",
        kept = kept,
        kept_error_bearing = kept_err,
        kept_sampled = kept_sampled,
        dropped = dropped,
        rate = rate,
        "sieve: kept {kept} traces ({kept_err} error-bearing, {kept_sampled} sampled at {rate:.2} rate), dropped {dropped} traces over the last summary window"
    );
}
```

The structured fields are: `kept`, `kept_error_bearing`,
`kept_sampled`, `dropped`, `rate`. The rendered message follows the
exact wording slice-06's brief documents (with the brief's "60s"
softened to "summary window" because the production interval may be
overridden via the env var; the field set carries the precise data).

The DEBUG per-decision events are emitted by the decorator
(ADR-0021), not the aggregator. The aggregator's only emission point
is the periodic summary. This split keeps each module's responsibility
crisp.

### 6. Test seam

```rust
#[doc(hidden)]
pub fn __test_summary_tick_now<S, N>(sink: &SamplingSink<S, N>)
where
    S: aperture::ports::OtlpSink + aperture::ports::Probe,
    N: Sampler;
```

The function reaches into `SamplingSink`'s aggregator and synchronously
calls `emit_summary(&counters, rate)`. This bypasses the timer
entirely; the slice-06 test asserts the INFO event fires once and
carries the right field set without waiting for a tick.

The `__` prefix + `#[doc(hidden)]` convention follows Spark
ADR-0011's `__reset_for_testing` precedent. `cargo public-api` records
the seam on the manifest; the convention signals "stable across
versions, but explicitly not part of the consumer-facing contract".

## Alternatives Considered

### Option A — Three `AtomicU64`s + Sieve-owned timer task with `CancellationToken` (RECOMMENDED, accepted)

Detailed above.

**Pros**:
- Zero lock contention on the hot path (`fetch_add` is wait-free on
  every CPU Sieve targets).
- Snapshot-and-reset via `swap` is atomic per-counter; the small
  cross-counter race (a record landing between two swaps) is
  semantically benign for an "aggregate over the last window"
  contract.
- `CancellationToken` is the canonical `tokio-util` cooperative-
  cancellation primitive; the pattern is well-trodden in the Tokio
  ecosystem.
- The timer task's `MissedTickBehavior::Delay` posture means the
  task does not "catch up" on missed ticks if the runtime was
  suspended (which the slice-06 test path exercises via
  `tokio::time::pause`).
- The Drop body matches Aperture's `Handle::Drop` precedent (sync
  cancel, best-effort join).

**Cons**:
- The cross-counter race means the partition between two consecutive
  windows can be off-by-one or off-by-a-handful at a high record
  rate. Sieve's KPIs do not require exact partition; the per-window
  approximation is acceptable. Documented in the `snapshot_and_reset`
  comment.

### Option B — `Mutex<Counters>` (one lock for all three counters)

```rust
pub(crate) struct Counters { kept_total: u64, kept_error_bearing: u64, dropped: u64 }
pub(crate) type AggregatorState = Mutex<Counters>;
```

**Pros**:
- Atomic snapshot: the `Mutex::lock` guard sees all three counters at
  once.
- Simpler reasoning about cross-counter consistency.

**Cons**:
- Lock contention on every record. At 10000 traces/second the lock
  becomes a hot point. The OS-level mutex (parking_lot or std) is
  fast but not free.
- The `Mutex` poisoning surface adds a panic-handling concern
  (`expect("aggregator mutex poisoned")` everywhere or a custom
  recovery). `AtomicU64` has no such surface.

**Rejected** for the unnecessary contention. The cross-counter
consistency is not a load-bearing property at v0 — the operator's
ask is "approximate aggregate over the window".

### Option C — `RwLock<Counters>`

**Pros**:
- Multiple readers (the timer task) can read while writers (the
  decorator) write.

**Cons**:
- Recording is a write, not a read. Every record needs the write
  guard. The read benefit is illusory: only one task reads (the
  timer task), and it does so once per window. `RwLock` is strictly
  worse than `Mutex` here.

**Rejected** for missing the access pattern.

### Option D — `Arc<DashMap<&'static str, AtomicU64>>` or similar concurrent map

**Pros**:
- Generic over arbitrary counter names; future counters slot in
  without code changes.

**Cons**:
- A concurrent hashmap for three statically-known counters is
  over-engineered. The keys are not needed at runtime; the type
  system gives them for free.
- Adds a `dashmap` runtime dependency (MIT, fine licence-wise but
  unnecessary).
- The hot-path lookup is a hash + bucket walk, slower than the
  direct `AtomicU64` field access.

**Rejected** for over-engineering.

### Option E — Aperture-owned timer task

**Pros**:
- One Tokio task per Aperture instance, regardless of how many sinks
  are wired in.
- Aperture's `Handle::shutdown` orchestrator could drive the final
  flush.

**Cons**:
- Couples Aperture's runtime to Sieve's internal accounting. Future
  sinks (Sluice, Pulse) would each need to register their summary
  emitters with Aperture's task, growing Aperture's surface for
  every consumer.
- Violates ADR-0007's minimal-`OtlpSink`-contract posture: Aperture's
  job is to validate, decode, and hand off; it should not own the
  observability lifecycle of every component downstream.
- The test seam becomes harder: firing Sieve's tick from a test that
  does not own Aperture is awkward; spinning up Aperture just to
  drive Sieve's timer adds test-fixture complexity.

**Rejected** for the boundary violation and the test ergonomics.

### Option F — Flush-on-batch instead of timer task

The aggregator would emit a summary every N batches rather than every
T seconds.

**Pros**:
- No timer task needed; one less Tokio handle to manage.
- The summary fires only when there is work, so a quiet Sieve does
  not pollute the log with empty summaries.

**Cons**:
- Visibility is coupled to traffic volume. A low-traffic Sieve may
  not emit a summary for hours; the operator on default verbosity
  has no signal.
- DISCUSS Q8 explicitly rejects "or on flush" wording (Sentinel's
  Finding 1 closed this; the contract is "every 60 seconds", full
  stop).

**Rejected** as it contradicts DISCUSS Q8.

## Consequences

### Positive

- Hot path is wait-free. Sieve introduces zero lock contention into
  Aperture's pipeline.
- Timer ownership is encapsulated. Aperture's runtime drives Sieve's
  task by virtue of being the ambient runtime; Aperture's code does
  not know about the task.
- The test seam fires the snapshot synchronously, decoupling the
  slice-06 assertion from wall-clock time.
- The cancellation pattern matches Aperture's existing `Handle::Drop`
  precedent.

### Negative

- The cross-counter race in `snapshot_and_reset` means the partition
  between consecutive windows can drift by a small number of
  records. The `Counters` impl comment documents this; the
  operator-facing contract ("approximate aggregate") absorbs the
  variance.
- The Drop body abandons the JoinHandle. The final-flush summary may
  emit "after" the Drop returns; an integration test that asserts
  "the final summary appears" must use `shutdown_async` rather than
  Drop. Slice-06's brief does not require asserting the post-Drop
  summary; the per-window summary suffices.

### Trade-off ATAM

This decision is a **sensitivity point** for **Performance Efficiency
— Resource Utilisation**: the wait-free hot path is the load-bearing
engineering choice for Sieve's throughput story. A lock-based
alternative would visibly slow Aperture's pipeline at high trace
rates.

It is a **sensitivity point** for **Reliability — Fault Tolerance**:
the cooperative-cancellation pattern means a runaway timer task
cannot block Aperture's shutdown indefinitely; the abandon-join
fallback caps the cleanup cost at "one in-flight summary emission".

It is a **trade-off point** between **Reliability — Recoverability**
(positive: in-process counters survive transient errors; nothing is
persisted, nothing to restore) and **Functional Suitability —
Completeness** (negative: counters are not preserved across process
restarts; an operator restarting Aperture loses the in-flight
window's accounting). The trade is accepted because v0's
observability contract is "approximate aggregate over the window";
restart-time durability would require a sub-system that v0 does not
need.

## Self-Application of Earned Trust (principle 12)

The aggregator's correctness contract is enforced by three mechanisms:

1. **Subtype check (compile-time)** — the `Counters` struct's three
   `AtomicU64` fields and the `record_kept_*` / `snapshot_and_reset`
   method signatures are exposed at the `pub(crate)` level. The
   compiler enforces that no consumer outside the crate can construct
   a `Counters` directly; the decorator and the timer task are the
   only call sites.
2. **Structural check (CI)** — `cargo mutants --package sieve
   --in-diff` (Gate 5) walks every line of the aggregator. Any
   mutation that swaps the `Ordering` (Relaxed → Acquire/Release),
   replaces `fetch_add` with a no-op, or replaces `swap` with `load`
   is caught by a slice test. The mutation kill rate target is 100%
   per ADR-0005 Gate 5.
3. **Behavioural check (CI)** — slice-06's integration test asserts
   that after `__test_summary_tick_now()` fires, exactly one INFO
   event with the right field set is captured. A regression in the
   aggregator that produces a wrong field value (e.g.
   `kept_error_bearing > kept_total` due to a swapped record method)
   is caught by the slice-06 assertion.

A change that bypasses one layer is caught by another. The
hot-path correctness, the cancellation lifecycle, and the
event-emission contract are all defended in depth.

The "Earned-Trust probe" question is moot for the aggregator
specifically: the aggregator has no external dependency to probe (it
runs entirely in process, against in-memory `AtomicU64`s and a
Tokio runtime that is always available). The decorator's `Probe`
delegation (per ADR-0021) covers the only external dependency Sieve
has — the inner sink. The aggregator's failure modes are bounded by
the type system and the test suite.
