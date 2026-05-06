# Test mapping — `spark` v0 (DISTILL)

> **Wave**: DISTILL.
> **Author**: Atlas (`nw-acceptance-designer`).
> **Date**: 2026-05-06.
> **Companion documents**: `wave-decisions.md`, `back-propagation.md`,
> `../discuss/journey-spark.feature`,
> `../discuss/user-stories.md`,
> `../design/slice-mapping.md`,
> `crates/spark/tests/`.

This file is the per-slice mapping the brief asks for: every BDD
scenario in `journey-spark.feature` plus every UAT scenario in
`user-stories.md` is mapped to a test binary, a `#[test]` function
name, and the public-API touchpoint the test asserts.

---

## Story coverage matrix (Sentinel Dim 8 Check A)

Every user story has at least one `#[test]` function:

| Story | Slice | Test binary | Test count | Coverage |
|---|---|---|---|---|
| US-SP-01 — Initialise Spark and round-trip a span | 01 | `slice_01_walking_skeleton.rs` | 7 | ✓ |
| US-SP-02 — Refuse missing required attrs at init time | 02 | `slice_02_init_error_paths.rs` + `invariant_single_init.rs` | 11 + 1 | ✓ |
| US-SP-03 — Inject all four house resource attributes | 03 | `slice_03_feature_flags_and_experiment.rs` | 10 | ✓ |
| US-SP-04 — Honour OTel-canonical env vars + precedence | 04 | `slice_04_env_var_precedence.rs` | 7 | ✓ |
| US-SP-05 — Inject house attrs on logs and metrics | 05 | `slice_05_logs_and_metrics.rs` | 5 active + 3 deferred | ✓ (deferred logs noted) |
| US-SP-06 — Flush pending exports synchronously on guard drop | 06 | `slice_06_flush_deadline.rs` | 10 | ✓ |
| Cross-cutting D5 — no telemetry on telemetry | — | `invariant_no_telemetry_on_telemetry.rs` | 3 | ✓ |
| Cross-cutting D7 — single-init invariant | — | `invariant_single_init.rs` | 1 | ✓ (own binary per ADR-0015 §3) |

**Total**: 57 `#[test]` functions across 8 binaries (54 active + 3
`#[ignore]`'d in Slice 05 pending Path A on the logs-emission
contract — see `back-propagation.md > Issue 1`).

---

## Slice 01 — Walking skeleton (US-SP-01)

Binary: `tests/slice_01_walking_skeleton.rs`. 7 `#[test]` functions.

| BDD scenario / UAT | `#[test]` function | Public-API touchpoint |
|---|---|---|
| "spark::init returns Ok(SparkGuard) for the canonical SparkConfig" | `developer_runs_init_with_canonical_config_and_receives_ok_guard` | `spark::init`, `SparkConfig::for_service`, `.require_tenant_id`, `.with_tenant_id`, `.with_endpoint`, `Result<SparkGuard, SparkError>` |
| "An ExportTraceServiceRequest emitted via `opentelemetry::global::tracer(...).in_span(...)` reaches Aperture's listener and the RecordingSink records the request" | `developer_records_one_span_and_recording_sink_captures_a_traces_export` | `spark::init`, `SparkGuard` Drop (clean flush), `opentelemetry::global::tracer(...)` |
| "The recorded request's ResourceSpans.resource.attributes contains service.name" | `developer_records_one_span_and_recording_sink_resource_includes_service_name` | (as above) + Resource composition (per ADR-0011) |
| "The Resource includes tenant.id" | `developer_records_one_span_and_recording_sink_resource_includes_tenant_id` | (as above) |
| Implicit: span count = 1 (single emission) | `developer_records_one_span_and_recording_sink_holds_exactly_one_span` | (as above) |
| "A single tracing INFO event with target=\"spark\" and message containing 'spark::init succeeded' is captured" | `developer_runs_init_and_observes_spark_init_succeeded_event_on_tracing_facade` | `spark::init` emits the INFO event via `observability::emit_init_succeeded` (locked by ADR-0011 + observability vocabulary) |
| "SparkConfig is plain data with no I/O" | `developer_builds_a_spark_config_and_emits_no_telemetry_before_init` | `SparkConfig::for_service` and builder methods (must not call `init`-time logic) |

---

## Slice 02 — Init error paths (US-SP-02 except `GlobalAlreadyInitialised`)

Binary: `tests/slice_02_init_error_paths.rs`. 11 `#[test]` functions.

| BDD scenario / UAT | `#[test]` function | Public-API touchpoint |
|---|---|---|
| "spark::init refuses missing required tenant.id" | `developer_calls_init_with_require_tenant_id_but_no_tenant_id_and_receives_missing_required_attribute_error` | `SparkError::MissingRequiredAttribute { name: "tenant.id" }` |
| Display contains "tenant.id" (substring assertion per ADR-0012) | `developer_reads_missing_tenant_id_error_display_and_finds_tenant_id_substring` | `SparkError::Display` impl |
| "spark::init refuses empty-string tenant.id" | `developer_calls_init_with_empty_string_tenant_id_and_receives_missing_required_attribute_error` | `SparkError::MissingRequiredAttribute { name: "tenant.id" }` (empty-string ≡ absence) |
| US-SP-02 AC defence-in-depth: empty service.name | `developer_calls_init_with_empty_service_name_and_receives_missing_required_attribute_error` | `SparkError::MissingRequiredAttribute { name: "service.name" }` |
| "spark::init refuses an invalid endpoint" | `developer_calls_init_with_typo_in_endpoint_scheme_and_receives_invalid_endpoint_error` | `SparkError::InvalidEndpoint { endpoint, reason }` |
| InvalidEndpoint.endpoint carries literal input | `developer_calls_init_with_invalid_endpoint_and_error_carries_literal_endpoint` | `SparkError::InvalidEndpoint.endpoint` |
| InvalidEndpoint.reason names the parse failure | `developer_calls_init_with_invalid_endpoint_and_reason_field_is_non_empty` | `SparkError::InvalidEndpoint.reason` |
| InvalidEndpoint Display contains the endpoint substring | `developer_reads_invalid_endpoint_display_and_finds_endpoint_substring` | `SparkError::Display` impl per ADR-0012 |
| "spark::init accepts a SparkConfig without require_tenant_id" | `developer_calls_init_without_require_tenant_id_and_receives_ok_without_tenant_id` | `spark::init` happy path with optional tenant.id |
| No INFO 'spark::init succeeded' event on Err paths | `developer_calls_init_with_invalid_config_and_no_init_succeeded_event_is_emitted` | `observability::emit_init_succeeded` MUST NOT fire on Err |
| No export reaches the RecordingSink on Err | `developer_calls_init_with_invalid_endpoint_and_no_export_reaches_recording_sink` | The exporter MUST NOT be constructed on Err |

---

## Slice 03 — Feature flags and experiment.id (US-SP-03)

Binary: `tests/slice_03_feature_flags_and_experiment.rs`. 10 `#[test]` functions.

| BDD scenario / UAT | `#[test]` function | Public-API touchpoint |
|---|---|---|
| "A traces export carries all four house attributes" — service.name | `developer_sets_all_four_house_attrs_and_resource_includes_service_name` | OTel Resource on traces |
| (cont.) — tenant.id | `developer_sets_all_four_house_attrs_and_resource_includes_tenant_id` | (as above) |
| (cont.) — feature_flag.checkout-v2 (with `feature_flag.` prefix) | `developer_sets_feature_flag_checkout_v2_and_resource_includes_prefixed_attribute` | `SparkConfig::with_feature_flags`, `feature_flag.{key}` namespace |
| (cont.) — experiment.id | `developer_sets_experiment_id_and_resource_includes_experiment_id_attribute` | `SparkConfig::with_experiment_id` |
| "feature_flag attributes are namespace-prefixed" — two pairs | `developer_sets_two_feature_flags_and_resource_includes_both_prefixed_attributes` | `SparkConfig::with_feature_flags` accepting `IntoIterator<Item=(K,V)>` |
| (cont.) "neither attribute appears WITHOUT the feature_flag. prefix" | `developer_sets_feature_flags_and_unprefixed_attribute_does_not_appear_on_resource` | (as above) |
| "Empty-string optional attributes are skipped" — empty experiment.id | `developer_sets_empty_experiment_id_and_resource_does_not_include_experiment_id` | Resource composition's empty-skip rule |
| "A SparkConfig without optional attributes produces a minimal Resource" — no tenant.id | `developer_uses_only_for_service_and_resource_does_not_include_tenant_id` | The opt-in tenant.id posture |
| (cont.) — no feature_flag.* | `developer_uses_only_for_service_and_resource_does_not_include_feature_flag_attribute` | (as above) |
| (cont.) — no experiment.id | `developer_uses_only_for_service_and_resource_does_not_include_experiment_id` | (as above) |

---

## Slice 04 — OTel-canonical env-var precedence (US-SP-04)

Binary: `tests/slice_04_env_var_precedence.rs`. 7 `#[test]` functions
(every test carries `#[serial]`).

| BDD scenario / UAT | `#[test]` function | Public-API touchpoint |
|---|---|---|
| "SparkConfig::with_endpoint takes precedence over OTEL_EXPORTER_OTLP_ENDPOINT" | `developer_sets_with_endpoint_explicitly_and_resolved_event_names_explicit_value` | `SparkConfig::with_endpoint` (highest precedence) + resolved-config event |
| "OTEL_EXPORTER_OTLP_ENDPOINT is honoured when SparkConfig::with_endpoint is not called" | `operator_sets_env_endpoint_and_resolved_event_names_env_value` | env-var resolution (second precedence) + resolved-config event |
| (cont.) round-trip witness — export reaches env-targeted Aperture | `operator_sets_env_endpoint_and_export_reaches_env_targeted_aperture` | (as above) + the OTLP exporter targeting the resolved value |
| "Spark defaults to http://localhost:4317 when neither config nor env var is set" | `developer_runs_init_with_no_endpoint_config_and_resolved_event_names_default_localhost` | default fallback (third precedence) + resolved-config event |
| "Resolved configuration is observable on the tracing facade" — service.name field | `developer_runs_init_and_resolved_event_carries_service_name_field` | Structured tracing event field |
| (cont.) — protocol="grpc" | `developer_runs_init_and_resolved_event_carries_protocol_grpc_field` | (as above) |
| (cont.) — flush_timeout_ms numeric field | `developer_runs_init_and_resolved_event_carries_flush_timeout_ms_field` | (as above) |

---

## Slice 05 — Logs and metrics symmetry (US-SP-05)

Binary: `tests/slice_05_logs_and_metrics.rs`. 8 `#[test]` functions:
5 active + 3 `#[ignore]`'d pending Path A on the logs-emission
contract (see `back-propagation.md > Issue 1`).

### Active tests (metrics + cross-signal symmetry)

| BDD scenario / UAT | `#[test]` function | Public-API touchpoint |
|---|---|---|
| "A metrics export carries the same four house attributes" — service.name | `developer_increments_one_counter_and_metrics_export_carries_service_name_on_resource` | `opentelemetry::global::meter(...)`; OTel Resource on metrics |
| (cont.) — tenant.id | `developer_increments_one_counter_and_metrics_export_carries_tenant_id_on_resource` | (as above) |
| (cont.) — feature_flag.checkout-v2 | `developer_increments_one_counter_and_metrics_export_carries_feature_flag_on_resource` | (as above) |
| (cont.) — experiment.id | `developer_increments_one_counter_and_metrics_export_carries_experiment_id_on_resource` | (as above) |
| "All three signals share the same Resource shape" — traces ∩ metrics symmetry | `developer_emits_trace_and_metric_and_resource_attributes_match_across_two_signals` | Cross-signal Resource composition |

### Deferred tests (Path A pending; see back-propagation.md)

| BDD scenario / UAT | `#[test]` function | Status |
|---|---|---|
| "A logs export carries service.name on Resource" | `developer_emits_one_log_record_and_logs_export_carries_service_name_on_resource` | `#[ignore]` |
| "A logs export carries tenant.id on Resource" | `developer_emits_one_log_record_and_logs_export_carries_tenant_id_on_resource` | `#[ignore]` |
| "All three signals share the same Resource shape" — extended to logs | `developer_emits_all_three_signals_and_resource_attributes_match_across_signals` | `#[ignore]` |

---

## Slice 06 — Bounded flush deadline (US-SP-06)

Binary: `tests/slice_06_flush_deadline.rs`. 10 `#[test]` functions.
Path A literal (`drained=unknown` / `dropped=unknown`) is the v0
contract per ADR-0014 §2.

| BDD scenario / UAT | `#[test]` function | Public-API touchpoint |
|---|---|---|
| Case A: clean flush emits "shutdown complete" INFO event | `developer_drops_guard_with_healthy_aperture_and_observes_shutdown_complete_info_event` | `SparkGuard::Drop` clean path + `observability::emit_shutdown_complete` |
| (cont.) the message carries the `drained=` prefix (Path A) | `developer_drops_guard_with_healthy_aperture_and_shutdown_complete_message_carries_drained_prefix` | Vocabulary prefix contract |
| (cont.) drop completes within the default flush timeout | `developer_drops_guard_with_healthy_aperture_and_drop_completes_within_default_timeout` | The bounded-flush invariant |
| Case B: deadline exceeded with unreachable endpoint | `developer_drops_guard_pointed_at_unreachable_endpoint_and_observes_flush_deadline_exceeded_event` | WARN event vocabulary |
| (cont.) the message carries the `dropped=` prefix (Path A) | `developer_drops_guard_pointed_at_unreachable_endpoint_and_warn_message_carries_dropped_prefix` | Vocabulary prefix contract |
| (cont.) the value after `dropped=` is `unknown` (v0) or an integer (future SDK) | `developer_drops_guard_pointed_at_unreachable_endpoint_and_warn_message_dropped_value_is_unknown_or_integer` | Path A literal flexibility |
| (cont.) drop completes close to the configured deadline | `developer_drops_guard_with_short_deadline_and_drop_completes_close_to_deadline` | Bounded-deadline guarantee |
| Case C: down downstream — drop does NOT panic | `developer_drops_guard_with_no_listener_at_endpoint_and_drop_does_not_panic` | Panic-safety in Drop (ADR-0014 §3) |
| Idempotent drop: exactly one shutdown_initiated event | `developer_calls_drop_explicitly_and_observes_exactly_one_shutdown_initiated_event` | `Option::take` idempotency (ADR-0014 §4) |
| "drop(guard) called explicitly is equivalent to scope-exit drop" | `developer_calls_drop_explicitly_and_observes_shutdown_complete_event_just_like_scope_exit` | (as above) + `SparkGuard::Drop` |

---

## Invariant: single init (US-SP-02 §`GlobalAlreadyInitialised`; ADR-0015 §3)

Binary: `tests/invariant_single_init.rs`. **Exactly one** `#[test]`
function per ADR-0015 §3 — the binary's process runs exactly two
`init` calls.

| BDD scenario / UAT | `#[test]` function | Public-API touchpoint |
|---|---|---|
| "spark::init refuses a second call in the same process" | `developer_calls_init_twice_in_same_process_and_second_call_returns_global_already_initialised` | `SparkError::GlobalAlreadyInitialised` |

---

## Invariant: no telemetry on telemetry (D5)

Binary: `tests/invariant_no_telemetry_on_telemetry.rs`. 3 `#[test]`
functions.

| BDD scenario / UAT | `#[test]` function | Public-API touchpoint |
|---|---|---|
| "exactly one tracing INFO event with target=\"spark\" and message containing 'spark::init succeeded' is captured" | `spark_emits_init_succeeded_event_to_tracing_facade` | The tracing-facade routing |
| "no record carries service.name=\"spark\"" | `no_export_reaches_recording_sink_with_spark_as_service_name` | Resource composition's identity invariant |
| Defence-in-depth: no `spark.*`-prefixed Resource attributes | `no_export_reaches_recording_sink_with_spark_prefixed_resource_attribute` | (as above) |

---

## Public-API touchpoint matrix (Sentinel CM-A evidence)

Every test imports only the four-item public surface. No internal
imports. Verified by:

```bash
$ grep -rn "use spark::" crates/spark/tests/
```

Returns only:

```
crates/spark/tests/slice_01_walking_skeleton.rs:36:use spark::{init, SparkConfig};
crates/spark/tests/slice_02_init_error_paths.rs:42:use spark::{init, SparkConfig, SparkError};
crates/spark/tests/slice_03_feature_flags_and_experiment.rs:34:use spark::{init, SparkConfig};
crates/spark/tests/slice_04_env_var_precedence.rs:36:use serial_test::serial;
crates/spark/tests/slice_04_env_var_precedence.rs:37:use spark::{init, SparkConfig};
crates/spark/tests/slice_05_logs_and_metrics.rs:62:use spark::{init, SparkConfig};
crates/spark/tests/slice_06_flush_deadline.rs:38:use spark::{init, SparkConfig};
crates/spark/tests/invariant_single_init.rs:31:use spark::{init, SparkConfig, SparkError};
crates/spark/tests/invariant_no_telemetry_on_telemetry.rs:42:use spark::{init, SparkConfig};
```

Every import is from the four-item public surface (`init`,
`SparkConfig`, `SparkError`, `SparkGuard`). Zero internal-module
references. CM-A holds.

---

## Build status

`cargo build --workspace --all-targets --locked` succeeds. Spark's
test binaries panic on `unimplemented!()` when run — the canonical
RED-on-day-one state. `cargo test -p spark` produces:

- 0 passing tests in `spark` (lib unit tests — none at DISTILL).
- 1 passing test in `slice_01_walking_skeleton` (the
  SparkConfig-pure-data witness, which never calls `init`).
- 0 passing tests in every other binary (every test calls `init`,
  which panics).
- 53 panicking tests reporting `not implemented: spark::init is the
  DISTILL-state stub`.
- 3 ignored tests in `slice_05_logs_and_metrics` (Path A pending).

This matches the harness/Aperture day-one posture: tests RED, build
green, contract surface stable.
