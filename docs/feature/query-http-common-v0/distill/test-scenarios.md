# Test Scenarios — query-http-common-v0

Author: `@nw-acceptance-designer` (Scholar), DISTILL wave, 2026-05-27.

Inline unit tests in `crates/query-http-common/src/lib.rs` under
`#[cfg(test)] mod tests`. Each row maps to ONE test function, ONE
behaviour, ONE public-API artefact. No BDD `*.feature` files exist for
this feature: the public API is library-only, the "acceptance test"
surface IS the public API surface, and inline unit tests are the right
grain (DISTILL Decision D1).

## Test catalogue

### Tier 1 — data-only (GREEN at DISTILL close)

| # | Test | US | Artefact under test | Expected behaviour | State |
|---|------|----|---------------------|--------------------|-------|
| 1 | `test_max_window_seconds_value` | US-01 | `MAX_WINDOW_SECONDS` | the literal value is `86_400` (ADR-0050 Decision 1) | GREEN |
| 2 | `test_max_result_rows_value` | US-01 | `MAX_RESULT_ROWS` | the literal value is `100_000` (ADR-0050 Decision 2) | GREEN |
| 3 | `test_reason_constants_match_callsite_texts` | US-02, US-03, US-04 | four `REASON_*` consts | the four literal strings match byte-for-byte the strings today emitted by the three consumer crates | GREEN |
| 4 | `test_reason_constants_never_contain_a_credential_marker` | US-03 | four `REASON_*` consts | redaction symmetry: no const contains `SECRET` or `Bearer` | GREEN |
| 5 | `test_error_body_serialises_to_expected_json` | US-03 | `ErrorBody` `Serialize` | `serde_json::to_string(&ErrorBody{ status:"error", error:REASON_WINDOW_TOO_LARGE })` is the byte-for-byte `{"status":"error","error":"window exceeds 86400 seconds"}` | GREEN |
| 6 | `test_error_body_field_order_is_status_then_error` | US-03 | `ErrorBody` field order | the `status` field precedes `error` in the JSON envelope (mutation-kill posture) | GREEN |
| 7 | `test_time_range_is_constructible_with_start_le_end` | US-02 | `TimeRange` struct | a valid `start_secs <= end_secs` pair constructs and exposes the public fields | GREEN |

Subtotal: 7 GREEN tests at DISTILL close. These run on the workspace
pre-commit hook and on the per-crate CI job; they exercise the DATA
side of the public API surface and are the Mandate 7 ground truth.

### Tier 2 — function bodies (RED via `#[ignore]` at DISTILL close)

| # | Test | US | Function under test | Expected behaviour | State |
|---|------|----|---------------------|--------------------|-------|
| 8 | `test_parse_time_range_accepts_valid_integer_bounds` | US-02 | `parse_time_range` | `("100", "200")` returns `TimeRange{ start_secs:100, end_secs:200 }` | RED-via-ignore |
| 9 | `test_parse_time_range_accepts_equal_bounds_as_empty_range` | US-02 | `parse_time_range` | `("100", "100")` is the valid empty half-open range, NOT an inverted-bounds rejection | RED-via-ignore |
| 10 | `test_parse_time_range_accepts_zero_as_lower_bound` | US-02 | `parse_time_range` | `("0", "100")` is valid (zero is a non-negative lower bound) | RED-via-ignore |
| 11 | `test_parse_time_range_truncates_fractional_seconds` | US-02 | `parse_time_range` | `("100.5", "200.9")` truncates to `(100, 200)` (Prism's float emission) | RED-via-ignore |
| 12 | `test_parse_time_range_rejects_non_numeric_start` | US-02 | `parse_time_range` | `("notanumber", "100")` returns `Err` | RED-via-ignore |
| 13 | `test_parse_time_range_rejects_non_numeric_end` | US-02 | `parse_time_range` | `("100", "later")` returns `Err` | RED-via-ignore |
| 14 | `test_parse_time_range_rejects_negative_bounds` | US-02 | `parse_time_range` | `("-1", "100")` returns `Err` (negative is out of range) | RED-via-ignore |
| 15 | `test_parse_time_range_rejects_inverted_bounds_with_named_reason` | US-02 | `parse_time_range` | `("200", "100")` returns `Err(REASON_INVALID_TIME_RANGE)` byte-for-byte | RED-via-ignore |
| 16 | `test_parse_time_range_error_never_echoes_raw_value` | US-02 | `parse_time_range` | the error reason for `("secretvalue", "100")` does NOT contain `secretvalue` (redaction symmetry) | RED-via-ignore |
| 17 | `test_resolve_tenant_or_refuse_returns_some_tenant_unchanged` | US-04 | `resolve_tenant_or_refuse` | `Some(TenantId("acme"))` returns `Ok(&t)`; the borrowed tenant value is unchanged | RED-via-ignore |
| 18 | `test_resolve_tenant_or_refuse_refuses_none_with_401` | US-04 | `resolve_tenant_or_refuse` | `None` returns `Err(_)`; the `Err` branch is reached (status is verified once Crafty wires axum body extraction) | RED-via-ignore |
| 19 | `test_resolve_tenant_or_refuse_uses_service_label_in_reason` | US-04 | `resolve_tenant_or_refuse` | `(None, "the trace query")` returns `Err` whose body, once extracted, contains `"the trace query service refuses unscoped requests"` | RED-via-ignore |
| 20 | `test_error_response_returns_given_status_code` | US-03 | `error_response` | `(BAD_REQUEST, REASON_WINDOW_TOO_LARGE)` returns a `Response` whose status is `400` | RED-via-ignore |
| 21 | `test_error_response_body_is_byte_identical_json_envelope` | US-03 | `error_response` | the response body extracts byte-for-byte to `{"status":"error","error":"window exceeds 86400 seconds"}` | RED-via-ignore |
| 22 | `test_error_response_content_type_is_application_json` | US-03 | `error_response` | the response `Content-Type` header starts with `application/json` | RED-via-ignore |
| 23 | `test_error_response_carries_unauthorized_status` | US-03, US-04 | `error_response` | `(UNAUTHORIZED, REASON_MISSING_TENANT)` returns a `Response` whose status is `401` (mutant-kill on hard-coded 400) | RED-via-ignore |
| 24 | `test_error_response_carries_internal_server_error_status` | US-03 | `error_response` | `(INTERNAL_SERVER_ERROR, "any reason")` returns a `Response` whose status is `500` (mutant-kill on hard-coded 400) | RED-via-ignore |

Subtotal: 17 RED-via-ignore tests. Each test compiles, identifies a
single observable behaviour, and is unblocked one-at-a-time by Crafty
in DELIVER as he implements each function body per the Mikado plan
(Steps C and D land the helpers; Step E-G threads them into the three
consumer crates).

## Public API → test count

| Public API artefact | Kind | Tests | Total |
|---------------------|------|-------|-------|
| `MAX_WINDOW_SECONDS` | `pub const u64` | #1 | 1 |
| `MAX_RESULT_ROWS` | `pub const usize` | #2 | 1 |
| `REASON_INVALID_TIME_RANGE` | `pub const &str` | #3, #4 (subset) | shared |
| `REASON_WINDOW_TOO_LARGE` | `pub const &str` | #3, #4, #5 | shared |
| `REASON_TOO_MANY_ROWS` | `pub const &str` | #3, #4 | shared |
| `REASON_MISSING_TENANT` | `pub const &str` | #3, #4, #23 | shared |
| `ErrorBody` | `pub struct + Serialize` | #5, #6 | 2 |
| `TimeRange` | `pub struct` | #7 | 1 (plus #8-#16 via the parser) |
| `parse_time_range` | `pub fn` | #8, #9, #10, #11, #12, #13, #14, #15, #16 | 9 |
| `resolve_tenant_or_refuse` | `pub fn` | #17, #18, #19 | 3 |
| `error_response` | `pub fn` | #20, #21, #22, #23, #24 | 5 |

24 inline tests total. 7 GREEN, 17 RED-via-ignore.

## Self-review checklist — Mandate 7 RED-not-BROKEN

- [x] **RED-not-BROKEN**: every `#[ignore]`'d test compiles; the
  workspace pre-commit gate (`cargo test --workspace` without
  `--include-ignored`) passes. The `unimplemented!("__SCAFFOLD__
  query-http-common-v0 RED")` panic message is the explicit RED
  marker; the `#[ignore]` reason string carries the explicit
  Mikado-step pointer (`"DELIVER step C — __SCAFFOLD__"` or
  `"DELIVER step D — __SCAFFOLD__"`).
- [x] **Every public API artefact has at least one test**: see the
  Public API → test count table. Six of the nine artefacts are
  exercised by GREEN data-only tests; three are exercised by
  RED-via-ignore function-body tests (one per function).
- [x] **Data fixtures are PRECONDITIONS, never EXPECTED OUTPUTS**: the
  fixture values in each test (`"100"`, `"200"`, `"acme"`, `"the
  trace query"`, `REASON_WINDOW_TOO_LARGE`) are INPUTS to the function
  under test. The assertions verify the function's OUTPUT or the
  observable property of its return value. No fixture computes the
  expected output.
- [x] **K2 byte-identity strategy is described**: the K2 acceptance
  gate is enforced by Crafty in DELIVER step E-G via the existing
  acceptance suites of the three consumer crates (recorded in DISTILL
  D6); the gate snapshot is `cargo test -p query-api -p log-query-api
  -p trace-query-api` pre- and post-rewire, with a byte-diff on every
  400/401 scenario's response body.
- [x] **Mandate 1 Hexagonal Boundary**: there is no driving port in
  this crate (it is a library, not a service). The "boundary" is the
  six-item public API surface, and every test invokes through it. No
  test reaches into private helpers (the future `parse_epoch_seconds`
  Crafty will add as a private helper of `parse_time_range` is NOT
  tested directly; its behaviour is exercised through the public
  parser only).
- [x] **Mandate 2 Business Language**: not applicable in the strict
  Gherkin sense (these are inline unit tests, not BDD scenarios), but
  the test names DO speak the maintainer's vocabulary
  (`accepts_equal_bounds_as_empty_range`, `refuses_none_with_401`,
  `uses_service_label_in_reason`). HTTP terms (`StatusCode`,
  `Response`) appear in the tests because the public API itself is
  HTTP-shaped (it returns an `axum::Response`); this is the correct
  vocabulary for a library that wraps `axum`.
- [x] **Mandate 3 User Journey Completeness**: the
  maintainer's job-to-be-done ("change a read-side HTTP scaffolding
  element in one place and trust the change to propagate") is
  exercised across the 24 tests: the maintainer who edits a cap value
  sees a focused test fail; the maintainer who edits a reason text
  sees the byte-equality test fail; the maintainer who edits the
  envelope sees the serialisation test fail. Each test is one row in
  the maintainer's "if I change X, what catches me?" matrix.
- [x] **Mandate 4 Pure Function Extraction**: the crate IS the pure
  function extraction. `parse_time_range` is a pure function over
  `(&str, &str) -> Result<TimeRange, &'static str>`. `error_response`
  and `resolve_tenant_or_refuse` are thin wrappers over the `axum`
  `IntoResponse` mechanism; they have no side effects of their own.
  No `tmp_path`, no `subprocess`, no fixture parametrisation needed.

## Notes for DELIVER (Crafty)

- The 17 `#[ignore]`'d tests are intended to be un-ignored ONE AT A
  TIME. The order is: Step C (the five `test_error_response_*` tests
  first, because the helper is the foundation), then Step D (the
  nine `test_parse_time_range_*` tests and the three
  `test_resolve_tenant_or_refuse_*` tests, in any order).
- Three `test_resolve_tenant_or_refuse_*` tests and two
  `test_error_response_*` tests have body-byte assertion comments
  noting that DELIVER threads the `axum::body::to_bytes` async
  machinery to extract the response body. The test signatures are
  ready; Crafty fills in the body-extraction lines as part of the
  RED-to-GREEN transition.
- The K4 mutation gate (`cargo mutants -p query-http-common
  --no-shuffle`) runs after Step H. Any mutant survivor is killed by
  adding a focused test inside `#[cfg(test)] mod tests`; the test
  catalogue above is the K4 ground truth.
