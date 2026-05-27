# Test Scenarios: trace-lookup-by-id-v0

Author: `@nw-acceptance-designer` (Scholar), DISTILL wave, 2026-05-27.

This file tabulates the acceptance scenarios for the lookup-by-id arm
of `crates/trace-query-api` (route `GET /api/v1/traces/by_id`). Each
scenario maps to one or more user stories and to a `#[tokio::test]`
function in `crates/trace-query-api/tests/slice_03_lookup_by_id.rs`.
Contract pinned by ADR-0053 (back-references ADR-0048 envelope and
redaction, ADR-0050 cap).

## Conventions

- Every scenario drives `trace_query_api::router(store, tenant)` via
  tower `oneshot`. Driving port: the public `router` function.
- "Seeded" scenarios use `open_durable_store` (a real
  `FileBackedTraceStore` in a unique tempdir) — same posture as the
  existing slice 01 / slice 02 suites.
- "FailingTraceStore" scenarios use the `common::FailingTraceStore`
  double, whose every method returns `PersistenceFailed`; a leaked
  call would lift the response to 500, so a clean 400 / 401 proves
  the no-store-call invariant (ADR-0053 D8).
- "trace_id" is exactly 32 hex characters (case-insensitive) on the
  accept path; any other shape is a 400 (ADR-0053 Decision 2).
- The single literal class label `"invalid trace_id"` is the entire
  reason on every malformed-input arm; the raw value is NEVER
  echoed.

## AC table

| AC | Story | Given | When | Then | Expected status | Expected body shape |
|----|-------|-------|------|------|-----------------|---------------------|
| AC-01 (WS) | US-01 | tenant "acme-prod" has 3 spans seeded under trace_id `abababababababababababababababab` | operator GETs `/api/v1/traces/by_id?trace_id=<id>` for "acme-prod" | response carries exactly the 3 spans, in ascending start_time order, each with `trace_id == "ababab..."` | 200 | bare JSON array of 3 spans |
| AC-01b | US-01 | tenant "acme-prod" has one `rich_span` (every Span field populated) under trace_id `aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa` | operator GETs the lookup endpoint with that trace_id | response carries the one span with all 12 fields present (`trace_id`, `span_id`, `parent_span_id`, `name`, `kind`, `start_time_unix_nano`, `end_time_unix_nano`, `status`, `attributes`, `resource_attributes`, `events`, `links`); name == "place-order"; status.code == "Error"; status.message == "upstream timeout" | 200 | bare JSON array of 1 rich span |
| AC-02 | US-02 | tenant "acme-prod" has spans seeded under a DIFFERENT trace_id (0xFF series) | operator GETs the lookup endpoint with trace_id `00000000000000000000000000000000` | response is the calm empty arm, NOT a 404 | 200 | bare JSON array `[]` |
| AC-03 | US-03 | endpoint configured with `tenant = None`; `FailingTraceStore` mounted | unscoped caller GETs the lookup endpoint with a well-formed trace_id `abcdef0123456789abcdef0123456789` | response is the existing 401 envelope; body does NOT echo the raw trace_id (no leak); store was NEVER called (a leaked call would lift to 500) | 401 | error envelope `{status:"error", error:"no tenant resolvable: ..."}` |
| AC-04a | US-04 | endpoint configured with tenant "acme-prod"; `FailingTraceStore` mounted | operator GETs `/api/v1/traces/by_id` with NO trace_id parameter at all | response is the 400 envelope with the single literal class label; store NEVER called | 400 | error envelope `{status:"error", error:"invalid trace_id"}` |
| AC-04b | US-04 | same as AC-04a | operator GETs the lookup endpoint with `trace_id=` (empty string) | response is the same 400 envelope; store NEVER called | 400 | error envelope `{status:"error", error:"invalid trace_id"}` |
| AC-04c-31 | US-04 | same as AC-04a | operator GETs the lookup endpoint with a 31-char trace_id `0123456789abcdef0123456789abcde` | response is the 400 envelope; store NEVER called | 400 | error envelope `{status:"error", error:"invalid trace_id"}` |
| AC-04c-33 | US-04 | same as AC-04a | operator GETs the lookup endpoint with a 33-char trace_id `0123456789abcdef0123456789abcdef0` | response is the 400 envelope; store NEVER called | 400 | error envelope `{status:"error", error:"invalid trace_id"}` |
| AC-04d | US-04 | same as AC-04a | operator GETs the lookup endpoint with a 32-char trace_id whose final char is `g` (`0123456789abcdef0123456789abcdeg`) | response is the 400 envelope; body does NOT contain the raw trace_id value; body contains neither "SECRET" nor "Bearer"; store NEVER called | 400 | error envelope `{status:"error", error:"invalid trace_id"}` |
| AC-04e | US-04 | tenant "acme-prod" has 3 spans seeded under lowercase trace_id `abababababababababababababababab` | operator GETs the lookup endpoint with the SAME id rendered in uppercase: `ABABABABABABABABABABABABABABABAB` | response carries the same 3 spans (case-insensitive accept; same 16-byte `TraceId`) | 200 | bare JSON array of 3 spans |
| AC-05 | US-05 | tenant "acme-prod" has 3 spans seeded under trace_id `cdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcd`; tenant "globex-prod" has none | endpoint resolving "globex-prod" GETs the lookup endpoint with that trace_id | response is the calm empty arm; body does NOT contain any acme-prod span name ("step-0", "step-1", "step-2") | 200 | bare JSON array `[]` |
| AC-CAP | (FLAG 3) | (documented placeholder) | (documented placeholder) | structural uniformity of `MAX_RESULT_ROWS` cap is verified at the parse-handler layer; same `if spans.len() > MAX_RESULT_ROWS` branch as the existing window arm (`slice_02_caps.rs`); covered by `--in-diff` mutation tests | (n/a) | (n/a) |

## Scenario count and ratios

- Total scenarios: 12 (`#[tokio::test]` functions).
- Walking skeletons: 1 (AC-01).
- Focused scenarios: 11.
- Happy-path scenarios: 3 (AC-01, AC-01b, AC-04e — the latter is
  also a happy-path under case-insensitive accept).
- Error / boundary / isolation scenarios: 8 (AC-02 calm empty,
  AC-03 401, AC-04a..d malformed 400s, AC-05 cross-tenant).
- Documented placeholder: 1 (AC-CAP).
- Error-path ratio: 8 / 12 = 67% (well above the 40% mandate).
- US coverage: every one of US-01 through US-05 has at least one AC.

## Self-review checklist

- [x] Mandate 1 (hexagonal boundary): every scenario invokes
      `trace_query_api::router(store, tenant)` — the single driving
      port. No internal handler / parser / formatter directly tested.
- [x] Mandate 2 (business language): doc-comments above each test
      use domain terms (on-call SRE, tenant, trace_id, spans, calm
      empty arm, cross-tenant isolation). Status-code assertions
      are mechanically necessary but co-located with the user-
      outcome assertion.
- [x] Mandate 3 (user-journey completeness): AC-01 is a demo-able
      walking skeleton; the other scenarios are focused boundary
      and error tests at the same driving port.
- [x] Mandate 4 (pure functions): the parse helper
      `parse_trace_id(&str) -> Result<TraceId, String>` is a pure
      function; DELIVER lands its inline `#[cfg(test)] mod tests`
      in `lib.rs` alongside the existing `parse_time_range_seconds`
      inline tests at `lib.rs:282-422`.
- [x] Mandate 7 (RED-not-BROKEN): `cargo build -p trace-query-api
      --tests` is green; `cargo test -p trace-query-api --test
      slice_03_lookup_by_id` shows the walking skeleton panicking
      with `__SCAFFOLD__ trace-lookup-by-id-v0 RED` (RED) and the
      eleven `#[ignore]`d scenarios skipped (one-at-a-time outer
      loop).
- [x] Story coverage: every US-01..US-05 has at least one AC.
- [x] Cross-tenant isolation explicitly covered (AC-05; the riskier
      sibling of AC-02 per US-05).
- [x] Anti-echo verified: AC-03 asserts the raw trace_id is NOT in
      the body; AC-04d asserts the raw value is NOT in the body
      AND the body contains neither "SECRET" nor "Bearer".
- [x] No-store-call invariant covered on every fail-closed and
      malformed-input path (AC-03 401; AC-04a..d 400) via
      `FailingTraceStore` (a leaked call lifts the response to
      500, which would fail the 401/400 assertion).
- [x] Case-insensitive accept pinned (AC-04e; ADR-0053 Decision 2).
- [x] Cap decision documented (AC-CAP `#[ignore]`d placeholder +
      DISTILL wave-decisions.md D2 + ADR-0053 Decision 3).
- [x] One-at-a-time outer loop honoured: AC-01 enabled; eleven
      others `#[ignore]`d with the same explanatory reason string.
- [x] DEVOPS target environments matched: single `clean` target
      (in-process via tower `oneshot`); no Dockerfile or external
      service required.

## Traceability table — US to AC

| US | Covered by ACs |
|----|----------------|
| US-01 | AC-01 (walking skeleton), AC-01b (field fidelity), AC-04e (case-insensitive happy path also exercises the same seam) |
| US-02 | AC-02 |
| US-03 | AC-03 |
| US-04 | AC-04a, AC-04b, AC-04c-31, AC-04c-33, AC-04d, AC-04e |
| US-05 | AC-05 |

Every story has at least one AC. AC-CAP documents the FLAG 3 / ADR-
0053 Decision 3 cap-uniformity decision and is not bound to a US
directly (it is a contract decision, not a user-value story).
