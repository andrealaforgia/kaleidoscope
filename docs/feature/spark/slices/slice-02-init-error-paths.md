# Slice 02 — Init error paths

> **Wave**: DISCUSS — Phase 2.5.
> **Companion stories**: US-SP-02.
> **Companion slice files**: depends on Slice 01.

## Outcome added

`spark::init` becomes loud at config time. Every misconfiguration matching one of the closed `SparkError` variants returns a precise diagnostic immediately, before any OTel SDK type is constructed and before any telemetry reaches the wire. The four variants land here: `MissingRequiredAttribute { name }` (for missing or empty `service.name` and missing or empty `tenant.id` when `require_tenant_id()` was called), `InvalidEndpoint { endpoint, reason }` (for unparseable URIs), `GlobalAlreadyInitialised` (for second-call), and the not-yet-triggered `ExporterInitFailed { reason }` (the variant exists but is exercised by future slices when the exporter constructor itself fails — at v0 reachable only via test-scaffolding).

## What it lights up (across the five backbone activities)

| Activity | Slice 02 coverage |
|---|---|
| Configure | The same `SparkConfig` builder as Slice 01, exercised on misconfiguration paths: missing `with_tenant_id()` after `require_tenant_id()`; empty-string `with_tenant_id("")`; invalid `with_endpoint("htp://...")`. |
| Lint | The reject paths: `MissingRequiredAttribute`, `InvalidEndpoint`, `GlobalAlreadyInitialised`. Each returns its named variant; no OTel SDK type is constructed. |
| Initialise SDK | NOT entered on the Err paths. Confirmed by absence: no `tracing` event with `target="spark"` and message containing `"spark::init succeeded"`; no `RecordingSink` capture. |
| Emit telemetry | NOT entered on the Err paths. |
| Shutdown / flush | NOT entered on the Err paths (no `SparkGuard` to drop). |

## Demo command

```bash
# Run the init-error-paths integration test.
cargo test -p spark --test slice_02_init_error_paths

# Expected: the test passes.
# Expected: each test case asserts a specific Err variant and confirms:
#   - No tracing event with target="spark" and message containing "spark::init succeeded" was captured.
#   - No ExportTraceServiceRequest reached the RecordingSink that the test plugged behind Aperture.
#
# A second demo, by hand:
# Terminal: run an example that triggers each error case in sequence.
cargo run -p spark --example trigger_init_errors

# Expected: the example prints (one per Err case):
#   "MissingRequiredAttribute name=tenant.id"
#   "MissingRequiredAttribute name=tenant.id"  (the empty-string case)
#   "InvalidEndpoint endpoint=htp://typo:4317 reason=scheme \"htp\" is not http or https"
#   "GlobalAlreadyInitialised"
#   And exits 0 (the example's job is to prove each error case is reachable, not to fail).
```

## Acceptance summary (full UAT in user-stories.md and journey-spark.feature)

- `spark::init` returns `Err(SparkError::MissingRequiredAttribute { name: "tenant.id" })` when `require_tenant_id()` was called and no `with_tenant_id` was called.
- `spark::init` returns `Err(SparkError::MissingRequiredAttribute { name: "tenant.id" })` when `with_tenant_id("")` was called.
- `spark::init` returns `Err(SparkError::InvalidEndpoint { endpoint, reason })` when the resolved endpoint cannot be parsed; the `reason` field names the parse failure.
- `spark::init` returns `Err(SparkError::GlobalAlreadyInitialised)` on the second call in the same process.
- `spark::init` returns `Ok(SparkGuard)` for a `SparkConfig::for_service` without `require_tenant_id()` (negative-case proof: tenant.id is opt-in).
- On any `Err`: no OTel SDK type was constructed; no `tracing` event with `"spark::init succeeded"` was emitted; no telemetry reached any backend.

## Complexity drivers

- First exhaustive enumeration of the `SparkError` variant set; each variant must be reachable from observable input. DESIGN-locked `#[non_exhaustive]` posture is established here.
- First test of the no-side-effects-on-Err invariant. The integration test must subscribe to the application's `tracing` facade AND plug a `RecordingSink` behind Aperture, then assert absence on both channels for each Err path.
- The `GlobalAlreadyInitialised` test requires a one-shot fixture (one process, two `init` calls). Cargo runs each `#[test]` in its own thread by default but in the same process; the fixture must coordinate around `opentelemetry::global::set_tracer_provider`'s once-per-process semantics.

## Known unknowns

- The exact mechanism for resetting the OTel global state between tests in the same process is upstream-dependent. If `opentelemetry::global::set_tracer_provider` does not support reset, Spark's `GlobalAlreadyInitialised` test must be a single test running once per process invocation, which DEVOPS handles with a `[[test]]` declaration in `Cargo.toml`. DESIGN-wave (Morgan) decides.
- Whether `InvalidEndpoint` should fire on `with_endpoint("ftp://...")` (HTTP/HTTPS-only) at v0, or whether v0 lets the OTel exporter fail with `ExporterInitFailed` instead, is a DESIGN-wave question. DISCUSS-locked: `InvalidEndpoint` fires on URI parse failure and on scheme-not-http-or-https.

## Out of scope for this slice

- `feature_flag.*` and `experiment.id` Resource attributes (Slice 03).
- `OTEL_EXPORTER_OTLP_ENDPOINT` precedence (Slice 04).
- Logs and metrics (Slice 05).
- Bounded flush (Slice 06).
- The `ExporterInitFailed` variant under realistic conditions (i.e. when the OTel exporter construction itself fails for reasons not detectable by the URI parser); v0 reaches it only via test scaffolding.
