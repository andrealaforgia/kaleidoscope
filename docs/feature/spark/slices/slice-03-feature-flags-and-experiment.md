# Slice 03 — Feature flags and experiment.id on the Resource

> **Wave**: DISCUSS — Phase 2.5.
> **Companion stories**: US-SP-03.
> **Companion slice files**: depends on Slice 01.

## Outcome added

`SparkConfig::with_feature_flags(I)` and `SparkConfig::with_experiment_id(s)` join the four-attribute Resource composition. The OTel `Resource` produced by `spark::init` includes `feature_flag.{key}` for each non-empty pair the application set, and `experiment.id` if a non-empty value was set. Every emitted span (and, after Slice 05, every emitted log record and metric data point) carries the full four-attribute Resource: `service.name`, `tenant.id` (if set), `feature_flag.*` (one per pair), `experiment.id` (if set).

## What it lights up (across the five backbone activities)

| Activity | Slice 03 coverage |
|---|---|
| Configure | New builder methods exercised: `.with_feature_flags([("checkout-v2", "on")])` and `.with_experiment_id("exp-2026-Q2-pricing")`. |
| Lint | The lint pass remains on the same path as Slice 01 (only `service.name` and opt-in `tenant.id` are required). The new optional attrs are not lint-required. |
| Initialise SDK | Resource composition extended: one attribute per `feature_flag` entry, plus `experiment.id`. Empty-string values are skipped. |
| Emit telemetry | Traces emitted via the standard OTel API now carry all four house attrs on their Resource. (Logs and metrics symmetry arrives in Slice 05.) |
| Shutdown / flush | Reused from Slice 01. |

## Demo command

```bash
# Run the feature-flags-and-experiment integration test.
cargo test -p spark --test slice_03_feature_flags_and_experiment

# Expected: the test passes.
# Expected: the test internally:
#   1. Spawns a real Aperture with a RecordingSink.
#   2. Calls spark::init(SparkConfig::for_service("payments-api")
#         .require_tenant_id()
#         .with_tenant_id("acme-prod")
#         .with_feature_flags([("checkout-v2", "on"), ("dark-mode", "off")])
#         .with_experiment_id("exp-2026-Q2-pricing")
#         .with_endpoint(...)).
#   3. Records one span.
#   4. Drops the SparkGuard.
#   5. Asserts the RecordingSink saw an ExportTraceServiceRequest whose Resource.attributes contains:
#         service.name="payments-api"
#         tenant.id="acme-prod"
#         feature_flag.checkout-v2="on"
#         feature_flag.dark-mode="off"
#         experiment.id="exp-2026-Q2-pricing"
#
# A second demo, by hand: same as Slice 01 but the example app is
# `cargo run -p spark --example send_one_span_with_house_attrs`, which sets
# all four house attributes. Aperture's stderr shows all four on the
# sink_accepted line.
```

## Acceptance summary

- `SparkConfig::with_feature_flags(I)` accepts an iterator of (key, value) pairs.
- `SparkConfig::with_experiment_id(s)` sets a single optional attribute.
- The OTel Resource carries `feature_flag.{key}` (with the `feature_flag.` prefix) for each non-empty pair.
- The Resource carries `experiment.id` for non-empty values.
- Empty-string values for feature_flag or experiment.id are SKIPPED, not emitted as empty-string attributes.
- A `SparkConfig` with no optional house attributes (no feature_flag, no experiment.id, no tenant.id) produces a Resource containing only `service.name`.
- Every emitted span carries all four set house attrs on its Resource (the `house_attribute_completeness` invariant for traces).

## Complexity drivers

- The `feature_flag.` namespace prefix is asserted verbatim in the integration test. This locks the prefix as part of the v0 contract.
- The empty-string-skip rule is structural — it's the difference between a Resource with one feature_flag entry and a Resource with one feature_flag entry plus a feature_flag entry with empty value (which OTel SDK consumers handle differently). DESIGN-wave decision: which `opentelemetry_sdk` `KeyValue` constructor handles empty-string skipping cleanly.

## Known unknowns

- Whether the `with_feature_flags` builder method takes `IntoIterator<Item=(impl Into<String>, impl Into<String>)>` or `HashMap<String, String>` is a DESIGN-wave decision (Morgan). DISCUSS specifies the *shape* (key/value pairs); the *type* is DESIGN's call.
- Whether OTel semconv 1.27 has stabilised on `feature_flag.*` (singular) or `feature_flags.*` (plural) at the harness's pinned spec version is a Codex-Phase-0+ concern. Spark v0 uses `feature_flag.` (singular) per the Kaleidoscope architecture document and roadmap C.2.

## Out of scope for this slice

- Logs and metrics signals (Slice 05) — the Resource on logs / metrics still inherits from the same OTel SDK Resource composition Spark sets up here, but the slice that lights up the LoggerProvider and MeterProvider is Slice 05.
- Env-var precedence (Slice 04).
- Bounded flush (Slice 06).
