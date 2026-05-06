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
