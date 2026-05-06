# ADR-0015 — `spark` single-init invariant and test mechanism

- **Status**: Accepted
- **Date**: 2026-05-06
- **Author**: `nw-solution-architect` (Morgan)
- **Feature**: `spark` v0
- **Supersedes**: none
- **Superseded by**: none

## Context

DISCUSS `wave-decisions.md > D7` locks the single-init invariant:
calling `spark::init` twice in the same process returns
`Err(SparkError::GlobalAlreadyInitialised)` on the second call.

`shared-artifacts-registry.md > otel_global_provider` classifies this
as **HIGH integration risk**: Spark's job is to set the OTel global
tracer/logger/meter providers exactly once with the configured
Resource. A second `spark::init` call must not re-set the global state
mid-process, because:

- Some application code may have already obtained
  `opentelemetry::global::tracer(...)` references that capture the
  previously-set provider via `Arc`. Re-setting the provider invalidates
  the assumptions those references made.
- The OTel SDK's own contract for `set_tracer_provider` is "succeed at
  most once"; subsequent calls return `Err(TraceError::Other(...))`
  or are silently ignored, depending on the SDK version.

DISCUSS does not lock:

1. **The detection mechanism**. Is the "already initialised" state
   tracked by Spark internally (a `static AtomicBool`), or is it
   delegated to the OTel SDK's own `set_*_provider` returning Err?
2. **The test mechanism**. The integration test in Slice 02 asserts
   `GlobalAlreadyInitialised` on the second call. But the OTel global
   tracer provider is **process-global** state; once set in a Cargo
   test binary's process, subsequent tests in the same binary inherit
   it. How does Spark's CI prove the invariant without leaking state
   across tests?

Sentinel's review (`discuss/peer-review.md > Suggestions for Morgan §4`)
explicitly directs: "If OTel's global state cannot be reset between
tests, flag this for DEVOPS and use a one-shot `[[test]]` declaration
in `Cargo.toml`."

## Decision

### 1. Detection — Spark-internal `AtomicBool` plus delegation to OTel SDK

```rust
// in init.rs (illustrative; software-crafter writes the actual code)
use std::sync::atomic::{AtomicBool, Ordering};

static SPARK_INITIALISED: AtomicBool = AtomicBool::new(false);

pub(crate) fn init(config: SparkConfig) -> Result<SparkGuard, SparkError> {
    // 1. Run the lint pass (synchronous, no I/O).
    lint(&config)?;

    // 2. Atomic compare-and-swap on Spark's own flag.
    if SPARK_INITIALISED
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return Err(SparkError::GlobalAlreadyInitialised);
    }

    // 3. Construct the OTel SDK pipeline.
    //    If opentelemetry::global::set_tracer_provider fails (because
    //    some OTHER code in this process already set it), Spark
    //    converts that to GlobalAlreadyInitialised and rolls back
    //    Spark's own flag (so a follow-up retry could succeed if the
    //    other code released its provider).
    let providers = build_providers(&config).map_err(|e| {
        SPARK_INITIALISED.store(false, Ordering::Release);
        e
    })?;

    if let Err(e) = set_global_providers(providers) {
        SPARK_INITIALISED.store(false, Ordering::Release);
        return Err(SparkError::GlobalAlreadyInitialised);
    }

    // 4. Construct and return the SparkGuard.
    Ok(build_guard(/* ... */))
}
```

The Spark-internal `AtomicBool` is the **primary** detection mechanism.
It catches the common case ("application calls `spark::init` twice
in main") cleanly and does not depend on the OTel SDK's behaviour at
the second call.

The delegation to the OTel SDK's `set_*_provider` Err path is the
**defence-in-depth** mechanism: if some other code in the same process
already set a tracer provider via the upstream API (e.g. an application
that uses both `spark::init` and `tracing-opentelemetry::layer().with_tracer(...)`
which sets its own provider), Spark detects that conflict at SDK call
time and reports `GlobalAlreadyInitialised`.

Both mechanisms are needed because the failure modes are different:
- Spark-flag fires when *Spark itself* is called twice.
- SDK-Err fires when *some other code* set the provider before Spark.
- Both produce the same user-facing error variant
  (`GlobalAlreadyInitialised`); the diagnostic is uniform.

### Roll-back on failure

If the lint pass passes but `build_providers` or `set_global_providers`
fails, Spark resets `SPARK_INITIALISED` back to false. This is the
"transactional" property: a failed init does not leave Spark in a
partially-initialised state where a retry would falsely report
`GlobalAlreadyInitialised`.

The roll-back is defence-in-depth for the test scenario. The intent
is that production code calls `spark::init` once and never calls it
again on Err; the rollback makes the testing scenario sane (an
integration test that triggers `ExporterInitFailed` mid-init, then
retries with a different config, gets a clean second attempt).

### 2. Test mechanism — one-process-per-test for global-state cases

The OTel global tracer provider is process-global state with no public
reset API at 0.27. Cargo's default test runner runs all `#[test]`
functions in a binary as concurrent threads inside a SINGLE process.
This means:

- Without intervention, Slice 02's "second call returns
  `GlobalAlreadyInitialised`" test would race with Slice 01's "first
  call returns Ok" test if they ran in the same binary, because both
  manipulate the global provider.
- The same hazard applies across slices: Slice 03, Slice 05, and
  Slice 06 all need a clean global state when their `spark::init`
  call runs.

The mechanism: **each integration test is its own `[[test]]` declaration
in `Cargo.toml`, which Cargo compiles as a separate binary**. Each
binary is invoked as a separate process by `cargo test`, giving each
slice its own pristine OTel global state.

```toml
# crates/spark/Cargo.toml — already in ADR-0011 §"Cargo.toml skeleton"
[[test]]
name = "slice_01_walking_skeleton"
path = "tests/slice_01_walking_skeleton.rs"

[[test]]
name = "slice_02_init_error_paths"
path = "tests/slice_02_init_error_paths.rs"

# ... and so on for slices 03..06
[[test]]
name = "invariant_single_init"
path = "tests/invariant_single_init.rs"

[[test]]
name = "invariant_no_telemetry_on_telemetry"
path = "tests/invariant_no_telemetry_on_telemetry.rs"
```

The same idiom Aperture uses (`crates/aperture/Cargo.toml` already
declares one `[[test]]` per slice). Cargo compiles N separate test
binaries; `cargo test` invokes each as a separate process. Each
process has its own pristine `static AtomicBool` and its own pristine
OTel global state.

### 3. The `GlobalAlreadyInitialised` test specifically

Within `tests/invariant_single_init.rs` (the cross-cutting invariant
test), the body is a SINGLE `#[test]` function that calls
`spark::init` twice and asserts the second call's Err:

```rust
// tests/invariant_single_init.rs (illustrative)
use spark::{init, SparkConfig, SparkError};

#[test]
fn second_init_returns_global_already_initialised() {
    let aperture = aperture::spawn(/* test config */).expect("aperture");

    let _guard = init(
        SparkConfig::for_service("test-svc")
            .with_endpoint(format!("http://{}", aperture.grpc_addr())),
    ).expect("first init succeeded");

    let result = init(
        SparkConfig::for_service("test-svc-2")
            .with_endpoint(format!("http://{}", aperture.grpc_addr())),
    );

    assert!(matches!(result, Err(SparkError::GlobalAlreadyInitialised)));
}
```

**Single `#[test]` function inside the binary, no other tests
present**. This guarantees the binary's process runs exactly two
`init` calls — the first succeeds, the second returns the variant.
No other test in the same binary touches the global state, so the
assertion is deterministic.

The `slice_02_init_error_paths` binary covers the OTHER three error
variants (`MissingRequiredAttribute`, `InvalidEndpoint`,
`ExporterInitFailed`) — those are pure config-validation paths that
do NOT depend on the global state, so they can share a binary
without sequencing concerns. The `GlobalAlreadyInitialised` test
specifically lives in its own binary.

### 4. Other tests' interaction with global state

Slice 01, 03, 05, 06 each call `spark::init` once and let `SparkGuard`
drop at end of test. Each lives in its own `[[test]]` declared
binary; each gets its own pristine global state. The `serial_test`
crate (dev-dep per ADR-0011) handles the OTHER concurrency hazard —
`std::env::set_var` for `OTEL_*` variables in Slice 04 — but is NOT
needed for the global-provider hazard, because the per-binary process
isolation already handles it.

### 5. DEVOPS handoff annotation

This ADR's contract for DEVOPS:

```text
The Spark CI workflow (gate-5-mutants-spark.yml mirroring the
Aperture pattern) MUST run `cargo test --workspace --all-targets
--locked` such that each `[[test]]`-declared integration test
binary runs as a separate process. This is the default `cargo test`
behaviour and requires no special workflow YAML, but the gate-5
mutation-testing setup (`cargo mutants --in-diff`) MUST configure
its `--test-threads=1` setting OR rely on the per-binary process
isolation being sufficient. Spark's mutation tests do not require
`--test-threads=1` because the per-binary isolation is sufficient
for the global-state hazards; mutation tests targeting `init.rs`
specifically can opt in to single-threaded execution if a future
mutation surfaces a within-binary thread-safety issue.

The `[[test]]` declarations in Spark's Cargo.toml are part of the
v0 contract; renaming or merging them into a single binary would
re-introduce the global-state hazard the per-binary scheme exists
to prevent.
```

This text appears verbatim in `wave-decisions.md` "Handoff to DEVOPS"
section.

## Alternatives Considered

### Option A — Spark-internal `AtomicBool` + delegation to OTel SDK + per-binary test isolation (RECOMMENDED, accepted)

Detailed above.

**Pros**:
- Catches both common failure modes (Spark called twice; OTel set by
  some other code first).
- Per-binary test isolation is the same idiom Aperture uses and Cargo
  natively supports.
- Roll-back on failed init means the testing surface is sane.

**Cons**:
- Two detection mechanisms to maintain (the AtomicBool and the SDK
  delegation). Acceptable: they are short, both are exercised by
  tests, and the `cargo mutants` Gate 5 catches drift between them.

### Option B — Pure delegation to the OTel SDK's `set_*_provider` Err

```rust
// rejected
pub(crate) fn init(config: SparkConfig) -> Result<SparkGuard, SparkError> {
    lint(&config)?;
    let providers = build_providers(&config)?;
    if let Err(e) = opentelemetry::global::set_tracer_provider(providers.tracer.clone()) {
        return Err(SparkError::GlobalAlreadyInitialised);
    }
    // ... etc for logger and meter ...
    Ok(build_guard(/* ... */))
}
```

**Pros**:
- One mechanism, less code.

**Cons**:
- The OTel SDK's `set_tracer_provider` at 0.27 returns `()`, not
  `Result`. The actual signature is
  `opentelemetry::global::set_tracer_provider(provider: TracerProvider)`
  — it is infallible from the API's seat. The "second call wins"
  semantics are silent at the API level. Spark cannot detect "already
  set" through the SDK call alone.
- If a future SDK version adds a Result, behaviour changes silently
  with the upgrade. Spark's contract ("second call returns Err")
  would become version-dependent.

**Rejected** because the OTel SDK's API does not expose the signal at
0.27. Spark must own the detection.

### Option C — Spark-internal `AtomicBool` only (no SDK delegation)

**Pros**:
- One mechanism.

**Cons**:
- Misses the case where some non-Spark code sets the OTel global
  provider before `spark::init`. Such an application would proceed
  with Spark's lint pass, build providers, and then call
  `set_tracer_provider` — which, at 0.27, silently *replaces* the
  prior provider. That is exactly the silent-replacement hazard the
  invariant exists to prevent.

**Rejected** because the SDK-delegation defence-in-depth catches the
silent-replacement case.

### Option D — Use `std::sync::Once` for the init

```rust
// rejected
static INIT: std::sync::Once = std::sync::Once::new();

pub fn init(config: SparkConfig) -> Result<SparkGuard, SparkError> {
    let mut result: Result<SparkGuard, SparkError> = Err(SparkError::GlobalAlreadyInitialised);
    INIT.call_once(|| {
        result = real_init(config);
    });
    result  // moved out of the closure; would need an Arc<Mutex<...>> dance
}
```

**Pros**:
- Standard library idiom for "exactly once".

**Cons**:
- `std::sync::Once::call_once` does not return a value from the closure
  cleanly; you need an external `Arc<Mutex<Option<T>>>` to capture the
  result, which is more code than the AtomicBool approach.
- The "the closure ran exactly once OR is currently running" semantics
  of `Once` are slightly different from "Spark::init was called
  exactly once and succeeded" — `Once` does not let a failed call
  reset, which breaks the "transactional retry" property.
- The roll-back-on-failure property of Option A is incompatible with
  `Once`: once the closure has run, `Once` is permanently latched.

**Rejected** because `Once`'s "no reset" property prevents the
transactional roll-back the test surface needs.

### Option E — Single-test-per-process via `cargo test --test-threads=1`

**Pros**:
- No `[[test]]` declarations needed.

**Cons**:
- Process-level isolation requires separate binaries, not just
  separate threads in the same process. `--test-threads=1` runs all
  tests in one process serially; the OTel global state still leaks
  between them.
- Slows the test suite proportionally (8 binaries × N tests vs N
  tests serial).

**Rejected** because it does not actually solve the global-state
problem.

## Consequences

### Positive

- The single-init invariant is enforced at two layers: Spark's flag
  (catches Spark-called-twice) AND the SDK delegation (catches
  someone-else-set-it-first).
- The roll-back-on-failure property keeps the testing surface sane
  without weakening the production guarantee.
- Per-binary test isolation is a Cargo-native pattern; no new tooling
  needed.
- The `[[test]]` declarations in `Cargo.toml` give DEVOPS clean per-
  slice CI step boundaries (one row per slice in the runner's UI).

### Negative

- Eight test binaries take longer to compile than one. Acceptable: the
  compile time is amortised once per CI run; the per-test wall-clock
  benefit (no global-state coordination) dominates.
- The Spark-internal `AtomicBool` is shared state; future changes to
  Spark must respect its lifecycle. The roll-back-on-failure semantic
  is the load-bearing detail, and `cargo mutants` Gate 5 mutates the
  flag operations as a regression net.

### Trade-off ATAM

This decision is a sensitivity point for **Reliability — Maturity**
(positive: two-layer detection prevents silent global-state collisions)
and for **Maintainability — Testability** (positive: per-binary
process isolation gives every test a clean global state).

It is a sensitivity point for **Performance Efficiency — Resource
Utilisation** (negative: 8 separate test binaries × the build per
configuration). Accepted because compile time scales linearly with
binary count and Cargo's incremental builds amortise the cost.

## Self-Application of Earned Trust (principle 12)

The single-init contract is enforced by:

1. **Subtype check (compile-time)** — `static AtomicBool` cannot be
   accidentally re-initialised; the language's `static` semantics
   prevent it.
2. **Structural check (CI)** — The `invariant_single_init` test binary
   is its own `[[test]]` declaration. A future code change that
   accidentally merged it into another test binary would be caught
   in PR review (`cargo public-api` watches the test list shape;
   `cargo deny` watches `[[test]]` declarations through the manifest).
3. **Behavioural check (CI)** — The `invariant_single_init` test
   itself: two `init` calls, the second returning
   `GlobalAlreadyInitialised`, asserted by `matches!`. A regression
   in the AtomicBool roll-back logic would either let the second call
   succeed (test fails) or break the first call (test fails). The
   mutation tests Gate 5 mutates the AtomicBool operations
   (`compare_exchange` -> `compare_and_swap`, `Ordering::AcqRel` ->
   `Ordering::Relaxed`) and must be killed by this test.

The two-layer detection (AtomicBool + SDK delegation) provides
redundant evidence: a single-layer bypass is caught by the other.
Specifically, a malicious or buggy code path that sets the OTel
provider directly without going through Spark is detected by the SDK
delegation; a pure double-init through Spark is detected by the
AtomicBool. Both produce the same Err variant; the diagnostic is
uniform.
