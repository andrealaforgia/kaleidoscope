# Slice 01 - trace-lookup-by-id

Thin slice on top of `crates/trace-query-api`. ONE new way to ask: look
up a trace by id. The substrate seam `ray::TraceStore::get_trace(
&tenant, &trace_id) -> Result<Vec<Span>, TraceStoreError>` already
exists at `crates/ray/src/store.rs:72` with the right semantics
("Return every span sharing `trace_id` for this tenant. Empty trace
returns `Ok(Vec::new())`"). The DELIVER is parse-and-wire on the HTTP
boundary. NO ray change. NO change to any storage trait.

## Walking-skeleton scenario (the demoable outcome)

> Tenant `acme-prod` has six spans seeded into a real durable ray
> store, distributed across two trace_ids: FOUR spans share trace_id
> `"abc123abc123abc123abc123abc12345"` (a Server "place-order", an
> Internal "reserve-stock", an Internal "decrement-inventory", a
> Client "charge-card", all with `service.name = "checkout"`), and
> TWO spans share trace_id `"ffffffffffffffffffffffffffffffff"` (a
> Server "health-probe" and an Internal "stats-flush", both with
> `service.name = "ops"`).
> The operator runs `curl
> 'http://traces.kaleidoscope.acme.internal/<lookup_url>?trace_id=abc123abc123abc123abc123abc12345'`
> (the exact `<lookup_url>` shape is FLAG 1 below; either
> `/api/v1/traces/by_id` or `/api/v1/traces` with `trace_id`
> overriding).
> The response is HTTP 200 with a bare JSON array of exactly the four
> spans of trace `"abc123..."`, in ascending `start_time_unix_nano`
> order. The two `"ffff..."` spans are NOT in the response. The shape
> of each span is the same `ray::Span` `serde::Serialize` derive the
> existing window arm returns.

## Learning hypothesis

> One parse step (`trace_id` as 32-char hex, case-insensitive) plus
> one wired call to the EXISTING `ray::TraceStore::get_trace` seam is
> enough to satisfy the operator's "give me this trace" job, without
> changing the ray trait, without changing the bare-JSON-array
> response shape, and without changing the existing caps.

The hypothesis is FALSIFIED if any of the following happens during DELIVER:

- The ray trait grows a new method.
- The `ray::TraceStore::get_trace` signature changes.
- A new error envelope or status code is introduced.
- An existing acceptance scenario in
  `tests/slice_01_traces_read.rs` is edited or deleted.
- The `MAX_WINDOW_SECONDS` or `MAX_RESULT_ROWS` constants change.
- The bare-JSON-array success shape (ADR-0048 Decision 2) changes.
- The lookup arm needs a new `gate-5-mutants-*` CI job (it should ride
  the existing `gate-5-mutants-trace-query-api` workflow).

Any of these means the slice is bigger than thought and should be
re-scoped under a successor feature ID, NOT under this one.

## Scope (IN)

- One new way to ask: lookup by `trace_id`. URL shape pinned by DESIGN
  (FLAG 1; recommended: new path `/api/v1/traces/by_id?trace_id=...`).
- Accepted `trace_id` value: a 32-character hex string, case-insensitive
  on the hex digits `a-f` / `A-F` (FLAG 2 recommendation; OTel W3C
  trace context shape).
- The lookup runs via the EXISTING `TraceStore::get_trace(&tenant,
  &trace_id)` seam against the real durable `FileBackedTraceStore`.
- Response on success: bare JSON array of `ray::Span`s in ascending
  `start_time_unix_nano` order, using the EXISTING `success_response`
  helper (`crates/trace-query-api/src/lib.rs:265`).
- Response on unknown `trace_id`: HTTP 200 with the bare empty array
  `[]`. NEVER 404. NEVER an error envelope. This is the calm-empty
  arm, identical in shape to the existing window arm's empty case
  (ADR-0048 Decision 2).
- Response on no-tenant: HTTP 401 with the EXISTING error envelope
  (`{"status":"error","error":"no tenant resolvable: ..."}`); no
  `get_trace` call is made on this path.
- Response on invalid `trace_id` format (non-hex, wrong length,
  missing): HTTP 400 with the EXISTING error envelope; the raw
  `trace_id` value is NEVER echoed; the body contains neither
  "SECRET" nor "Bearer"; no `get_trace` call is made on this path.
- Result cap: `MAX_RESULT_ROWS = 100_000` (ADR-0050 Decision 2)
  applies uniformly (FLAG 3 recommendation). A `Vec<Span>` whose
  length strictly exceeds the cap is refused with the EXISTING
  NAMED 400 ("result exceeds 100000 rows"); the boundary is `>`, not
  `>=`. Serialisation never starts.
- Tenant fail-closed seam: reuse the existing seam at
  `lib.rs:134-142`; tenant resolves to `Some(TenantId)` or `None`;
  the new arm honours the same posture as the existing arm.
- Test posture: `tokio::test` + `tower::ServiceExt::oneshot` against
  the `Router`; REAL durable `FileBackedTraceStore` for the success
  and isolation arms; `FailingTraceStore` double for the
  400-before-store and 500 arms; reuse `tests/common/mod.rs` helpers
  (`tenant`, `open_durable_store`, `seed`, `span_with_ids`, `call`,
  `spans_array`, `is_error_envelope`); ADD a new request-builder
  helper for the lookup URL shape (`traces_by_id_request`,
  `traces_by_id_request_without_trace_id`,
  `traces_by_id_request_with_auth`).

## Scope (OUT - declared, deferred)

- Tree assembly: root + children topology, parent/child resolution
  beyond the bare-array shape.
- Per-service filter inside a trace (e.g. `?trace_id=...&service=...`
  on the lookup arm).
- Batch lookup of multiple `trace_id`s in one request
  (`?trace_id=A,B,C`).
- Complex URL path routing (e.g. REST-style
  `GET /api/v1/traces/<trace_id>` with the id as a path segment).
- Cap relaxation specifically for the lookup arm. The cap is uniform
  (FLAG 3 recommendation).
- A `get_trace_with(predicate)` seam on the ray trait. Predicate
  matchers on the lookup arm are a later slice that would need a
  ray change.
- Aliases or partial-`trace_id` lookups (e.g. accepting a 16-character
  W3C span_id by mistake with a helpful diagnostic; rejected per
  redaction posture, FLAG 2).
- Env-driven default `trace_id` (no such thing; nonsensical for an
  identifier lookup).
- The `query-http-common` extraction (ADR-0048 Decision 5).
- Any change to `MAX_WINDOW_SECONDS` / `MAX_RESULT_ROWS`.
- Any prism UI for the lookup arm.

## Mapping to user stories

| Story | Scenario(s) in user-stories.md | Walking-skeleton role |
|---|---|---|
| US-01 | "Operator reads only that trace's spans by trace_id" + field fidelity + reads from the real store | THE walking skeleton |
| US-02 | "An unknown trace_id returns 200 with the empty array" + "An unknown trace_id is never a 404" | Calm-empty contract on the new arm |
| US-03 | "A lookup with no resolvable tenant is refused" + "Spans existing under a tenant are not returned to an unscoped caller" | Fail-closed tenant seam |
| US-04 | "A non-hex trace_id is rejected" + "A wrong-length trace_id is rejected" + "A missing trace_id is rejected" + "A malformed trace_id error never echoes the raw value" | Redaction + no-store-on-400 |
| US-05 | "A trace_id present under tenant A returns the empty arm for tenant B" + "A trace_id present under both tenants returns only the resolved tenant's spans" | Cross-tenant isolation on the by-id key |

## Acceptance file (DELIVER target)

`crates/trace-query-api/tests/slice_01_trace_lookup_by_id.rs` - NEW.
Reuses `mod common` helpers from `tests/common/mod.rs`. Follows the
established one-at-a-time outer-loop convention from
`tests/slice_01_traces_read.rs` (walking skeleton enabled first,
following scenarios `#[ignore]`'d until enabled one at a time by the
crafter).

NEW helpers landing in `common/mod.rs` in the same slice-01 commit:

- `traces_by_id_request(trace_id: &str) -> Request<Body>` - the GET
  request for the lookup URL shape DESIGN picks (FLAG 1).
- `traces_by_id_request_without_trace_id() -> Request<Body>` - the
  GET request that omits the `trace_id` parameter entirely; the
  handler must return 400 before touching the store.
- `traces_by_id_request_with_auth(trace_id: &str, authorization: &str)
  -> Request<Body>` - the GET request with a forwarded
  `Authorization` header, for the redaction arm.
- `trace_ids_in(body: &Value) -> Vec<String>` - the `trace_id`
  strings of the returned spans, for the "only-this-trace's-spans"
  assertion.

## Flags to DESIGN

1. **Endpoint shape**: new path `GET /api/v1/traces/by_id?trace_id=...`
   (clean separation, recommended) vs extending the existing
   `/api/v1/traces` with `trace_id` overriding `service`+`start`+`end`
   (single endpoint, more behaviours). See `discuss/wave-decisions.md`
   FLAG 1.
2. **trace_id format**: 32-character hex, case-insensitive on the
   hex digits `a-f`/`A-F` (matching the substrate's hex codec at
   `crates/ray/src/span.rs:42-60`); reject other lengths including the
   16-character W3C span_id mistake (recommended). See FLAG 2.
3. **Cap interaction (ADR-0050)**: the existing `MAX_RESULT_ROWS =
   100_000` cap fires uniformly on the lookup arm (recommended); a
   pathological client could in principle produce a single trace
   over the cap. See FLAG 3.
4. **ADR-0053 vs refinement of ADR-0048**: a small ADR-0053 ("trace
   lookup by id"), back-referencing ADR-0048, in the same shape
   ADR-0052 back-referenced ADR-0047 (recommended; ADRs in this
   repository are immutable). See FLAG 4.

See `discuss/wave-decisions.md` § "Flags to DESIGN" for the reasoning
behind each recommendation.

## Estimated effort

2-3 days end-to-end:

- Parse one parameter (`trace_id` as 32-char hex, case-insensitive)
  and reject the four malformed shapes (missing, empty, wrong length,
  non-hex) with the named 400 (~40 lines).
- Wire one new handler arm (or one new route under a new handler;
  FLAG 1) to call `TraceStore::get_trace` and serialise the bare
  array (~15 lines, reusing the existing `success_response`).
- Apply the `MAX_RESULT_ROWS` cap on the returned `Vec<Span>`
  uniformly (~5 lines, reusing the existing cap check).
- Five acceptance scenarios in one new test file (~180 lines, mostly
  fixture seeding via existing helpers plus the new request-builder
  helpers).
- One mutation-test pass on the modified files (existing
  `gate-5-mutants-trace-query-api` workflow, no new CI job).
