# AC Coverage — claims-honesty-pass-v0 (DISTILL)

Each AC / slice mapped to its observable and the test that pins it, marked
RED-`#[ignore]`d (until DELIVER) vs GREEN (today).

## Legend

- **RED** = `#[ignore = "RED until DELIVER: claims-honesty-pass-v0"]`. FAILS
  today (the false string is still present); DELIVER removes the `#[ignore]`
  after applying the matching prose correction. COMPILES (RED-not-BROKEN).
- **GREEN** = passes today; NOT ignored; guards against regression.

## US-01 — README codename honesty

| AC / scenario | Observable | Test (file::fn) | State |
|---|---|---|---|
| Spark row names manual init, not auto-instrumentation | `README.md`: "Auto-instrumentation SDKs" ABSENT, "manual-init OTel SDK wrapper" PRESENT | `harness/tests/slice_08::us01_readme_spark_row_names_manual_init_not_auto_instrumentation` | RED |
| Strata row + cost line name passive profile storage | `README.md`: "Continuous profiling" (row) + "Continuous profiling as a top-tier add-on" (cost) ABSENT, "profile storage" PRESENT | `harness/tests/slice_08::us01_readme_strata_row_and_cost_line_name_passive_profile_storage` | RED |
| Cinder row names local tier metadata | `README.md`: "cold-tier coordinator" ABSENT, "tier-metadata" PRESENT | `harness/tests/slice_08::us01_readme_cinder_row_names_local_tier_metadata_not_cold_tier_coordinator` | RED |
| Loom row names TOML change control | `README.md`: "Dashboards-as-code, alert-rules-as-code" (row) ABSENT, "change control" PRESENT | `harness/tests/slice_08::us01_readme_loom_row_names_toml_change_control_not_dashboards_as_code` | RED |

## US-02 — codex stub-declaration honesty

| AC / scenario | Observable | Test (file::fn) | State |
|---|---|---|---|
| Cargo.toml no longer declares a stub | `codex/Cargo.toml`: "DISTILL-state stub" + "panics with `unimplemented!()`" ABSENT, "delivered" PRESENT | `codex/tests/slice_06::us02_codex_cargo_toml_no_longer_declares_a_stub` | RED |
| Slice headers describe live behaviour | 5x `codex/tests/slice_0*.rs`: "Tests panic on `unimplemented!()` until DELIVER" ABSENT | `codex/tests/slice_06::us02_codex_slice_headers_no_longer_claim_unimplemented_panic` | RED |
| common/mod.rs describes live helpers | `codex/tests/common/mod.rs`: "panics with `unimplemented!()` until DELIVER" ABSENT | `codex/tests/slice_06::us02_codex_common_mod_no_longer_claims_unimplemented_panic` | RED |
| No genuinely-RED codex test altered | codex suite carries no active `#[ignore]` (stale-over-green precondition) | `codex/tests/slice_06::codex_test_suite_carries_no_active_unimplemented_or_ignore` | GREEN |

## US-03 — stale `__SCAFFOLD__`-over-green doc comments (BIDIRECTIONAL)

| AC / scenario | Observable | Test (file::fn) | State |
|---|---|---|---|
| query-http-common module doc no longer claims scaffold | `query-http-common/src/lib.rs`: "DISTILL scaffold — DELIVER fills the bodies" + `unimplemented!("__SCAFFOLD__ query-http-common-v0 RED")` ABSENT | `query-http-common/tests/slice_01::us03_module_doc_no_longer_claims_unimplemented_scaffold` | RED |
| (query-http-common stale-over-green precondition) | per-fn "DELIVER state: implemented" note PRESENT today | `query-http-common/tests/slice_01::per_function_implemented_notes_are_present_today` | GREEN |
| trace-query-api handler doc no longer claims scaffold | `trace-query-api/src/lib.rs`: "Scaffold for DISTILL Mandate 7 RED-not-BROKEN: the handler is" ABSENT, "get_trace" PRESENT | `trace-query-api/tests/slice_04::us03_handler_doc_no_longer_claims_unimplemented_scaffold` | RED |
| (trace-query-api stale-over-green precondition) | live `async fn handle_traces_by_id` + `fn parse_trace_id` PRESENT today | `trace-query-api/tests/slice_04::live_handler_body_is_present_today` | GREEN |
| **In-flight markers REMAIN PRESENT (over-reach guard)** | `log-query-api/slice_05_body_regex` `__SCAFFOLD__ log-body-regex-search-v0 RED`; lumen/ray crash-durability `RED-not-BROKEN`/`#[ignore]`; aperture `slice_09_tls_config_reject` `#[ignore]`; log-query `slice_06_pagination` `__SCAFFOLD__` — all PRESENT | `harness/tests/slice_08::us03_in_flight_scaffold_markers_remain_present` | GREEN |

## US-04 — harness validation-depth honesty

| AC / scenario | Observable | Test (file::fn) | State |
|---|---|---|---|
| Harness describes structural decode-level, not wire-spec | harness `lib.rs`/`README.md`/`Cargo.toml`: "OpenTelemetry OTLP wire specification" ABSENT, "structural decode-level" PRESENT | `harness/tests/slice_08::us04_harness_describes_structural_decode_not_wire_spec_conformance` | RED |
| Harness README status reflects green code | harness `README.md`: "Implementation is intentionally absent at this point" + `validate_*` returns `unimplemented!()` ABSENT | `harness/tests/slice_08::us04_harness_readme_status_reflects_delivered_green_code` | RED |
| Structurally-valid, semantically-invalid body is accepted | `validate_traces` ACCEPTS a 4-byte-`trace_id` body; the id round-trips untouched (no semantic length check ran) | `harness/tests/slice_09::structurally_valid_semantically_bogus_trace_id_is_accepted` | GREEN |

## US-05 — query-api `step` honesty (DOCUMENT; ADR-0062)

| AC / scenario | Observable | Test (file::fn) | State |
|---|---|---|---|
| README no longer implies a Prometheus stepped grid | `README.md`: "Prometheus-compatible `/api/v1/query_range` HTTP endpoint over the durable" ABSENT, "raw"+"step" PRESENT | `harness/tests/slice_08::us05_readme_query_range_no_longer_implies_a_prometheus_stepped_grid` | RED |
| Two `step` values + omitted `step` return identical output (INVARIANCE) | `query_api::router` over a fixed window: `step=15s`, `step=60s`, and no-`step` all return byte-identical JSON | `query-api/tests/slice_06::step_is_not_honoured_two_step_values_and_omitted_step_return_identical_output` | GREEN |

> ADR-0062 note: a FUTURE stepped-grid feature will INTENTIONALLY retire the
> invariance assertion (two `step` values would then differ). That feature's
> DISTILL/DELIVER must delete this test deliberately — a then-failing assertion
> is PLANNED, not a regression. The test carries this note inline.

## US-06 — harness `GrpcProtobuf` framing honesty (DOCUMENT)

| AC / scenario | Observable | Test (file::fn) | State |
|---|---|---|---|
| Harness docs flag `GrpcProtobuf` as a non-behavioural label | harness `lib.rs`/`README.md`: "length prefix" PRESENT (caller strips it; framing inert) | `harness/tests/slice_08::us06_harness_docs_flag_grpc_framing_as_a_non_behavioural_label` | RED |
| Prefix-stripped bytes validate identically under both framings | `validate_logs` accepts under both `HttpProtobuf` and `GrpcProtobuf`; decoded payloads byte-identical | `harness/tests/slice_09::prefix_stripped_bytes_validate_identically_under_both_framings` | GREEN |
| Length-prefixed body under `GrpcProtobuf` fails to decode | `validate_logs` on a 5-byte-gRPC-framed body returns `Rule::WireType(ProtobufDecode)` | `harness/tests/slice_09::length_prefixed_body_under_grpc_framing_fails_to_decode` | GREEN |

## Coverage tally

| Story | RED-`#[ignore]`d doc guards | GREEN tests | Total |
|---|---|---|---|
| US-01 | 4 | 0 | 4 |
| US-02 | 3 | 1 | 4 |
| US-03 | 2 | 3 | 5 |
| US-04 | 2 | 1 | 3 |
| US-05 | 1 | 1 | 2 |
| US-06 | 1 | 2 | 3 |
| **Total** | **13** | **8** | **21** |

Every US-01..US-06 story has at least one scenario. The two flag slices
(US-05, US-06) each carry the doc-guard PLUS the behaviour assertion DESIGN
specified. The US-03 bidirectional guard is expressed in both directions.

## Verification at the DISTILL commit

`cargo test --workspace --all-targets --locked` stays GREEN: the 13 doc guards
are `#[ignore]`d (RED-not-BROKEN), the 8 behaviour/guardrail tests pass. See
io-strategy.md for the run evidence.
