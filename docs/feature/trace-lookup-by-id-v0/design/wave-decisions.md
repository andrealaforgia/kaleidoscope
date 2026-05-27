# Wave Decisions: trace-lookup-by-id-v0 — DESIGN wave

Author: `@nw-solution-architect` (Morgan), DESIGN wave, 2026-05-27.
Mode: propose.

This file pins the four DISCUSS-wave flags and records the
parse-and-wire micro-decisions the crafter and the acceptance
designer need to take the slice to GREEN without further design
ambiguity. ADR-0053 carries the durable cross-reference for the
contract growth (the new path on the trace-query-api crate). The
ray substrate is unchanged.

## Inputs read (read-first checklist)

The DESIGN wave was grounded in the following files, read in full
before any artefact was written:

- [x] `docs/feature/trace-lookup-by-id-v0/discuss/user-stories.md`
      — five user stories (US-01 happy path; US-02 unknown trace_id
      is `200 []`; US-03 fail-closed tenancy 401; US-04 malformed
      trace_id 400 with no echo; US-05 cross-tenant isolation 200
      `[]`) plus the four flags named with recommendations.
- [x] `docs/feature/trace-lookup-by-id-v0/discuss/story-map.md`
      — backbone (Receive request -> Resolve tenant -> Resolve
      query inputs -> Query ray -> Render spans); walking skeleton;
      scope assessment (PASS, 5 stories, 1 crate, 2-3 days).
- [x] `docs/feature/trace-lookup-by-id-v0/discuss/outcome-kpis.md`
      — five KPIs (one HTTP call pivot from trace_id to spans;
      100% field fidelity; p95 latency <= 200 ms on CI; tenant
      fail-closed and zero cross-tenant leak; redaction with no
      raw value echo and no store call on the 400 path).
- [x] `docs/feature/trace-lookup-by-id-v0/discuss/wave-decisions.md`
      — seven scope and process decisions taken in DISCUSS; four
      flags to DESIGN; risks (LOW); coherence check; verified
      facts grounded in source.
- [x] `docs/product/architecture/adr-0048-ray-trace-query-api-contract-and-crate-layout.md`
      — Decision 1 (the existing route `/api/v1/traces` requires
      `service`); Decision 2 (the bare JSON array success shape;
      the `{status:"error", error}` envelope; the redaction
      posture stricter than logs: the body must contain neither
      "SECRET" nor "Bearer", and never the raw `service` / `start`
      / `end` values); Decision 6 (the `TraceStore` trait is
      UNCHANGED; `get_trace` EXISTS but is NOT used in slice 01 of
      `ray-query-api-v0`). This ADR is the precedent ADR-0053
      GROWS by one new sibling path, NOT modified.
- [x] `docs/product/architecture/adr-0050-earned-trust-read-side-caps.md`
      — Decision 1 (window cap 86_400s; uniform across the three
      crates); Decision 2 (result cap `MAX_RESULT_ROWS = 100_000`;
      measured AFTER the store returns and BEFORE serialisation);
      Decision 3 (REFUSE, never TRUNCATE); Decision 4 (the cap
      measures what the user observes, not the upstream raw row
      count). This ADR is the precedent ADR-0053 honours at the
      result-cap arm of the new lookup endpoint, NOT modified.
- [x] `docs/product/architecture/adr-0052-log-query-severity-filter.md`
      — the immediate stylistic sibling; the layout of a small
      ADR for a parse-and-wire thin slice growing an existing read
      contract (sections, decision count, alternatives,
      consequences shape) is mirrored here. Cited as the style
      precedent, NOT modified.
- [x] `crates/ray/src/store.rs` — confirmed
      `TraceStore::get_trace(&self, tenant: &TenantId, trace_id:
      &TraceId) -> Result<Vec<Span>, TraceStoreError>` at line 72;
      doc-comment semantics at line 70-71 ("Return every span
      sharing `trace_id` for this tenant. Empty trace returns
      `Ok(Vec::new())`"); `InMemoryTraceStore::get_trace` at lines
      182-192 returns `Ok(Vec::new())` for an unknown `(tenant,
      trace_id)` key via `state.by_trace.get(&key).cloned()
      .unwrap_or_default()`. NO ray change needed.
- [x] `crates/ray/src/span.rs` — confirmed `TraceId(pub [u8; 16])`
      at line 65 (the inner array is `pub`; the parse helper in
      `trace-query-api` constructs `TraceId(bytes)` directly
      after manual hex decode); the `Serialize` impl writes 32
      lowercase hex characters at lines 67-71; the `Deserialize`
      impl at lines 73-87 accepts EXACTLY 32 hex characters via
      the private `hex::decode::<16>` helper at lines 42-60. The
      `hex` module is `mod hex { ... }` (crate-private,
      `pub(self)`); `trace-query-api` therefore hand-rolls the
      same case-insensitive 32-hex-char decode at the HTTP
      boundary (D6 below). The codec accepts both `a-f` and `A-F`
      via `(byte as char).to_digit(16)`.
- [x] `crates/trace-query-api/src/lib.rs` — confirmed the handler
      shape (`handle_traces` at lines 129-191); the `TracesParams
      { service, start, end }` struct at lines 115-120; the
      `read_required_service` 400 arm at lines 197-204 (the
      precedent shape for the new `read_required_trace_id` 400
      arm); the `error_response` envelope helper at lines 274-280;
      the `success_response(Vec<Span>)` bare-array helper at lines
      265-267 (REUSED on the new arm verbatim); the
      `MAX_RESULT_ROWS = 100_000` constant at line 78 (REUSED on
      the new arm); the `MAX_WINDOW_SECONDS = 86_400` constant at
      line 71 (UNUSED on the new arm — no window check); the
      `ApiState { store, tenant }` struct at lines 82-86 (REUSED
      verbatim as the shared state); the router constructor at
      lines 98-103 (the place the new route is added next to the
      existing one); the `TRACES_ROUTE = "/api/v1/traces"`
      constant at line 64 (UNCHANGED; the new path is added as a
      sibling constant, not a modification of this one). NOTE:
      the existing `handle_traces` and its order-of-checks
      (tenancy -> required service -> parse window -> window cap
      -> store -> result cap -> serialise) is UNCHANGED.
- [x] `crates/log-query-api/src/lib.rs` — confirmed
      `parse_min_severity(raw: &str) -> Result<SeverityNumber,
      String>` at lines 261-291; the shape (`fn`, not `pub fn`;
      returns `Result<T, String>` where `Err` is the literal
      reason text used by `error_response`; redaction by
      construction: the reason is the literal class label and the
      raw value is never echoed) is the precedent ADR-0053
      reproduces verbatim for the new `parse_trace_id(raw: &str)
      -> Result<TraceId, String>` helper (D6 below). The
      rule-of-three for extracting this parse pattern into a
      `query-http-common` crate (M-5; ADR-0048 Decision 5) is
      noted as mounting pressure but NOT acted on in this slice.

## Flags resolved (the four DISCUSS flags)

| # | Flag | DISCUSS recommendation | DESIGN decision | Rationale |
|---|---|---|---|---|
| 1 | Endpoint shape (URL surface) | New separate path `/api/v1/traces/by_id?trace_id=<32-hex>` (FLAG 1 option a) | **PIN: new path `/api/v1/traces/by_id`** | Separation of concerns: the two shapes ask two different questions (a window-by-service filter; a single-key lookup). A separate path makes the question honest at the URL surface rather than hiding it behind a precedence rule ("trace_id wins, service unread") in the body of one handler. No silent fall-through on the existing route: a request that fat-fingers `service` and supplies a stale `trace_id` is served against the operator's expectation if the routes share. The existing 18 acceptance scenarios in `crates/trace-query-api/tests/slice_01_traces_read.rs` keep firing verbatim; the new scenarios land in a sibling test file. The ~10 LOC of scaffolding duplicated across the two handlers is bounded and accepted; the rule-of-three extraction (M-5) is now under genuine pressure (this is the third sibling crate to grow a parse-and-wire arm after `query-api`, `log-query-api`, and the original `trace-query-api`) and is annotated for a near-future slice. |
| 2 | trace_id wire format | 32 hex characters, case-insensitive on the hex digits (FLAG 2 option a) | **PIN: 32 hex characters, case-insensitive; len != 32 -> 400; non-hex -> 400; empty -> 400** | Matches the OTel / W3C trace context spec (128-bit trace_id = 16 bytes = 32 hex chars). Matches the substrate: the ray hex codec at `crates/ray/src/span.rs:42-60` accepts both `a-f` and `A-F` via `(byte as char).to_digit(16)`; the HTTP boundary does not invert or relax the substrate's rule. Matches the operator: clipboards vary in case across logging pipelines and incident-ticket renderings; rejecting `FFFF...` while accepting `ffff...` is user-hostile for a parameter that names the same identifier either way. Matches the immediate precedent (ADR-0052 Decision 2: case-insensitive on the six OTel severity names). The clever diagnostic of option (b) ("span_ids are 16 chars, not 32") is REJECTED — it leaks a property of the raw value into the error text, breaking the redaction posture (ADR-0048 Decision 2: the body must contain neither "SECRET" nor "Bearer", and never the raw value); the operator's path to the right shape is the OTel spec, not a clever 400. The literal class label `"invalid trace_id"` is the entire error reason on every malformed-input path. |
| 3 | Cap interaction (`MAX_RESULT_ROWS`) | Uniform: the cap fires on the lookup arm too (FLAG 3 option a) | **PIN: uniform `MAX_RESULT_ROWS = 100_000` applies to the lookup arm; NO window cap (no window)** | ADR-0050 Decision 3 ("REFUSE, never TRUNCATE") and Decision 2 (`MAX_RESULT_ROWS = 100_000`, uniform across the three crates) are PRESERVED. A typical trace is dozens of spans; the cap is plausibly never hit on the happy path. A misbehaving client (pathologically deep recursive instrumentation, a stray test harness, a deliberate replay attack) could in principle produce a single trace whose span count exceeds the cap; the lookup arm refuses out loud with the existing `"result exceeds 100000 rows"` reason text and the existing `{status:"error", error}` envelope at HTTP 400, same as the existing window arm and the sibling `query-api` / `log-query-api` arms. NO window cap applies (the lookup arm has no window parameter; `MAX_WINDOW_SECONDS` is inert on this arm). The result-cap check fires AFTER the store returns and BEFORE serialisation (ADR-0050 Decision 2), measuring the `Vec<Span>` length the substrate returned. |
| 4 | ADR placement (new ADR-0053 vs in-place edit of ADR-0048) | New small ADR-0053 (FLAG 4 option a) | **PIN: write ADR-0053** | ADRs in this repository are immutable (the repo-wide convention set by ADR-0001 and honoured by every preceding ADR including ADR-0049, ADR-0050, ADR-0051, ADR-0052). The contract is growing by one new sibling path on the `trace-query-api` crate (a new accepted parameter name `trace_id`, a new 400 reason class `"invalid trace_id"`, a new 401-then-400 order on a sibling route, a new envelope-reuse cite); that growth lands as a new ADR with cross-reference to ADR-0048 and ADR-0050, neither modified. ADR-0053 number verified free by `ls docs/product/architecture/adr-0053*` (no hits; `adr-0052-log-query-severity-filter.md` is the latest; 0053 is the next). The new ADR mirrors ADR-0052's layout: small, back-referencing, six decisions, alternatives considered, consequences. |

## Other decisions pinned (parse + wire micro-decisions)

These are the small mechanical choices the crafter needs and the
acceptance designer needs in order to write the slice without
further design ambiguity. They are NOT contract decisions; they
are HOW-it-wires inside the existing `trace-query-api` crate.

### D5. Router wiring: a new route next to the existing one, ApiState reused

The `router` constructor at `crates/trace-query-api/src/lib.rs:98-103`
grows by ONE additive route registration. The existing route
declaration is UNCHANGED; the new route is added as a sibling:

```text
const TRACES_ROUTE: &str = "/api/v1/traces";              // existing, UNCHANGED
const TRACES_BY_ID_ROUTE: &str = "/api/v1/traces/by_id";  // NEW

pub fn router(store: Arc<dyn TraceStore + Send + Sync>, tenant: Option<TenantId>) -> Router {
    let state = ApiState { store, tenant };
    Router::new()
        .route(TRACES_ROUTE, get(handle_traces))                    // existing, UNCHANGED
        .route(TRACES_BY_ID_ROUTE, get(handle_traces_by_id))        // NEW
        .with_state(state)
}
```

The `ApiState { store, tenant }` struct is REUSED verbatim; no new
state, no new field, no new constructor. The single shared state
carries both handlers. The fail-closed tenant seam (`tenant:
Option<TenantId>`) is shared by both handlers; the existing
`KALEIDOSCOPE_TRACE_QUERY_TENANT` env-var resolution at the
composition root is unchanged.

The above is illustrative; the crafter owns the GREEN / REFACTOR
shape. What is PINNED here: the new sibling route constant
spelling (`/api/v1/traces/by_id`); the new handler name
(`handle_traces_by_id`); the shared `ApiState` reuse; both routes
mounted on the same `Router`.

### D6. New handler `handle_traces_by_id` — params, parse, order of checks

A new private struct mirrors the existing `TracesParams` shape but
with one field:

```text
#[derive(Debug, serde::Deserialize)]
struct TracesByIdParams {
    trace_id: Option<String>,
}
```

The single field is `Option<String>` so axum's `Query` extractor
accepts a missing parameter and the handler emits the contract's
named 400 itself (rather than axum's default rejection body),
matching the pattern of the existing `TracesParams`.

The new handler `handle_traces_by_id` runs the following ORDER OF
CHECKS (PINNED):

1. **Resolve tenant** (fail-closed). Same seam as the existing
   handler at `lib.rs:134-142`. `state.tenant.is_none()` -> HTTP
   401 with `"no tenant resolvable: the trace query service
   refuses unscoped requests"` (the existing reason text reused
   verbatim). No store call.
2. **Read required `trace_id`** (presence check). Missing /
   `None` -> HTTP 400 with `"invalid trace_id"`. Empty
   string `Some("")` -> HTTP 400 with `"invalid trace_id"`. No
   store call. (Treated as a single class of fault: "the
   identifier is not a 32-hex string". The DISCUSS user-stories
   permit a slightly more verbose "trace_id is required" reason
   on the missing path, but the redaction-by-class-label posture
   favours one literal reason for every malformed arm; PINNED at
   the single class label `"invalid trace_id"`.)
3. **Parse `trace_id`** (format check, 32 hex chars,
   case-insensitive). Length != 32 -> HTTP 400 with `"invalid
   trace_id"`. Any non-hex character (anything other than
   `[0-9a-fA-F]`) -> HTTP 400 with `"invalid trace_id"`. The raw
   parameter value is NEVER echoed in the reason text (redaction
   per ADR-0048 Decision 2 extended to the new parameter). No
   store call.
4. **`store.get_trace(&tenant, &trace_id)`** with the parsed
   `TraceId`. On `Err(TraceStoreError::PersistenceFailed { reason
   })` -> HTTP 500 with `"the backing trace store could not be
   read"` (the existing reason text on the existing arm reused
   verbatim) and a `tracing::error!` event matching the existing
   `traces.store.failed` shape.
5. **Result cap** (ADR-0050 Decision 2). `spans.len() >
   MAX_RESULT_ROWS` -> HTTP 400 with the existing `"result
   exceeds 100000 rows"` reason text. NO window cap (the lookup
   arm has no `start`/`end` parameters; `MAX_WINDOW_SECONDS` is
   inert on this arm).
6. **`success_response(spans)`** — HTTP 200 with the bare JSON
   array of spans (`[]` when empty), in ascending
   `start_time_unix_nano` order (the store's natural order).
   `Span` carries its own `Serialize` derive; field fidelity is
   the same `Span` machinery the existing arm uses; no
   hand-written mapping.

NOTE on order: tenancy fail-closed FIRST (step 1), THEN the
trace_id presence + format checks (steps 2 and 3). This matches
the existing arm's order (`lib.rs:134-150`: tenancy -> required
service -> parse window) and the security posture: an unscoped
caller never learns whether a given trace_id is well-formed (the
401 fires before the 400 can reveal anything about the input).

### D7. Parse helper `parse_trace_id` — shape, signature, behaviour

A new free function in `crates/trace-query-api/src/lib.rs`, next
to `read_required_service` and `parse_time_range_seconds`:

```text
fn parse_trace_id(raw: &str) -> Result<TraceId, String>
```

Shape (PINNED):

- `fn`, not `pub fn`: same visibility as `read_required_service`
  and `parse_time_range_seconds`. The function is module-internal
  to the existing crate.
- Returns `Result<TraceId, String>`. The `Err` arm is the
  literal class label `"invalid trace_id"` — IDENTICAL across
  every malformed-input path. The raw parameter value is NEVER
  in the `Err` payload.
- Imports `TraceId` from `ray` (`use ray::TraceId;`).

Behaviour (PINNED):

1. If `raw.len() != 32` -> `Err("invalid trace_id".to_string())`.
   This catches empty strings (length 0), the 16-character
   W3C-span_id-shape mistake (length 16), and any other wrong
   length.
2. For each byte position `i` in `0..16`: read two ASCII bytes
   from `raw.as_bytes()`; map each via
   `(byte as char).to_digit(16)`. Any `None` (i.e. any
   non-hex character, in either case) -> `Err("invalid
   trace_id".to_string())`. Otherwise pack the two nibbles into
   one byte. The `to_digit(16)` route accepts BOTH `a-f` and
   `A-F` by construction (the same approach used by the ray
   `hex::decode::<16>` private helper at
   `crates/ray/src/span.rs:42-60`); the HTTP boundary
   reproduces the substrate's case-insensitive rule rather than
   inverting it.
3. Return `Ok(TraceId(bytes))`. The `TraceId.0` inner array is
   `pub [u8; 16]` (`crates/ray/src/span.rs:65`); the constructor
   is direct positional struct construction.

The helper does NOT call `ray::TraceId::deserialize` indirectly
via `serde_json` (no allocation of an intermediate `Value`); it
does NOT depend on the `hex` crate (`ray` keeps its own
hand-rolled hex codec; the trace-query-api boundary follows the
same hand-rolled posture for the same reason — minimum
dependency footprint per CLAUDE.md and ADR-0048's substrate
shape).

The helper is the SECOND instance of the parse-helper pattern
(after `parse_min_severity` in `log-query-api`). The
rule-of-three for extracting to `query-http-common` (ADR-0048
Decision 5, M-5) is now under genuine pressure (PINNED as a
DEFERRED note; not extracted in this slice).

### D8. The new 400 arms NEVER touch the store; the 401 arm NEVER touches the store

The order of checks (D6) is enforced by the handler's control
flow: tenancy fail-closed returns 401 BEFORE the trace_id read;
the presence / parse checks return 400 BEFORE the dispatch to
`get_trace`. The store is NEVER called on the 401 path; the store
is NEVER called on either 400 path. The acceptance scenarios
US-03 (no-store-call assertion on the 401 path) and US-04
(no-store-call assertion on every 400 path) encode this with a
`FailingTraceStore` test double whose `get_trace` returns
`PersistenceFailed`: a clean 401 or a clean 400 PROVES the store
was not reached (a leaked call would lift the response to a
500). The `FailingTraceStore` double is REUSED unchanged from
`crates/trace-query-api/tests/common/mod.rs`.

### D9. Existing helpers reused unchanged

The new handler reuses, verbatim, from the existing crate:

- `error_response(status, reason)` for every error arm (401, 400,
  500). The `{status:"error", error:"<reason>"}` envelope shape is
  IDENTICAL on the new arm; no new envelope field; no new error
  status code.
- `success_response(Vec<Span>)` for the 200 arm. The bare JSON
  array shape (with `[]` for empty) is IDENTICAL on the new arm;
  field fidelity rides `Span`'s existing `Serialize` derive; no
  hand-written mapping.
- `ApiState { store, tenant }` for the shared application state.
- `MAX_RESULT_ROWS = 100_000` for the result cap. The constant is
  the existing `pub const`; the value is UNCHANGED.
- The fail-closed tenant seam (`tenant: Option<TenantId>` on
  `ApiState`). The seam is shared between both handlers; the
  composition-root env-var resolution is unchanged.
- The `tracing::error!` event shape for the 500 arm. The event
  name (`traces.store.failed`) and the structured field
  (`reason`) are REUSED verbatim from the existing handler.

The crate's public API is byte-identical to the prior tag: the
`router` function signature is unchanged; the
`TRACES_ROUTE` `pub const` is unchanged; the
`MAX_WINDOW_SECONDS` and `MAX_RESULT_ROWS` `pub const`s are
unchanged. The new `TRACES_BY_ID_ROUTE` `const` is module-private
(not `pub`). The new `handle_traces_by_id` function and the new
`TracesByIdParams` struct are module-private. Gate 2 `cargo
public-api` confirms zero diff on the crate's public surface.

The `ray::TraceStore` trait signatures are byte-identical (Gate 2
`cargo public-api` on ray confirms); `get_trace` already exists at
line 72 and is used for the first time on the HTTP boundary in
this slice. No trait change.

## Reuse Analysis (mandatory table)

| Existing Component | File | Overlap | Decision | Justification |
|---|---|---|---|---|
| `handle_traces` (existing trace endpoint) | `crates/trace-query-api/src/lib.rs:129-191` | Shared scaffold (axum `Query` extractor, `ApiState` shape, tenancy seam, error envelope, result-cap check, bare-array serialisation) | **EXTEND crate (add sibling handler `handle_traces_by_id`)** | The two handlers ask two different questions (window-by-service vs single-key lookup); separation of concerns at the URL surface (FLAG 1, decision PIN above) means two handlers, not one with branched dispatch. ~10 LOC of scaffolding duplicated (the tenancy match, the error-response calls, the result-cap check). Acceptable: bounded, mutation-tested in place, and the rule-of-three (M-5) for extraction into a `query-http-common` crate is now under genuine pressure and annotated as DEFERRED. |
| `ray::TraceStore::get_trace` | `crates/ray/src/store.rs:72` | EXACT behaviour required ("Return every span sharing `trace_id` for this tenant. Empty trace returns `Ok(Vec::new())`.") | **REUSE (NO change)** | The substrate seam is precisely the question the lookup arm asks. Per-tenant isolation by the `(TenantId, TraceId)` key (the dual index) makes cross-tenant isolation a property of the substrate (US-05's territory); the HTTP boundary preserves it. The `InMemoryTraceStore::get_trace` adapter at `store.rs:182-192` returns `Ok(Vec::new())` for an unknown `(tenant, trace_id)` pair via `state.by_trace.get(&key).cloned().unwrap_or_default()` (the calm-empty arm for US-02). The `FileBackedTraceStore` adapter implements the same trait method with the same semantics. ADR-0048 Decision 6 (the `TraceStore` trait is UNCHANGED) is HONOURED. |
| `ray::TraceId` (the parsed type) | `crates/ray/src/span.rs:65` (`pub struct TraceId(pub [u8; 16])`) | The `pub` inner array allows direct positional construction from a `[u8; 16]` produced by the parse helper | **REUSE (NO change)** | The `TraceId.0` field is `pub`; the parse helper at the HTTP boundary constructs `TraceId(bytes)` directly after manually decoding 32 hex characters into `[u8; 16]`. The substrate's `Deserialize` impl is NOT used (it would require an indirect path through `serde_json`); the helper hand-rolls the same case-insensitive decode the substrate uses internally. No change to `span.rs`. |
| `parse_min_severity` (severity filter helper, shape precedent) | `crates/log-query-api/src/lib.rs:261-291` | Parse pattern: `fn parse_<thing>(raw: &str) -> Result<<Thing>, String>` where `Err` is the literal reason text used by `error_response` and the raw value is never echoed | **MIRROR pattern; do NOT extract** | This is the SECOND instance of the parse-helper pattern (after `parse_min_severity`). The rule-of-three for extraction into a shared `query-http-common` crate (ADR-0048 Decision 5, M-5) requires three instances. The third instance arrives with this slice (counting the existing `parse_time_range_seconds` in two crates as the first pattern, the `parse_min_severity` as the second, and `parse_trace_id` as the third), and the pressure to extract is now genuinely mounting. Extraction is annotated as DEFERRED for a near-future slice; this slice mirrors the pattern in place to keep the blast radius minimal. |
| `read_required_service` (the required-parameter helper) | `crates/trace-query-api/src/lib.rs:198-204` | Shape precedent for "read a required string parameter; missing or empty is a 400 with a literal reason and no raw value echo" | **REFERENCE only; the new helper does parse + presence in one function** | `parse_trace_id` collapses the presence check and the format check into one helper: any of `None`, `Some("")`, `Some(s) if s.len() != 32`, or `Some(s)` with a non-hex character returns the same `Err("invalid trace_id".to_string())`. The combined helper matches the redaction posture (one class label for the whole class of fault) and is one fewer match in the handler. No edit to `read_required_service`. |
| `ApiState`, `error_response`, `success_response`, `TRACES_ROUTE`, `MAX_RESULT_ROWS`, `MAX_WINDOW_SECONDS` | `crates/trace-query-api/src/lib.rs:64-103, 265-280` | Shared scaffolding consumed by the new handler | **REUSE unchanged** | Six items reused verbatim. No edit. The new handler imports and calls each as-is. `MAX_WINDOW_SECONDS` is inert on the new arm (no window parameter) but the `pub const` stays unchanged for use by the existing handler. |
| `crates/trace-query-api/src/composition.rs` (composition root) | `crates/trace-query-api/src/composition.rs` | The composition root opens the store, resolves the tenant from `KALEIDOSCOPE_TRACE_QUERY_TENANT`, runs the Earned-Trust startup probe, and binds the listener | **UNCHANGED** | The composition root does NOT name the new route or the new handler. The probe is the existing parameter-less empty-range `query` probe (NOT a `get_trace` probe; the lookup arm shares the same `Arc<dyn TraceStore>` and the same fail-closed tenancy seam, so the existing probe already demonstrates the substrate is reachable for both arms). No edit. No new env variable. |
| `crates/trace-query-api/src/main.rs` (thin binary) | `crates/trace-query-api/src/main.rs` | The thin binary calls `router(store, tenant)` and serves it | **UNCHANGED** | The `router` signature is unchanged; the binary needs no edit to mount the new route. |
| `crates/trace-query-api/tests/slice_01_traces_read.rs` (existing acceptance suite) | `crates/trace-query-api/tests/slice_01_traces_read.rs` | The existing 18 scenarios on the window-by-service arm | **UNCHANGED (NOT EDITED)** | DISCUSS scope: the existing arm is untouched; the existing acceptance suite stays green unchanged. The new arm's scenarios land in a sibling test file (DISTILL output). |
| `crates/trace-query-api/tests/common/mod.rs` (test helpers and `FailingTraceStore` double) | `crates/trace-query-api/tests/common/mod.rs` | `tenant(&str)`, `open_durable_store(label)`, `seed(store, tenant, spans)`, `span_with_ids(secs, service, name, trace_byte, span_byte)`, `call(router, request)`, `spans_array`, `is_error_envelope`, `FailingTraceStore` | **EXTEND (add `traces_by_id_request(trace_id)` builder; reuse the rest)** | The new acceptance scenarios reuse every existing helper and `FailingTraceStore` double. A NEW request-builder helper for the by-id URL shape lands in `common/mod.rs` (DISTILL output, NOT DESIGN output); the existing `traces_request(service, start, end)` builder is unchanged. |
| NEW `crates/trace-query-api/tests/slice_02_traces_lookup_by_id.rs` | (does not exist yet) | The new acceptance suite | **CREATE (DISTILL output, NOT this DESIGN's output)** | The acceptance file is created by the DISTILL wave (`@nw-acceptance-designer`); this DESIGN wave records the scenarios it must encode (US-01 through US-05 plus a result-cap 400 scenario per FLAG 3 PIN). |
| `query-http-common` (shared parse-helper crate) | (does not exist) | The deferred extraction target for the parse-helper pattern | **NOT CREATED in this slice** | ADR-0048 Decision 5 / M-5 deferral HONOURED. Annotated: pressure mounts with this slice (third instance of the parse-helper pattern); a near-future slice should extract `parse_min_severity`, `parse_trace_id`, and the shared `parse_time_range_seconds` into a single crate. NOT this slice's scope. |

**Reuse verdict**: the slice is parse + wire inside the existing
`crates/trace-query-api/src/lib.rs`. No new crate. No new external
dependency. No new module under `crates/trace-query-api/src/`. No
new file under `crates/ray/src/`. The CREATE NEW items at the
workspace level are: ADR-0053 (the contract growth),
`docs/feature/trace-lookup-by-id-v0/design/wave-decisions.md`,
`docs/feature/trace-lookup-by-id-v0/design/application-architecture.md`,
and (DISTILL-wave output, NOT DESIGN-wave output) the new
acceptance file
`crates/trace-query-api/tests/slice_02_traces_lookup_by_id.rs`
plus the new request-builder helper in
`crates/trace-query-api/tests/common/mod.rs`. The existing
acceptance suite `tests/slice_01_traces_read.rs` is NOT edited.

## Architecture Summary

- **Pattern**: parse + wire growth on a single existing crate
  (`crates/trace-query-api`); ports-and-adapters preserved (the
  `Arc<dyn TraceStore + Send + Sync>` driven port is the only
  collaborator; the new handler binds to the same port as the
  existing handler). Hexagonal posture honoured.
- **Paradigm**: Rust idiomatic per `CLAUDE.md` — data + free
  functions + traits only where polymorphism is genuinely
  required. The new helper is a free function; the new handler is
  a free `async fn`; the new params struct is data; no class
  hierarchies, no new `dyn` boundary beyond the existing
  `Arc<dyn TraceStore>` indirection (which is the substrate's
  pre-existing genuine polymorphism: durable adapter vs in-memory
  / failing doubles in tests). Composition over inheritance.
- **Key components (no new ones)**: the existing `router`,
  `handle_traces`, `ApiState`, `error_response`,
  `success_response`, `read_required_service`,
  `parse_time_range_seconds`, `parse_epoch_seconds`,
  `seconds_to_nanos`, `TRACES_ROUTE`, `MAX_WINDOW_SECONDS`,
  `MAX_RESULT_ROWS`. The NEW: `TRACES_BY_ID_ROUTE` const,
  `handle_traces_by_id` async fn, `TracesByIdParams` struct,
  `parse_trace_id` fn. All inside one source file.

## Technology Stack

NO change. The slice uses only the existing dependencies of
`crates/trace-query-api`:

- `axum` (existing): the `Router`, `Query` extractor, response
  helpers. No new feature flag.
- `serde` (existing): the `Deserialize` derive on
  `TracesByIdParams`.
- `serde_json` (existing): the `json!` macro for the error
  envelope.
- `tracing` (existing): the `tracing::error!` event on the 500
  arm.
- `ray` (existing): the `TraceStore` trait, the `TraceId` type,
  and the `Span` type (re-exported via `pub use` at
  `crates/ray/src/lib.rs:60-63`).
- `aegis` (existing): the `TenantId` type.

NO new external crate dependency. NO new feature flag on existing
dependencies. NO new build-tool. NO new CI feature.

## Constraints Established

- The new route path is `/api/v1/traces/by_id` (constant in
  `crates/trace-query-api/src/lib.rs`).
- The new query parameter is `trace_id`; the value is exactly 32
  case-insensitive hex characters.
- The error envelope for every malformed-input arm is
  `{"status":"error","error":"invalid trace_id"}` at HTTP 400.
- The error envelope for the no-tenant arm is the existing
  `{"status":"error","error":"no tenant resolvable: the trace
  query service refuses unscoped requests"}` at HTTP 401 (reason
  text REUSED verbatim from the existing handler).
- The error envelope for the result-cap arm is the existing
  `{"status":"error","error":"result exceeds 100000 rows"}` at
  HTTP 400 (reason text REUSED verbatim from the existing
  handler).
- The error envelope for the store-failure arm is the existing
  `{"status":"error","error":"the backing trace store could not
  be read"}` at HTTP 500 (reason text REUSED verbatim from the
  existing handler).
- The success arm is HTTP 200 with the bare JSON array of `Span`s
  in ascending `start_time_unix_nano` order; the empty arm is the
  same shape `[]`. Bare array; no envelope.
- The order of checks is tenancy -> presence -> parse -> store
  -> cap -> serialise. The tenancy fail-closed fires before any
  reflection of input shape leaks to an unscoped caller.
- The `ray::TraceStore` trait is UNCHANGED; the
  `ray::TraceId(pub [u8; 16])` shape is UNCHANGED; the
  `ray::Span` shape is UNCHANGED.
- Gate 2 `cargo public-api` returns zero diff on
  `crates/trace-query-api` and on `crates/ray`.

## DEVOPS Handoff

- **Paradigm**: Rust idiomatic per `CLAUDE.md` (data + free
  functions + traits only where polymorphism is genuinely
  required; composition over inheritance).
- **External integrations**: NONE. The parse helper is in-process
  string matching; the store call uses an in-process trait
  method (`ray::TraceStore::get_trace`) against the durable
  `FileBackedTraceStore`, which is a first-party library, not a
  network service. No third-party API consumed; no
  consumer-driven contract test recommendation.
- **New crate**: NO. The slice is parse + wire inside the
  existing `crates/trace-query-api` crate.
- **New dependency**: NO new third-party crate; no new feature
  flag on existing dependencies.
- **CI gates**: INHERIT ADR-0005's five workspace gates unchanged
  (Gate 1 build, Gate 2 `cargo public-api` byte identity on store
  trait signatures and crate public APIs, Gate 3 unit + doctests,
  Gate 4 acceptance, Gate 5 mutation kill 100% on modified
  files).
- **Mutation-testing scope**:
  `crates/trace-query-api/src/lib.rs` (the only modified source
  file) is COVERED by the existing
  `gate-5-mutants-trace-query-api` workflow via `--in-diff` at
  the 100% kill-rate gate (ADR-0005 Gate 5; `CLAUDE.md`). No new
  CI job. No new workflow file. No new manifest entry. Primary
  mutation targets:
  - The `raw.len() != 32` check in `parse_trace_id` (a `!=` ->
    `<` or `!=` -> `>` mutant must be killed by the boundary
    cases: a 31-char string is rejected, a 32-char string is
    accepted, a 33-char string is rejected).
  - The hex-decode per-byte loop (`(byte as char).to_digit(16)`)
    must reject a non-hex byte at any position; a mutant that
    skips the check at position `i` is killed by US-04's non-hex
    scenarios.
  - The order of checks (tenancy 401 before trace_id presence /
    parse 400) is killed by US-03's no-store-call assertion plus
    a scenario where an unscoped caller submits a malformed
    trace_id: the response is 401 (not 400).
  - The result-cap dispatch (FLAG 3 PIN) is killed by a
    `BulkTraceStore` test double that returns a `Vec<Span>` of
    length 100_001; the response is 400 with `"result exceeds
    100000 rows"`.
  - The cross-tenant isolation (US-05) is killed by the
    two-tenant fixture: the resolved tenant's `get_trace` for a
    trace_id present under the other tenant returns
    `Ok(Vec::new())`; the response is 200 `[]`.
  - The success-arm field fidelity (US-01 boundary scenario) is
    killed by a rich-span fixture that asserts every field of
    every span survives the round trip.
- **Graduation tag**: NONE. No crate boundary changes; no new
  `pub` surface on any crate; no `cargo public-api` diff on
  `crates/trace-query-api` or on `crates/ray`. The slice ships
  on a normal feature commit on `main` per the trunk-based
  posture.
- **Observability**: NO new event, NO new metric, NO new
  dashboard (consistent with ADR-0050 Decision 8 and ADR-0052
  Decision 10: at v0/v1 the platform has no live observability
  stack of its own; the contract IS the signal). The existing
  `traces.store.failed` `tracing::error!` event is REUSED on the
  500 arm of the new handler. The existing `record_query`
  recorder seam on the `InMemoryTraceStore` adapter
  (`crates/ray/src/store.rs:190`) continues to record query
  duration and returned span count for `get_trace`; the
  `FileBackedTraceStore` adapter inherits the same recorder
  surface. No new recorder method; no new label.

## Upstream Changes

None. No DISCUSS assumption was contradicted by the design. The
seven DISCUSS-wave scope decisions (no ray change, calm-empty
`200 []` not 404, fail-closed tenancy at 401, presence-then-parse
of trace_id BEFORE the store, cross-tenant isolation by the
substrate key, redaction on the new 400 arm extending ADR-0048
Decision 2, uniform `MAX_RESULT_ROWS` cap honoured per FLAG 3)
are all preserved.

The single small refinement: DISCUSS US-04 named two literal
class labels for the malformed-trace_id 400 arm (`"trace_id is
required"` for missing, `"invalid trace_id format"` for
malformed). DESIGN PIN collapses these to a single class label
`"invalid trace_id"` for every malformed-input path (missing,
empty, wrong length, non-hex). Rationale: one class label
matches the redaction-by-class-label posture more cleanly (no
information leaks about which kind of malformed input was sent,
which is a small but real anti-fingerprinting benefit), and the
acceptance scenarios in US-04 are satisfied either way (each
scenario asserts the 400 status, the error envelope shape, the
no-store-call assertion, and the absence of the raw value in
the body; none requires a specific class-label substring beyond
the literal reason carried in the envelope). The DISCUSS
scenarios for US-04 will read `"invalid trace_id"` instead of
`"trace_id is required"` / `"invalid trace_id format"` at the
DISTILL wave. The user value (an honest 400 with no echo) is
unchanged.

## Risks (carried from DISCUSS, addressed)

| # | Risk | Status |
|---|---|---|
| R-1 | DESIGN inverts FLAG 1 (extend existing route instead of new path) | NOT INVERTED. New path `/api/v1/traces/by_id` PINNED. The existing acceptance suite stays green unchanged; the new arm lands in a sibling test file. |
| R-2 | DESIGN inverts FLAG 2 (strict case-sensitivity or lenient length) | NOT INVERTED. 32 hex chars, case-insensitive, no aliases. The case-insensitive match aligns with the substrate and the operator's clipboard variance; the length check is exact (`!= 32`). |
| R-3 | DESIGN inverts FLAG 3 (relax the cap on the lookup arm) | NOT INVERTED. Uniform `MAX_RESULT_ROWS = 100_000` PINNED on the new arm. ADR-0050 Decision 3 (REFUSE, never TRUNCATE) preserved. |
| R-4 | DESIGN inverts FLAG 4 (in-place edit of ADR-0048) | NOT INVERTED. New small ADR-0053 PINNED; ADR-0048 cited, NOT modified. |
| R-5 | Scaffold duplication mounts to the third sibling crate | CARRIED. The rule-of-three (M-5; ADR-0048 Decision 5) is now under genuine pressure; the `query-http-common` extraction is annotated as DEFERRED for a near-future slice. NOT THIS SLICE'S SCOPE. |
| R-6 | The composition root's Earned-Trust probe does not exercise `get_trace` | CARRIED, LOW. The existing probe is a parameter-less empty-range `query` probe (per ADR-0048's Earned-Trust posture). `get_trace` shares the same `Arc<dyn TraceStore>`, the same fail-closed tenancy seam, and the same durable adapter (`FileBackedTraceStore`); the existing probe is sufficient evidence that the substrate is reachable for both arms. A dedicated `get_trace` probe is annotated as a NEAR-FUTURE follow-up if any future per-arm fault becomes plausible (e.g. a divergent durable-adapter branch); NOT THIS SLICE'S SCOPE. |

## Handoff to DISTILL

The DISTILL wave (`@nw-acceptance-designer`) inherits:

1. The two DESIGN artefacts in
   `docs/feature/trace-lookup-by-id-v0/design/`:
   `wave-decisions.md` (this file) and
   `application-architecture.md`.
2. The new ADR
   `docs/product/architecture/adr-0053-trace-lookup-by-id.md`.
3. The brief.md application-architecture section appended at
   `docs/product/architecture/brief.md` §
   "Application Architecture — trace-lookup-by-id-v0".
4. The four flags pinned (new path
   `/api/v1/traces/by_id`; 32-hex case-insensitive trace_id;
   uniform `MAX_RESULT_ROWS` cap; ADR-0053) and the parse + wire
   micro-decisions (D5-D9 above).
5. The Gherkin / BDD scenarios from `discuss/user-stories.md`
   (US-01 through US-05) to translate into `#[test]` functions
   in the new
   `crates/trace-query-api/tests/slice_02_traces_lookup_by_id.rs`,
   following the existing `tests/slice_01_traces_read.rs`
   conventions (`open_durable_store`, `tenant`, `seed`,
   `span_with_ids`, `traces_request`-shaped builder for the
   by-id URL, `spans_array`, `is_error_envelope`,
   `FailingTraceStore` double). The single class label `"invalid
   trace_id"` replaces the two DISCUSS-named labels (`"trace_id
   is required"` / `"invalid trace_id format"`) per the
   Upstream Changes note above.
6. The no-store-call assertion on every 400 path and on the 401
   path (via `FailingTraceStore`).
7. The result-cap 400 scenario (FLAG 3 PIN) using a
   `BulkTraceStore`-shaped double whose `get_trace` returns
   `Vec<Span>` of length `MAX_RESULT_ROWS + 1`.

## Handoff to DEVOPS (Apex)

The DEVOPS wave (`@nw-platform-architect`, Apex) inherits:

- **NO new CI job, NO new graduation tag, NO new dependency, NO
  new env variable.**
- The existing `gate-5-mutants-trace-query-api` covers the
  modified file `crates/trace-query-api/src/lib.rs` via
  `--in-diff` at the 100% kill-rate gate (ADR-0005 Gate 5;
  `CLAUDE.md`).
- The existing `gate-2-public-api` confirms the `ray::TraceStore`
  trait signatures are byte-identical to the prior tag and the
  `trace-query-api` `pub` surface (the `router` function,
  `TRACES_ROUTE`, `MAX_WINDOW_SECONDS`, `MAX_RESULT_ROWS`
  `pub const`s) is byte-identical (the new
  `TRACES_BY_ID_ROUTE` const is module-private; the new
  `handle_traces_by_id`, `TracesByIdParams`, `parse_trace_id`
  are module-private).
- No external integration; no consumer-driven contract test
  recommendation.
- The slice ships on a normal feature commit on `main` per the
  trunk-based posture (per project memory: Kaleidoscope is pure
  trunk-based; CI is feedback, not a gate).

## Contradictions with DISCUSS

None of substance. All four DISCUSS-wave flag recommendations
are pinned as recommended. The seven DISCUSS-wave scope decisions
(no ray change, calm-empty 200 not 404, fail-closed tenancy at
401, presence-then-parse before store, cross-tenant isolation by
substrate key, redaction extension to trace_id, uniform cap) are
preserved. The single small refinement (collapse the two
malformed-arm class labels into one) is documented in the
Upstream Changes section above and does not invalidate any
DISCUSS acceptance criterion.
