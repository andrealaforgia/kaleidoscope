# Slice 06 — Bounded flush deadline

> **Wave**: DISCUSS — Phase 2.5.
> **Companion stories**: US-SP-06.
> **Companion slice files**: depends on Slice 05 (and on every prior slice via the SparkGuard).

## Outcome added

`SparkGuard::Drop` becomes bounded and observable. On clean flush within the configured deadline (default 5 s): one `tracing::info!(target: "spark", "shutdown complete drained=N")` event. On deadline expiry with records still in the OTel SDK's batch processor: one `tracing::warn!(target: "spark", "flush deadline exceeded dropped=M flush_timeout_ms=...")` event. At v0 with `opentelemetry_sdk =0.27`, both `N` and `M` report the literal `unknown` because the SDK does not expose those counters publicly (DESIGN ADR-0014 §2). On a downed downstream (Aperture forcibly killed): the drop does NOT panic; one tracing event describes the outcome; the drop completes within the deadline. Configurable via `SparkConfig::with_flush_timeout(Duration)`. After this slice, no application exit drops in-flight data silently.

## What it lights up (across the five backbone activities)

| Activity | Slice 06 coverage |
|---|---|
| Configure | New builder method: `.with_flush_timeout(Duration::from_secs(N))`. |
| Lint | Reused — `with_flush_timeout` accepts any `Duration`; v0 does not lint duration values. |
| Initialise SDK | Reused. The flush_timeout is stored on the `SparkGuard` for use at Drop. |
| Emit telemetry | Reused — emission paths are unchanged; the slice is about exit-time behaviour. |
| Shutdown / flush | The full bounded-flush logic: `force_flush` per provider with the deadline; INFO on clean, WARN on deadline-exceeded; no panic on downed-downstream. |

## Demo command

```bash
# Run the flush-deadline integration test.
cargo test -p spark --test slice_06_flush_deadline

# Expected: the test passes.
# Expected: the test runs three sub-cases:
#   Case A (clean flush within deadline):
#     SparkConfig with default flush_timeout (5 s).
#     Aperture is healthy. 7 spans recorded.
#     Drop -> RecordingSink eventually receives all 7 spans.
#     -> tracing INFO event with target="spark" and message containing "shutdown complete drained=unknown" captured (v0 SDK does not expose the count).
#     -> drop completes well within 5 s.
#
#   Case B (deadline-exceeded with slow downstream):
#     SparkConfig with .with_flush_timeout(Duration::from_millis(500)).
#     Aperture is configured to delay every accept by 10 s.
#     3 spans recorded.
#     Drop -> deadline expires; spans remain in the batch processor.
#     -> tracing WARN event with target="spark" and message containing "flush deadline exceeded" captured.
#     -> WARN event names the dropped count.
#     -> drop completes within ~500 ms (no indefinite block).
#
#   Case C (downed downstream, no panic):
#     SparkConfig pointed at an Aperture instance that has been forcibly killed.
#     3 spans recorded.
#     Drop -> the OTel exporter cannot send; deadline likely exceeded; some tracing event describes the outcome.
#     -> drop does NOT panic.
#     -> drop completes within the configured flush_timeout_ms.
#
# A fourth case (drop(guard) is equivalent to scope-exit drop) is exercised
# in a unit test against a fixture that does not require a real Aperture.
```

## Acceptance summary

- `SparkConfig::with_flush_timeout(Duration)` sets the deadline (default 5 s).
- `SparkGuard::Drop` calls `force_flush` synchronously on `TracerProvider`, `LoggerProvider`, `MeterProvider`.
- The total drop time is bounded by `flush_timeout_ms`; no `Drop` blocks indefinitely.
- On clean flush: a single `tracing::info!(target: "spark")` event with message containing `"shutdown complete drained=N"` is emitted, where `N` is the SDK-exposed drained count if available; v0 with `opentelemetry_sdk =0.27` reports `drained=unknown`.
- On deadline: a single `tracing::warn!(target: "spark")` event with message containing `"flush deadline exceeded dropped=M"` and the configured `flush_timeout_ms` is emitted, with the same `=N`/`=unknown` convention.
- `Drop` does not panic on a downed downstream, does not call `process::exit`, does not return early without writing the appropriate event.
- Calling `drop(guard)` explicitly produces the same observable outcome as letting the guard drop at scope exit.
- A second drop on the same guard is a no-op (the guard's internal state is consumed on first drop).

## Complexity drivers

- The OTel SDK's `force_flush` API is per-provider; Spark v0 calls it three times in sequence. The deadline must be divided across providers; DESIGN-wave decision is whether to give each provider `flush_timeout_ms / 3` or to track a remaining-time budget across the three calls. DISCUSS-locked: the *total* drop time is bounded; the per-provider mechanism is DESIGN's call.
- Drained-vs-dropped count derivation: DESIGN ADR-0014 §2 confirmed the OTel SDK at the family-pinned `=0.27` does NOT expose those counts publicly. v0 emits `drained=unknown` / `dropped=unknown`. A future SDK release that exposes the counts can switch to the integer without breaking the v0 vocabulary contract: the `drained=` / `dropped=` prefix is the contract, the value is informational. DISCUSS contract updated 2026-05-06 (see `user-stories.md > Changed Assumptions`).
- The down-downstream test requires forcibly killing an Aperture instance mid-test, which is fiddly. The DEVOPS workflow YAML may need to skip this case under some CI configurations; the unit-test version (an OTel SDK exporter pointed at a port nothing is listening on) is the contract proxy.

## Settled by DESIGN

- The OTel SDK at `=0.27` does NOT expose drained/dropped counts; v0 reports the literal `unknown`. ADR-0014 §2.
- The per-provider flush is sequential, with a shared remaining-time budget across the three providers; the total drop time is bounded. ADR-0014 §1.

## Out of scope for this slice

- Auto-instrumentation hooks at the Drop boundary (post-v0).
- Persistent buffering of un-flushed records to disk (Sluice's domain in Phase 7).
- Retry of failed exports during Drop (the OTel SDK's exporter handles retries during normal operation; Drop's deadline is the final bound, not a retry budget).
