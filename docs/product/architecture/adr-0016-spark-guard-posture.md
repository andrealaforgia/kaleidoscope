# ADR-0016 — `SparkGuard` posture: opaque, `#[must_use]`, Drop-only

- **Status**: Accepted
- **Date**: 2026-05-06
- **Author**: `nw-solution-architect` (Morgan)
- **Feature**: `spark` v0
- **Supersedes**: none
- **Superseded by**: none

## Context

`SparkGuard` is the value type returned from `spark::init`. DISCUSS
`wave-decisions.md > Q4` locks the contract:

- Returned from `spark::init` on `Ok`.
- Application stores it in a local variable so its `Drop` runs at
  scope exit.
- `Drop` flushes pending exports synchronously with the configured
  deadline (mechanism in ADR-0014).

`shared-artifacts-registry.md > spark_guard_type` classifies the
guard as **HIGH integration risk**: losing the guard (binding to `_`
instead of `_guard`) drops it immediately, flushing nothing useful and
stopping the OTel pipeline before the application has emitted anything.

DISCUSS does NOT lock:

1. The `#[must_use]` posture. The journey assumes opaque + must-use,
   but it is not declared.
2. Whether the guard exposes any public methods (e.g. `shutdown()`
   for explicit-not-Drop-time flush).
3. Whether the guard exposes any public fields.
4. The Debug shape (does `{:?}` reveal the resolved configuration?).
5. The Send / Sync posture (can the guard cross threads?).

Sentinel's review (`discuss/peer-review.md > Suggestions for Morgan §5`)
explicitly directs: "`SparkGuard` posture. `#[must_use]`? Opaque? The
journey assumes opaque."

## Decision

### 1. Opaque struct with private fields

```rust
// in guard.rs (illustrative; software-crafter writes the actual code)
#[must_use = "SparkGuard must be held for the lifetime of the application; binding to `_` drops it immediately and stops the OTel pipeline before any telemetry is emitted"]
pub struct SparkGuard {
    inner: Option<Inner>,
}

// Inner is pub(crate); never exposed.
pub(crate) struct Inner {
    tracer_provider: opentelemetry_sdk::trace::SdkTracerProvider,
    logger_provider: opentelemetry_sdk::logs::SdkLoggerProvider,
    meter_provider: opentelemetry_sdk::metrics::SdkMeterProvider,
    flush_timeout: std::time::Duration,
}

impl Drop for SparkGuard {
    fn drop(&mut self) {
        let Some(inner) = self.inner.take() else {
            return;  // second drop is a no-op (ADR-0014 §4)
        };
        // ... per-provider sequential flush with shared budget per
        //     ADR-0014 §1; tracing event emission per ADR-0014 §2 ...
    }
}
```

**No public fields.** Every field is private; the struct is opaque
from the consumer's perspective. The `Inner` carrier is `pub(crate)`
to keep the drop-time helpers in `guard.rs` private to the crate.

### 2. `#[must_use]` with a directive message

```rust
#[must_use = "SparkGuard must be held for the lifetime of the application; binding to `_` drops it immediately and stops the OTel pipeline before any telemetry is emitted"]
pub struct SparkGuard { /* ... */ }
```

The `#[must_use]` attribute makes the compiler emit a warning if the
return value of `spark::init` is discarded:

```rust
spark::init(config);  // -> warning: unused `Result` that must be used
spark::init(config)?; // -> warning: unused `SparkGuard` that must be used
let _ = spark::init(config)?;  // -> binds to _ which discards immediately
```

The directive message is the documentation explaining WHY. A
consumer reading `cargo build` output sees the warning AND the
explanation; they do not need to read the docs to understand the
hazard.

The canonical pattern:

```rust
let _guard = spark::init(config)?;  // _guard binds (does NOT discard)
// ... application code ...
// _guard drops at end of scope.
```

`let _guard = ...` binds to a named variable (which lives until the
end of scope); `let _ = ...` is the discard pattern (which drops
immediately). The `#[must_use]` does not distinguish between the two
at compile time, but the doctring + the directive message + the
canonical pattern in every example file make the right idiom obvious.

### 3. No public methods on `SparkGuard`

```rust
// REJECTED:
impl SparkGuard {
    pub fn shutdown(self) -> Result<(), SparkError> { /* explicit flush */ }
    pub fn flush_now(&self) -> Result<(), SparkError> { /* mid-life flush */ }
    pub fn timeout(&self) -> Duration { /* expose configured timeout */ }
}
```

`SparkGuard` has **NO public methods at v0**. The only operation a
consumer performs is letting it drop (or `drop(guard)` explicitly,
which is the same operation expressed differently).

Why no `shutdown()`?

- The whole point of the RAII pattern is to avoid the
  "did-the-developer-remember-to-call-shutdown" hazard. Adding a
  `shutdown()` method that does the same thing as `Drop` creates two
  paths and invites bugs (double-flush, half-state).
- DISCUSS `wave-decisions.md > D1` locks "spark::init is the only
  public entry point at v0". Adding `SparkGuard::shutdown` would
  silently introduce a second entry point for the operation Drop
  already performs.
- `drop(guard)` is the explicit-shutdown idiom Rust already provides,
  and it does the right thing because `Drop::drop` runs synchronously
  with `flush_timeout` bounded.

Why no `flush_now()`?

- Mid-application flush is a feature DISCUSS does not name. The OTel
  SDK's batch processor handles flush scheduling; the application
  calls `opentelemetry::global::tracer_provider().force_flush()`
  directly if it really needs an out-of-band flush — Spark does not
  need to add an alias.
- The `force_flush_with_timeout` semantics inside Drop use the
  remaining-budget arithmetic (ADR-0014); replicating that on a
  mid-life flush would couple two state machines.

Why no `timeout()` accessor?

- The configured timeout was set on `SparkConfig`; the application
  has it. Re-exposing it via the guard adds a redundant accessor with
  no use case.

### 4. Debug — minimal, no resolved configuration

```rust
impl std::fmt::Debug for SparkGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SparkGuard").finish_non_exhaustive()
    }
}
```

`{:?}` on a `SparkGuard` prints `SparkGuard { .. }`. No fields, no
configuration leak. Reasoning:

- The resolved configuration (endpoint, protocol, flush_timeout) is
  observable via the resolved-config tracing event Spark emits at
  init (Slice 04 §"Resolved configuration is observable on the
  tracing facade"). Re-exposing it via Debug duplicates the
  observability surface.
- Some applications may consider the resolved endpoint sensitive
  (e.g. an internal Aperture URL). Debug-printing the guard at an
  unfortunate log level would leak it.
- The journey-spark.yaml mockups never `dbg!()` the guard; the
  resolved configuration is always shown via the tracing INFO event.

The `finish_non_exhaustive()` mark in the Debug formatter signals
"there are private fields not shown" without listing them. Future
changes to `Inner` are non-breaking from the Debug surface's
perspective.

### 5. Send + Sync (compiler-inferred)

`SparkGuard` does NOT explicitly implement `Send` or `Sync`. The
inner OTel SDK provider types (`SdkTracerProvider`, etc.) are `Send +
Sync` per the upstream OTel SDK contract; `Option<Inner>` is `Send +
Sync` if `Inner` is. Spark's `SparkGuard` is therefore implicitly
`Send + Sync`.

This means a guard CAN cross thread boundaries (an application that
spawns a worker thread and moves the guard into it would observe the
flush on that thread's exit). Whether that is a good idea is
application-specific; Spark does not forbid it.

The journey-spark.yaml does not require Sync; the test scenarios all
hold the guard in `main`. But upstream is Send + Sync, so there is
no benefit to artificially restricting Spark.

### 6. No Clone, no Copy, no PartialEq

```rust
// (no derives beyond Debug)
pub struct SparkGuard { /* ... */ }
```

`SparkGuard` is **not `Clone`**: the underlying OTel providers are
`Arc<Inner>` shared state, but Spark's guard semantically OWNS the
"this guard's drop will flush these providers" relationship. Cloning
the guard would create two values both responsible for flushing on
drop, which would either double-flush (per-provider) or race on the
`Option::take()`.

`SparkGuard` is **not `Copy`** for the same reason.

`SparkGuard` is **not `PartialEq`**: there is no meaningful equality
relation between two guards; consumers do not compare guards.

Minimum-trait posture matches ADR-0012's `SparkError` posture (no
extra trait impls without a customer).

### 7. Stale guard on err path of `init`

If `spark::init` returns `Err`, no `SparkGuard` is constructed.
There is no half-constructed guard to leak. ADR-0011 specifies the
`init` call orders the lint pass BEFORE provider construction, and
ADR-0015 specifies the AtomicBool flag is rolled back on any
post-flag-set failure. The combined guarantee: a returned `Err`
means "no global state was changed; no resources were acquired; no
guard exists". Drop of a non-existent guard is a no-op (the value
was never constructed).

## Alternatives Considered

### Option A — Opaque, `#[must_use]`, Drop-only, no public methods (RECOMMENDED, accepted)

Detailed above.

**Pros**:
- Minimum surface area; nothing to misuse.
- `#[must_use]` warning catches the silent-discard hazard at compile
  time.
- RAII is the canonical Rust idiom for resource lifetimes.
- Consistent with `tracing-subscriber::fmt::WorkerGuard` which has
  the exact same shape (opaque, must-use, Drop-only).

**Cons**:
- Applications that want to emit a span "after explicit shutdown"
  must reorganise their code so `drop(guard)` runs after the last
  emission. Acceptable: this is what RAII teaches.

### Option B — Opaque + `shutdown(self)` method

```rust
impl SparkGuard {
    pub fn shutdown(self) -> Result<(), SparkError> { /* explicit flush */ }
}
```

**Pros**:
- Lets the application explicitly handle a flush failure (e.g. log
  the error returned by `shutdown` rather than silently dropping it
  via `tracing::warn!`).

**Cons**:
- Two ways to flush (Drop and shutdown) violates "one and only one
  obvious way". Application code that calls `shutdown(guard).unwrap()`
  followed by `drop(guard)` would either double-flush or be a compile
  error (depending on whether `shutdown` consumes self).
- DISCUSS D1 explicitly forbids "spark::shutdown" as a public entry
  point. A method on the guard is a soft-form of the same entry
  point.

**Rejected** because the existing `tracing::warn!` path on Drop
already surfaces the deadline-exceeded case to the application's
tracing subscriber. The application that wants the failure observable
subscribes to its own tracing; it does not need an explicit shutdown
return value.

### Option C — Public fields exposing the resolved configuration

```rust
pub struct SparkGuard {
    pub endpoint: String,
    pub flush_timeout: Duration,
    // ...
}
```

**Pros**:
- Application can introspect the resolved config without reading
  tracing events.

**Cons**:
- Stable contract on every field. Adding a future field (e.g.
  `tls_enabled: bool` in Aegis Phase 2) is a breaking change at the
  pattern-construction sites.
- Re-exposes information already on the tracing INFO event. Two
  observability surfaces for the same data.
- Some fields may be sensitive (an internal endpoint URL); making
  them public encourages logging them at `Debug` level.

**Rejected** because the resolved-config tracing event is the
designed observability surface; the guard's job is RAII, not
configuration introspection.

### Option D — Trait `SparkLifetime`

```rust
pub trait SparkLifetime { fn shutdown(self); }
impl SparkLifetime for SparkGuard { fn shutdown(self) { drop(self); } }
```

**Pros**:
- "Polymorphic" lifetime management for future test doubles.

**Cons**:
- No customer at v0 (DISCUSS rejects trait-based plugin shapes for
  the same reason).
- Adds an indirection that conveys no information.

**Rejected**. Aperture's testing seam (`testing::RecordingSink`) does
not need a trait on Spark's guard; the guard's behaviour is observable
through the tracing events Drop emits.

## Consequences

### Positive

- The opaque + must-use posture catches the silent-discard hazard at
  compile time. The directive message names the consequence
  ("stops the OTel pipeline before any telemetry is emitted") so
  the compiler warning is self-explaining.
- Drop-only contract means there is exactly one way to flush. No
  double-flush race; no half-state; no "did the application call
  shutdown?" question.
- The journey-spark visual mockups (the only reads of
  `SparkGuard`'s shape) all show `let _guard = ...`; the canonical
  pattern is established by the design and reinforced by the
  `#[must_use]` directive.

### Negative

- The doctring on `spark::init` MUST explain the `let _guard = ...`
  pattern; without it, a new consumer might write `let _ = spark::init(config)?;`
  and observe silent telemetry loss. Acceptable: every example file
  + the doctring + the directive message all reinforce the pattern.
- A future operation that legitimately needs a guard method (e.g.
  "register an exit hook") would have to pass through a separate API
  rather than as a method on the guard. Acceptable: such an API
  belongs on `SparkConfig` builder methods or as a separate function,
  not on the lifetime-management value.

### Trade-off ATAM

This decision is a sensitivity point for **Maintainability —
Modifiability** (positive: opaque + private fields means future
changes to the inner shape are non-breaking) and for **Reliability
— Fault Tolerance** (positive: RAII is the guarantor that Drop runs
on every exit path the application has).

It is a sensitivity point for **Usability — Operability** (positive:
the `#[must_use]` warning teaches the canonical pattern at compile
time; the directive message explains the consequence).

It is a trade-off point against **Functional Suitability —
Appropriateness** (negative: applications that want explicit-shutdown
return values do not get them; they get tracing events and the
canonical-error-handling pattern). Accepted because the simplicity
gain dominates: every Spark consumer learns one pattern, applies it
in `main`, and the hazard is structurally prevented.

## Self-Application of Earned Trust (principle 12)

The opaque-guard contract is enforced by:

1. **Subtype check (compile-time)** — Private fields cannot be
   accessed from outside the module. Rust's visibility rules are the
   gate.
2. **Structural check (CI)** — `cargo public-api` (Gate 2 of ADR-0011)
   reads the public surface and refuses commits that promote any
   `Inner` field to `pub`. The diff is the rejection.
3. **Behavioural check (CI)** — Slice 06's tests assert the
   observable behaviour (clean flush emits INFO, deadline emits
   WARN, downed-downstream does not panic). A regression that exposes
   the inner shape via, e.g., a new accessor would not be caught
   structurally if `cargo public-api` accepted the addition; the
   behavioural check is the substrate that says "the contract is the
   tracing events, not the struct".

The `#[must_use]` directive is enforced by:

1. **Subtype check** — `rustc` emits the warning. With
   `#![deny(unused_must_use)]` in the crate's lints (a recommendation
   for Spark consumers' own crates, not a Spark-side enforcement),
   it becomes an error.
2. **Structural check** — Spark's example files all use `let _guard = ...`;
   `cargo build --examples` (Gate 1) compiles them; a regression that
   broke the canonical pattern would surface there.
3. **Behavioural check** — Slice 01's integration test runs the
   walking-skeleton example end-to-end; if the example accidentally
   discarded the guard, no `ExportTraceServiceRequest` would reach
   the RecordingSink and the test would fail.
