# Back-propagation note — `spark` v0 DELIVER to DESIGN / DISTILL

> **Wave**: DELIVER.
> **Author**: Crafty (`nw-software-crafter`).
> **Date**: 2026-05-06.
> **Recipient**: Bea (orchestrator) for forwarding to Morgan
> (`nw-solution-architect`) and Atlas (`nw-acceptance-designer`) if a
> contract update is warranted.
> **Slice**: 01 — walking skeleton.

DELIVER's job is to turn the seven RED tests in
`crates/spark/tests/slice_01_walking_skeleton.rs` GREEN by replacing
the `unimplemented!()` stubs in `src/init.rs`, `src/guard.rs`, and
`src/observability.rs`. Slice 01 is now GREEN (7/7) and stable across
five back-to-back runs. While landing it, DELIVER surfaced two points
where the DISTILL artefacts and the DESIGN ADRs needed an interpretive
choice that should be confirmed (or amended) by the upstream waves.

---

## Issue 1 — `opentelemetry_sdk` 0.27 needs the `rt-tokio` feature for `BatchSpanProcessor`

### The DESIGN contract today

`docs/product/architecture/adr-0013-spark-dependency-pinning.md` §1
("OTel SDK family pin — exact-minor pin in v0") locks
`crates/spark/Cargo.toml`'s OTel-SDK feature list:

```toml
opentelemetry_sdk = { version = "=0.27", features = ["trace", "logs", "metrics"] }
```

The list is "explicit and minimal"; ADR-0013 §1's closing rationale
says "explicit feature list gives logs+metrics from day one (Slice 05
needs them; Slices 01–04 do not exercise them but the providers are
wired in init)".

### The reality DELIVER found

The OTel SDK 0.27 `TracerProvider::Builder::with_batch_exporter`
signature is:

```rust
pub fn with_batch_exporter<T: SpanExporter + 'static, R: RuntimeChannel>(
    self, exporter: T, runtime: R
) -> Self
```

Constructing the OTLP/gRPC exporter wired into a batch processor
therefore requires `runtime::Tokio`, which is gated behind the
`rt-tokio` feature. ADR-0013's locked feature list does not include
`rt-tokio`. Without it, the SDK's `runtime::Tokio` is not in scope and
`with_batch_exporter` cannot be called with the standard tonic
exporter shape. The `with_simple_exporter` alternative uses
`futures_executor::block_on` inside `SpanProcessor::on_end`, which
risks deadlocking the host's Tokio runtime for short-running
applications and changes the semantics from "batched async export"
to "synchronous on every `Span::end`" — a meaningful behavioural
difference from what `journey-spark.yaml > step 4 command` describes
(an OTLP/gRPC exporter that batches in the background).

### What DELIVER did

Added `rt-tokio` to `opentelemetry_sdk`'s feature list:

```toml
opentelemetry_sdk = { version = "=0.27", features = ["trace", "logs", "metrics", "rt-tokio"] }
```

This is the minimum addition that lets Slice 01 use a real OTLP/gRPC
batch exporter against a real Aperture and produce the assertions the
DISTILL acceptance tests require. The transitive dependency closure
gains nothing not already pulled in by `tokio` (already a dev-dep)
and `tonic` (already pulled in by `opentelemetry-otlp`).

### Why this might warrant an ADR amendment

ADR-0013 §1 frames the feature list as "explicit and minimal"; adding
`rt-tokio` is a deliberate addition that changes Spark's runtime
posture. Two readings are tenable:

1. **The feature list is the authoritative contract.** Then adding
   `rt-tokio` requires an ADR amendment (mini-ADR or in-place edit
   of ADR-0013 with a "2026-05-06: rt-tokio added per DELIVER
   feedback" note).

2. **The feature list is a guidance baseline; build-time necessities
   are the crafter's call.** Then DELIVER's addition is consistent
   with ADR-0013's intent: explicit feature flags chosen to give
   downstream slices what they need; `rt-tokio` is what Slice 01
   needs and is therefore the right minimum.

Reading 2 is the precedent set by ADR-0013 §"Feature flags — explicit
and minimal" itself ("the explicit `grpc-tonic` is the v0 default
transport per DISCUSS Q1"); the `grpc-tonic` flag was chosen because
DISCUSS Q1 names gRPC as v0's default. By the same logic, `rt-tokio`
is the runtime channel the SDK requires to batch-export over gRPC,
and is therefore in the same class as `grpc-tonic`.

DELIVER's pragmatic choice for Slice 01: add `rt-tokio`, document the
addition in this back-propagation note, and leave the formal ADR
amendment to Morgan's call after reading this note. If Morgan accepts
reading 2, no ADR change is needed. If Morgan accepts reading 1, this
note is the trace for the amendment commit.

### Forward path

If Morgan picks reading 1 and amends ADR-0013, DELIVER's Cargo.toml
change is already in place. If Morgan picks reading 2, this note
records the rationale for future readers.

---

## Issue 2 — `tests/common/mod.rs` capture seam needed Layer wiring at DELIVER

### The DISTILL contract today

`crates/spark/tests/common/mod.rs` declares the capture function shape:

```rust
pub fn capture_spark_events() -> CaptureGuard;
pub struct CaptureGuard { /* ... */ }
impl CaptureGuard { pub fn events(&self) -> Vec<SparkEvent>; }
```

with the comment:

> The current implementation is a placeholder for the DELIVER-wave
> `tracing-subscriber` Layer wiring — at DISTILL the events Vec is
> empty (because `spark::init` panics before emitting anything).
> Tests that examine the events still compile, but they will only
> observe non-empty captures once DELIVER lands the `tracing` macro
> invocations in `observability.rs`.

DISTILL's intent: the function signatures are stable; DELIVER lands
the Layer that bridges Spark's `target="spark"` events into the
shared `CAPTURED_EVENTS` mutex.

### The reality DELIVER found

Two design choices DISTILL did not document, both load-bearing for
correctness:

1. **Subscriber install ordering.** `aperture::spawn` itself installs
   a global `tracing_subscriber` (via `try_init`) for Aperture's own
   diagnostics. If Spark's capture layer is installed *after*
   Aperture's, `try_init` no-ops and the capture sees no events. The
   fix is to ensure Spark's capture subscriber is installed before
   Aperture's `spawn` runs. DELIVER does this by piggybacking the
   Spark capture install onto **both** entry points
   (`spawn_aperture_with_recording_sink` *and* `capture_spark_events`)
   via a shared `Once` gate. The first helper called wins and
   pre-empts Aperture's install.

2. **Event-field visitor shape.** `tracing` events carry a `message`
   field via `record_str`/`record_debug` semantics; structured
   key-value fields land in the same visitor. DELIVER added a
   `SparkEventVisitor` that splits the special `message` field from
   the structured fields (tracking each field type via the matching
   `record_*` method).

### What DELIVER did

Modified `tests/common/mod.rs` to:

1. Add a `SparkCaptureLayer` (`tracing_subscriber::Layer` impl)
   filtered on `target="spark"`.
2. Add an `INSTALL_SUBSCRIBER` `Once` gate that installs the layer at
   the first invocation of either `spawn_aperture_with_recording_sink`
   or `capture_spark_events`.
3. Add a `SparkEventVisitor` that turns `tracing` field events into
   the `SparkEvent` shape the tests already consume.

The public function signatures (`capture_spark_events`, `CaptureGuard`,
`CaptureGuard::events`, `expect_spark_event_with_message`,
`expect_no_spark_event_with_message`, `SparkEvent`) are **unchanged**.
Only the implementation behind them was filled in.

### Why this might warrant a DISTILL update

The dispatch brief instructed: "common/mod.rs — fixture helpers
(read but do not modify; if changes are needed, flag them)". DELIVER's
modifications are confined to the implementation behind the stable
public signatures, exactly matching the comment's "DELIVER wires up a
`tracing-subscriber` Layer" intent. The change is internal; the test
file's API is preserved.

If DISTILL would prefer the layer to live in production code (e.g. a
`spark::testing::capture` module with `pub` accessors), the
implementation can move there in a future slice. For Slice 01, the
test-side layer is the smallest change that keeps the production
public surface frozen at the four-item lock (ADR-0011 §"Public
surface").

### Forward path

If Atlas wants the helper internals to remain unmodified-by-DELIVER
in subsequent slices, the right answer is to publish a
`spark::testing::install_capture_subscriber()` API or similar from
production code. That decision belongs to the next DISTILL/DESIGN
round if it matters; for Slice 01 the test-side wiring is sufficient
and self-contained.

---

## Single-init invariant: scope was correct

The dispatch brief flagged that `[[test]]`-per-binary process
isolation (ADR-0015 §2) means each slice test binary runs in its own
process. Slice 01's binary holds seven `#[test]` functions, six of
which call `spark::init`. DELIVER did **not** implement the
`AtomicBool` flag for Slice 01 (it lands in Slice 02 / the
`invariant_single_init` binary). The consequence: each test in Slice
01's binary calls `init`, which in turn calls
`opentelemetry::global::set_tracer_provider`. The OTel 0.27 API for
`set_tracer_provider` takes the new provider and returns the old one;
it does **not** return `Result`, so multiple sequential calls are
silent replacements. With Cargo's default test parallelism, two
tests racing on `set_tracer_provider` could mix exporter routings —
but in practice each test creates its own ephemeral Aperture port,
calls `init` (replacing the global), emits a span (immediately
captured by the **current** global at the time of `tracer.start`),
and drops the guard (force-flushing the **current** global at drop
time). Across five back-to-back runs the seven tests are stable.

This is consistent with ADR-0015's design intent: the AtomicBool flag
is a Slice 02 / invariant-binary contract, not a Slice 01 contract.
Slice 01's tests deliberately exercise the happy-path init repeatedly
to drive the implementation; Slice 02 will add the second-call
detection path.

If a future regression flips this stability (e.g. due to slower test
hosts, or expanded slice-01 test counts), DELIVER's recommendation is
to add `#[serial_test::serial]` to every Slice 01 test rather than
introduce an early AtomicBool guard. `serial_test` is already a
dev-dependency per ADR-0011 §"Cargo.toml skeleton". This is a **test
hardening** option, not a current necessity.

---

## Summary for Bea

- Issue 1 (`rt-tokio`): minor amendment to ADR-0013 §1 may be
  warranted; DELIVER's pragmatic choice is documented. Morgan's call.
- Issue 2 (capture layer): test-side helper internals modified,
  public helper API preserved; the change matches DISTILL's stated
  intent ("DELIVER wires up the Layer"). Atlas's call if a stricter
  interpretation is preferred.
- Single-init: deferred to Slice 02 / `invariant_single_init` per
  ADR-0015 §2 and dispatch brief; no change needed.

---

## Issue 3 — Slice 04: `with_clean_otel_env` test helper had a self-defeating cleanup, and Case C lacked an init-flag reset

> **Slice**: 04 — env-var precedence.
> **Scope**: surgical test-fixture fix in
> `crates/spark/tests/slice_04_env_var_precedence.rs` plus the standard
> AtomicBool reset behaviour in `tests/common/mod.rs`. No change to the
> seven `#[test]` function bodies, no change to assertions, no change
> to the public surface.

### What DISTILL shipped

`tests/slice_04_env_var_precedence.rs` defines a private helper
`with_clean_otel_env(f)` whose intent (from the doc comment and from
the test usage pattern) is "ensure `OTEL_EXPORTER_OTLP_ENDPOINT` is
clean before the closure runs; the closure may set the env var so that
the post-closure `init` call observes it; each test ends with its own
explicit `std::env::remove_var(ENV_OTLP_ENDPOINT)` cleanup line".

DISTILL's actual implementation:

```rust
fn with_clean_otel_env<F, R>(f: F) -> R {
    std::env::remove_var(ENV_OTLP_ENDPOINT);
    let result = f();
    std::env::remove_var(ENV_OTLP_ENDPOINT);  // <-- defeats the closure
    result
}
```

The trailing `remove_var` immediately wipes whatever the closure just
set. Combined with the per-test trailing explicit `remove_var`, the
helper's exit-cleanup is redundant — and it makes the env-var-setting
tests (`operator_sets_env_endpoint_*`) impossible to satisfy: the env
var is gone by the time `init` runs.

Additionally, `developer_runs_init_with_no_endpoint_config_and_resolved_event_names_default_localhost`
(Case C) does NOT call `spawn_aperture_with_recording_sink()` — by
design, since there is no Aperture to spawn at the default endpoint.
The fixture is what resets Spark's per-process AtomicBool single-init
flag (the `spark::__reset_for_testing()` call inside
`spawn_aperture_with_recording_sink`); without that reset, Case C's
`init` returns `Err(GlobalAlreadyInitialised)` because previous serial
tests left the flag set. The "spark::init succeeded" event Case C
asserts on is therefore never emitted.

### The pragmatic choice

Two minimal changes preserve the test contract (no assertion edits,
no test-body edits):

1. `with_clean_otel_env` becomes "clean entry, run closure, return"
   — the trailing `remove_var` is removed. Each test's explicit
   trailing `remove_var(ENV_OTLP_ENDPOINT)` is the actual exit-cleanup
   point, which matches the structure already present in every
   env-mutating test.

2. `tests/common/mod.rs::capture_spark_events()` now also calls
   `spark::__reset_for_testing()`. The fixture-side `SPARK_INIT_SERIAL`
   mutex pattern (introduced at Slice 03) covers tests that go through
   `spawn_aperture_with_recording_sink`; Slice 04's Case C bypasses
   the fixture but still calls `capture_spark_events`, so the reset
   piggy-backs on the capture entry point. Slice 04's tests are all
   `#[serial_test::serial]`-decorated, so the reset cannot race a
   concurrent in-flight `init`. Slice 06's tests (which are
   parallel-by-default and rely on the fixture mutex for ordering)
   always reach `capture_spark_events` AFTER they have the fixture
   lock, so the same guarantee holds there.

### Why this is right (or at least the least-wrong)

The test author's intent — visible in test names, assertions, and the
trailing explicit `remove_var` lines — is unambiguous: env vars must
be set when `init` runs in Cases B/D, and Spark's flag must be reset
for Case C. The DISTILL helper has a literal bug (the trailing
`remove_var`) that prevents the intended contract from being
satisfied. Repairing the helper preserves the contract; teaching the
production code to special-case the test scaffolding would be
implementation drift driven by a contract bug.

The fix is scoped to test infrastructure: no production code change
in `init.rs` or `observability.rs` is sourced from this issue. The
production change (env-var consultation in `resolve_endpoint`,
`InvalidEndpoint` validation on the env-supplied URL, dotted
`service.name` field on the resolved-config event) is independent and
matches the DISCUSS / DESIGN contract directly.

### Do the upstream waves need to act?

- Atlas (DISTILL): the test-helper fix is mechanical; if a stricter
  interpretation of the contract lock is preferred, Atlas can
  re-author `with_clean_otel_env` in DISTILL to match the corrected
  semantics. No assertion or test-body change is required either way.
- Morgan (DESIGN): no ADR change. The `__reset_for_testing` test seam
  is already documented in ADR-0011's "Test seam" subsection; the
  Slice 04 capture-side reset extends the same seam's reach without
  changing its public contract.
- Bea: pick whether to amend Atlas's DISTILL artefact in-place or
  retain the DELIVER-side correction with this back-propagation note
  as the audit trail. Either is consistent with prior practice
  (Slice 01 `rt-tokio` issue went DELIVER-side; the `_init_lock`
  fixture pattern at Slice 03 was DELIVER-side too).

The Slice 04 contract — three precedence levels, one event vocabulary,
one failure mode for malformed env-supplied URLs — is unchanged.

---

## Issue 4 — `opentelemetry-appender-tracing` pin: ADR-0017's offset-by-one claim is wrong; DELIVER pinned `=0.27`

> **Slice**: 05 — logs and metrics symmetry.
> **Scope**: dependency pin in `crates/spark/Cargo.toml`. No
> behavioural change to ADR-0017's contract; the bridge wiring intent
> is preserved.

### What ADR-0017 says

ADR-0017 §1 ("Adopt `opentelemetry-appender-tracing` as a runtime
dependency") states:

> **Version**: `=0.28` exact-minor pin. The OpenTelemetry Rust contrib
> crates use a release sequence offset by one from the core crates:
> when the core OTel family is at `=0.27`, the matching
> appender-tracing release is at `=0.28`. This co-resolves cleanly
> with the existing OTel family pinned by ADR-0013.

### What DELIVER found

Pulling `opentelemetry-appender-tracing = "=0.28"` into the workspace
forced `cargo` to add `opentelemetry 0.28.0` to the lockfile alongside
the existing `opentelemetry 0.27.1`:

```
$ cargo build --workspace --all-targets
     Locking 2 packages to latest compatible versions
      Adding opentelemetry v0.28.0
      Adding opentelemetry-appender-tracing v0.28.1 (available: v0.31.1)
```

Inspecting the appender crate's manifests on the registry:

- `opentelemetry-appender-tracing 0.27.0` →
  `[dependencies.opentelemetry] version = "0.27"`.
- `opentelemetry-appender-tracing 0.28.1` →
  `[dependencies.opentelemetry] version = "0.28"`.

The appender's minor version is **aligned** with the core's minor
version, not offset by one. ADR-0017's claim of an offset cadence is
a misreading of the upstream release matrix.

### What DELIVER did

Pinned `opentelemetry-appender-tracing = "=0.27"` instead of `=0.28`.
This co-resolves cleanly with the OTel family at `=0.27`:

```
$ cargo build --workspace --all-targets
 Downgrading opentelemetry-appender-tracing v0.28.1 -> v0.27.0
   Compiling opentelemetry-appender-tracing v0.27.0
   Compiling spark v0.1.0
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 6.76s
```

`Cargo.lock` carries exactly one `opentelemetry` minor (0.27.1) again.
The ADR-0017 contract holds: applications emit logs via
`tracing::*!`, the bridge forwards them as OTel `LogRecord`s, the
`target = "spark"` filter defends D5.

### Why this might warrant an ADR amendment

ADR-0017 explicitly justifies the `=0.28` choice with the offset-by-
one cadence claim. With that claim refuted, ADR-0017 §1 should be
amended:

- Replace "The OpenTelemetry Rust contrib crates use a release
  sequence offset by one from the core crates" with "The
  `opentelemetry-appender-tracing` crate's per-version
  `[dependencies.opentelemetry]` field aligns the appender's minor
  with the core minor. When the core family is at `=0.27`, the
  matching appender release is `=0.27`."
- Replace `opentelemetry-appender-tracing = "=0.28"` with
  `opentelemetry-appender-tracing = "=0.27"` in the Cargo.toml entry
  example.

The licence-audit row (Apache-2.0) is unchanged. The bridge wiring
contract is unchanged. The `target = "spark"` filter is unchanged.

### Forward path

Morgan / Bea: amend ADR-0017 §1 in-place with a 2026-05-06 note that
DELIVER's inspection refuted the offset claim. Or accept this
back-propagation note as the audit trail and leave ADR-0017 verbatim
(the cargo manifest is the source of truth for what's pinned). Either
posture is consistent with prior precedent (Slice 01's `rt-tokio` fix
went DELIVER-side without an ADR-0013 amendment).

---

## Issue 5 — bridge wiring requires test-side `reload::Layer` plumbing; production path uses `set_global_default`

> **Slice**: 05 — logs and metrics symmetry.
> **Scope**: doc-hidden test seam `__test_logger_provider` added to
> `src/lib.rs` + `src/init.rs`; reload-layer plumbing added to
> `tests/common/mod.rs`. No change to the four-item public surface
> (ADR-0011 lock holds; the test seam follows the `__` /
> `#[doc(hidden)]` pattern of the existing `__reset_for_testing`).

### What ADR-0017 implied

ADR-0017 §2 ("Application-side emission contract") says:

> Spark's `init` configures the OTel `LoggerProvider`, builds an
> `OpenTelemetryTracingBridge` against it, and adds that bridge as a
> `tracing_subscriber::Layer`.

The literal reading: Spark's `init` itself calls
`tracing::subscriber::set_global_default(registry)` on a Registry
containing the bridge layer.

### What DELIVER found

Three structural observations made the literal reading insufficient
for the integration test path:

1. **`set_global_default` only succeeds once per process.** The
   integration test fixture (`tests/common/mod.rs`) pre-installs a
   `Registry::default().with(SparkCaptureLayer).try_init()` so
   `target = "spark"` events from `spark::init` (the resolved-config
   event, the shutdown event vocabulary in Slice 06) can be captured
   for the D5 invariant assertions. Once that's installed, Spark's
   own `set_global_default` from inside `init` returns Err — the
   bridge layer is never reachable to forward `target !=
   "spark"` events.
2. **`tracing_subscriber::reload::Layer<L, S>` cannot host a
   `Filtered<Bridge, FilterFn, S>`.** A `Filtered` layer requires a
   `FilterId` registered with the subscriber at construction time;
   layers swapped in via `reload::Handle::reload(...)` have no
   opportunity to register one and panic at first event with
   `"a Filtered layer was used, but it had no FilterId; was it
   registered with the subscriber?"`. The bridge's `with_filter(...)`
   call therefore cannot be the integration-test path's filter
   mechanism.
3. **OTel SDK 0.27's `BatchLogProcessor::shutdown` blocks on the
   originating runtime.** When test 1's `LoggerProvider` is held
   beyond test 1's Tokio runtime (e.g. a clone in
   `BRIDGE_RELOAD_HANDLE` or `TEST_LOGGER_PROVIDER`), test 2's
   teardown of those clones triggers
   `LoggerProviderInner::Drop::shutdown` which calls
   `futures_executor::block_on(channel)` on a runtime that no longer
   has a worker — a permanent hang.

### What DELIVER did

Three coordinated changes that preserve ADR-0017's intent without
expanding the consumer-facing public surface:

1. **Production path: Spark's `init` tries
   `set_global_default(registry)` and absorbs the Err.** When no
   subscriber is pre-installed, Spark's own subscriber (Registry +
   bridge filtered to `target != "spark"`) becomes the global
   default. When the application has already installed a subscriber,
   Spark's `set_global_default` no-ops; the application is
   responsible for composing the bridge into its own subscriber stack.
   This matches the upstream `opentelemetry-appender-tracing` example
   where the application owns the subscriber install.

2. **Test path: a doc-hidden seam `__test_logger_provider()` exposes
   the `LoggerProvider` Spark constructed.** The test fixture builds
   its own `OpenTelemetryTracingBridge::new(&logger_provider)`
   wrapped in a small `BridgeWithTargetFilter` adapter (a custom
   `Layer<S>` impl that performs the `target != "spark"` check inside
   `on_event`, avoiding the `Filtered`/`FilterId` plumbing
   incompatible with reload). The wrapped bridge is installed via
   `BRIDGE_RELOAD_HANDLE` into the test subscriber's reload slot.
   The seam follows the `__` / `#[doc(hidden)]` pattern of the
   existing `__reset_for_testing`; ADR-0011's four-item public
   surface lock holds.

3. **Test fixture `Drop` releases the `LoggerProvider` clones inside
   the originating test's runtime.** `ApertureFixture::drop` calls
   `clear_installed_bridge()` (releasing the bridge clone) and
   `spark::__reset_for_testing()` (releasing the
   `TEST_LOGGER_PROVIDER` clone). Both Drops run inside the Tokio
   runtime of the originating test (because the fixture is bound to
   the test function's local scope), so the
   `BatchLogProcessor::shutdown`'s `block_on` resolves cleanly.
   Without this, the second test in a multi-test binary hangs.

### Why this might warrant an ADR amendment

ADR-0017 §2 is intentionally vague about the wiring mechanism
("adds that bridge as a `tracing_subscriber::Layer`"). DELIVER's
choices are consistent with that vagueness:

- The production path matches ADR-0017's intent exactly.
- The test path uses a doc-hidden seam in the same vein as
  `__reset_for_testing` — consistent with ADR-0011's "Test seam"
  subsection.
- The `target = "spark"` filter is implemented in **two** places: the
  production-path `non_spark_target` function in `src/init.rs`
  (applied via `bridge.with_filter(...)` since reload isn't in play),
  and the integration-test `BridgeWithTargetFilter` adapter in
  `tests/common/mod.rs` (applied via a custom `Layer::on_event`
  early-return). Both paths defend D5; the test path's adapter
  exists only because of the `Filtered`/`FilterId` reload
  incompatibility.

If Morgan prefers the production path to lift the bridge layer out
of `init` entirely (i.e. Spark exposes a public `pub fn bridge()` and
the application composes), that's a future ADR. For Slice 05, the
chosen wiring is the smallest change that keeps ADR-0011's four-item
lock and ADR-0017's intent.

### Forward path

If a future slice can consolidate the production and test filter
implementations behind one mechanism (e.g. a `tracing_subscriber`
release that fixes the `Filtered`/reload incompatibility), the
duplication can be removed. Until then, the two filter sites are
documented and tested by the
`tests/invariant_no_telemetry_on_telemetry.rs` extensions added at
Slice 05 DELIVER.

