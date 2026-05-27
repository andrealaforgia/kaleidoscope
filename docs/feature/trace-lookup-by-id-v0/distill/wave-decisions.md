# Wave Decisions: trace-lookup-by-id-v0 — DISTILL wave

Author: `@nw-acceptance-designer` (Scholar), DISTILL wave, 2026-05-27.
Mode: execute.

This file pins the acceptance test design choices that translate the
DESIGN wave's contract growth (ADR-0053; new sibling route
`GET /api/v1/traces/by_id`) into an executable Rust integration test
suite. The slice is parse + wire inside the existing
`crates/trace-query-api`; the substrate (`ray::TraceStore::get_trace`)
is unchanged. Kaleidoscope is a Rust workspace, so the acceptance
suite is a Rust integration test file using `#[tokio::test]` +
`axum::Router::oneshot`, mirroring `slice_02_caps.rs` and the sibling
`crates/log-query-api/tests/slice_03_severity_filter.rs`.

## Inputs read (read-first checklist)

- [x] `docs/feature/trace-lookup-by-id-v0/discuss/user-stories.md` —
      five user stories (US-01 happy path, US-02 unknown trace_id 200
      `[]`, US-03 fail-closed tenancy 401, US-04 malformed trace_id
      400 with no echo, US-05 cross-tenant isolation 200 `[]`).
- [x] `docs/feature/trace-lookup-by-id-v0/discuss/story-map.md` —
      backbone, walking skeleton, scope assessment (PASS).
- [x] `docs/feature/trace-lookup-by-id-v0/design/wave-decisions.md` —
      four flags pinned (new path, 32-hex case-insensitive, uniform
      cap, ADR-0053); D5-D9 micro-decisions (router wiring, handler
      order, parse helper, no-store-call invariants, helper reuse).
- [x] `docs/feature/trace-lookup-by-id-v0/design/application-architecture.md`
      — sequence (L2), Changes Per File, Error Contract.
- [x] `docs/product/architecture/adr-0053-trace-lookup-by-id.md` —
      durable record of the contract growth.
- [x] `docs/feature/trace-lookup-by-id-v0/devops/environments.yaml` —
      single `clean` target environment (in-process via tower
      `oneshot`); no Dockerfile change.
- [x] `crates/trace-query-api/tests/slice_02_caps.rs` — direct pattern
      precedent for the test harness (`oneshot`, `BulkTraceStore`-style
      doubles, `FailingTraceStore` proof-of-no-store-call).
- [x] `crates/log-query-api/tests/slice_03_severity_filter.rs` —
      immediate stylistic sibling; the file structure (mod common,
      local `*_request_with_*` helper, `#[tokio::test]` per AC, one
      walking skeleton enabled + remainder `#[ignore]`d) is mirrored.
- [x] `crates/ray/src/store.rs` — confirmed `TraceStore::get_trace` at
      line 72; `InMemoryTraceStore::get_trace` at lines 182-192.
- [x] `crates/ray/src/span.rs` — confirmed `TraceId(pub [u8; 16])` at
      line 65; hex codec accepts both `a-f` and `A-F` (lines 42-60).
- [x] `crates/trace-query-api/src/lib.rs` — confirmed `MAX_RESULT_ROWS`
      is a `pub const` at line 78; `handle_traces` order-of-checks at
      lines 129-191; `success_response` / `error_response` helpers at
      lines 265-280; router constructor at lines 98-103.
- [x] `crates/trace-query-api/tests/common/mod.rs` — `tenant`,
      `open_durable_store`, `seed`, `span_with_ids`, `rich_span`,
      `call`, `spans_array`, `is_error_envelope`, `FailingTraceStore`
      reused unchanged.

## Story-to-AC mapping

| AC | Story | Test function | Tag | State |
|----|-------|---------------|-----|-------|
| AC-01 | US-01 | `ac_01_known_trace_id_returns_all_spans` | `@walking_skeleton @driving_port @real-io @US-01` | enabled |
| AC-01b | US-01 | `ac_01_known_trace_id_carries_every_span_field` | `@driving_port @real-io @US-01` | `#[ignore]` (outer loop) |
| AC-02 | US-02 | `ac_02_unknown_trace_id_returns_empty_array` | `@driving_port @real-io @US-02` | `#[ignore]` (outer loop) |
| AC-03 | US-03 | `ac_03_missing_tenant_returns_401_with_no_store_call_and_no_leak` | `@driving_port @US-03` | `#[ignore]` (outer loop) |
| AC-04a | US-04 | `ac_04a_missing_trace_id_returns_400` | `@driving_port @US-04` | `#[ignore]` (outer loop) |
| AC-04b | US-04 | `ac_04b_empty_trace_id_returns_400` | `@driving_port @US-04` | `#[ignore]` (outer loop) |
| AC-04c | US-04 | `ac_04c_trace_id_31_chars_returns_400` + `ac_04c_trace_id_33_chars_returns_400` | `@driving_port @US-04` | `#[ignore]` (outer loop) |
| AC-04d | US-04 | `ac_04d_non_hex_trace_id_returns_400_with_no_echo` | `@driving_port @US-04` | `#[ignore]` (outer loop) |
| AC-04e | US-04 | `ac_04e_uppercase_trace_id_resolves_to_the_same_bytes` | `@driving_port @real-io @US-04` | `#[ignore]` (outer loop) |
| AC-05 | US-05 | `ac_05_cross_tenant_isolation_on_the_lookup_arm` | `@driving_port @real-io @US-05` | `#[ignore]` (outer loop) |
| AC-CAP | (FLAG 3 / ADR-0053 D3) | `ac_cap_result_cap_applies_uniformly_on_the_lookup_arm` | `@driving_port @US-04 @cap` | `#[ignore]` (documented decision; see Decisions D2 below) |

Total: 12 test functions. Walking skeleton enabled (AC-01); 11 remain
`#[ignore]`d for the one-at-a-time outer-loop convention. Coverage:
every one of US-01 through US-05 has at least one AC.

## Decisions pinned in DISTILL

### D1. Test file location and naming

The acceptance file lands at
`crates/trace-query-api/tests/slice_03_lookup_by_id.rs`. The DESIGN
artefact suggested `slice_02_traces_lookup_by_id.rs`, but slice 02 is
already occupied by `slice_02_caps.rs` (the honest-read-caps slice on
the existing window arm). The next free slice index is 03; the file
name follows the sibling convention
(`crates/log-query-api/tests/slice_03_severity_filter.rs`).

### D2. Result-cap (AC-CAP) covered by `#[ignore]`d breadcrumb, NOT by a 100_001-span fixture

`MAX_RESULT_ROWS = 100_000` is a `pub const`
(`crates/trace-query-api/src/lib.rs:78`) and is NOT cleanly
test-overridable (no `cfg(test)` override exists today; introducing
one would expand the production surface). A 100_001-span fixture in
the acceptance suite would be expensive in CI time and memory and
offer little incremental signal beyond the existing window-arm cap
scenario
`a_result_one_row_over_the_cap_is_refused_with_a_named_400` in
`slice_02_caps.rs`, which already pins the same `if spans.len() >
MAX_RESULT_ROWS { error_response(...) }` branch the lookup arm
reuses. The DELIVER wave's `--in-diff` mutation tests against
`crates/trace-query-api/src/lib.rs` will exercise the structural
uniformity at the parse-handler layer. The decision is documented
in two places: an `#[ignore]`d placeholder test function
(`ac_cap_result_cap_applies_uniformly_on_the_lookup_arm`) with a
verbose `#[ignore = "..."]` reason string, and this paragraph.
ADR-0053 Decision 3 is the durable record.

### D3. Walking-skeleton strategy: Strategy A (full InMemory acceptable; durable adapter chosen for parity)

The lookup arm is a pure function over the `ray::TraceStore` driven
port. Strategy A (full InMemory) would suffice for the user-value
proof. However, the existing slice 01 / slice 02 suites already use
`open_durable_store(label)` (a real `FileBackedTraceStore` in a
unique tempdir per test) for every seeded scenario, and the walking
skeleton inherits that posture for stylistic parity and to keep the
hexagonal boundary proof identical to the sibling slices. The
`@real-io @adapter-integration` tags on AC-01 reflect this. The
remaining seeded scenarios (AC-01b, AC-02, AC-04e, AC-05) also use
the real durable adapter. The 401 / 400 / no-store-call scenarios
(AC-03, AC-04a..d) use `FailingTraceStore` so a leaked call would
lift the response to a 500 and fail the assertion (proving the no-
store-call invariant the contract pins, ADR-0053 D8).

### D4. Scaffold confirmed in `crates/trace-query-api/src/lib.rs`

The RED-ready scaffold is in place. Additions (no edits to existing
items):

- `const TRACES_BY_ID_ROUTE: &str = "/api/v1/traces/by_id";` next to
  the existing `TRACES_ROUTE` const (line 64).
- `.route(TRACES_BY_ID_ROUTE, get(handle_traces_by_id))` chained
  onto the existing `Router` in `router(...)` (lines 100-103).
- `pub struct TracesByIdParams { pub trace_id: Option<String> }`
  with `#[derive(Debug, Deserialize)]`, declared `pub` so the
  acceptance file's external integration test can refer to it if
  needed (the field is `pub` for the same reason; DELIVER may
  narrow visibility once the GREEN handler lands and the public-
  api `cargo public-api` check pins the surface).
- `async fn handle_traces_by_id(...)` with the same signature shape
  as `handle_traces` (`State<ApiState>`, `Query<TracesByIdParams>`,
  returning `Response`) and body `unimplemented!("__SCAFFOLD__
  trace-lookup-by-id-v0 RED")`.

The existing `handle_traces`, `read_required_service`,
`parse_time_range_seconds`, `parse_epoch_seconds`,
`seconds_to_nanos`, `success_response`, `error_response`,
`ApiState`, `TracesParams`, `TRACES_ROUTE`, `MAX_WINDOW_SECONDS`,
`MAX_RESULT_ROWS` items are UNCHANGED. The existing 18 scenarios in
`slice_01_traces_read.rs` and the 8 scenarios in `slice_02_caps.rs`
keep firing verbatim.

### D5. Mandate 7 (RED-not-BROKEN) verification

Verified locally before this handoff:

- `cargo build -p trace-query-api --tests` -> green; the slice 03
  test binary compiles against the real ray + axum + tower surfaces
  and against the scaffold in `lib.rs`.
- `cargo test -p trace-query-api --test slice_03_lookup_by_id` ->
  1 failed (the enabled walking skeleton, panic on
  `__SCAFFOLD__ trace-lookup-by-id-v0 RED`), 11 ignored (the
  one-at-a-time outer-loop convention), 0 compile errors.

The failure is RED (a behavioural panic from `unimplemented!`),
NOT BROKEN (a missing symbol). DELIVER lands the handler body and
enables the scenarios one at a time.

### D6. Shared helpers reused unchanged; new helpers local to slice 03

Reused unchanged from `tests/common/mod.rs`: `tenant`,
`open_durable_store`, `seed`, `span_with_ids`, `rich_span`, `call`,
`spans_array`, `is_error_envelope`, `FailingTraceStore`. NO edit to
`common/mod.rs` in this DISTILL output (the DESIGN brief flagged a
potential `traces_by_id_request` builder in `common`, but the
sibling `slice_03_severity_filter.rs` keeps the request builder
local to the slice file for the same reason — shared helpers stay
unchanged across slices). New helpers local to
`tests/slice_03_lookup_by_id.rs`:

- `traces_by_id_request(trace_id: &str)` — builds the GET request
  with the trace_id as a query parameter. Passes the value through
  verbatim so the 400 arms can submit malformed values.
- `traces_by_id_request_without_trace_id()` — builds the GET
  request with NO `trace_id` parameter at all (AC-04a).
- `seed_trace_with_spans(store, tenant, trace_byte, base_secs, n)`
  — seeds `n` spans sharing one `trace_id` (derived from the
  one-byte selector pattern already used by `span_with_ids`) at
  ascending second offsets, for the happy-path and cross-tenant
  scenarios.

## Mandate compliance evidence

- **CM-A (hexagonal boundary)**: every scenario invokes
  `trace_query_api::router(store, tenant)` (the single driving
  port). Zero imports of internal types beyond the public surface
  (`router`, the future `TracesByIdParams` if exposed). Test doubles
  (`FailingTraceStore`) implement the `ray::TraceStore` driven port
  trait, NOT internal handler components.
- **CM-B (business language)**: the scenarios are written in
  domain terms — "on-call SRE", "tenant", "trace_id", "spans",
  "look up", "calm empty arm". Status-code assertions are present
  but the scenario titles describe the user outcome ("known trace_id
  returns all spans", "unknown trace_id returns empty array",
  "missing tenant returns 401 with no store call and no leak").
  Rust is not Gherkin, so the doc-comments above each
  `#[tokio::test]` carry the Given/When/Then phrasing in business
  vocabulary; the assertions mechanically encode the user-observable
  outcome.
- **CM-C (user-journey completeness)**: AC-01 is the walking
  skeleton — Sara holds a trace_id, GETs the endpoint, sees the
  trace's spans for her tenant. Demo-able to a non-technical
  stakeholder. The remaining scenarios are focused boundary tests
  (15-20 sweet spot honoured: 12 scenarios, one walking skeleton,
  eleven focused). Error path ratio: 8 out of 12 scenarios target
  error / boundary / isolation cases (AC-02 calm empty, AC-03 401,
  AC-04a..d malformed 400s, AC-05 cross-tenant isolation, AC-CAP
  documented placeholder). 67% — well above the 40% mandate.
- **CM-D (pure functions)**: the parse helper `parse_trace_id` is
  a pure function (`&str -> Result<TraceId, String>`); DELIVER
  writes inline unit tests in `lib.rs` `#[cfg(test)]` block (the
  same pattern as the existing `parse_time_range_seconds` inline
  tests at `lib.rs:282-422`). The acceptance scenarios drive the
  handler through the router; the helper is exercised indirectly.

## Outputs produced by this DISTILL wave

1. `crates/trace-query-api/tests/slice_03_lookup_by_id.rs` — the
   new acceptance suite (12 test functions, ~430 LOC).
2. `crates/trace-query-api/src/lib.rs` — RED-ready scaffold edits:
   `TRACES_BY_ID_ROUTE` const, `.route(...)` registration,
   `TracesByIdParams` struct, `handle_traces_by_id` unimplemented
   stub.
3. `docs/feature/trace-lookup-by-id-v0/distill/wave-decisions.md`
   (this file).
4. `docs/feature/trace-lookup-by-id-v0/distill/test-scenarios.md` —
   the AC table with Given/When/Then phrasing per scenario.

NO commit. The pre-commit hook would reject the RED tests because
it runs `cargo test`; DELIVER's atomic test+src GREEN commit lands
the handler body and re-enables the scenarios one at a time.

## Handoff to DELIVER

The DELIVER wave (`@nw-software-crafter`, Crafty) inherits:

1. The 12 acceptance scenarios (one enabled, eleven `#[ignore]`d).
2. The RED-ready scaffold in `lib.rs`.
3. The parse + wire decisions from ADR-0053 and from the DESIGN
   `application-architecture.md`.
4. The one-at-a-time outer-loop convention: walk inward from the
   walking skeleton, enabling the next `#[ignore]`d scenario at
   each REFACTOR turn.
5. The mutation targets named in the DESIGN DEVOPS Handoff
   section (length check boundary, hex-decode per-position check,
   order-of-checks invariant, result-cap branch, cross-tenant
   isolation, success-arm field fidelity).

## Contradictions with DESIGN

None of substance. The DESIGN file suggested the test file name
`slice_02_traces_lookup_by_id.rs`; DISTILL pins it as
`slice_03_lookup_by_id.rs` (D1 above) because slice 02 is already
occupied. The DESIGN file suggested adding `traces_by_id_request`
to `common/mod.rs`; DISTILL keeps it local to the slice file (D6
above) for parity with the sibling `slice_03_severity_filter.rs`.
Neither change affects the contract or any AC.
