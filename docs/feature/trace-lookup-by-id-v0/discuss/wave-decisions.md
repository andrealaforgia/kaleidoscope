# Wave Decisions: trace-lookup-by-id-v0

British English throughout. No em dashes.

## Wave context

- **Feature**: `trace-lookup-by-id-v0`
- **Type**: backend, thin slice on `crates/trace-query-api`.
- **JTBD**: not run (per task brief; the feature is a thin slice on an
  existing read API with a single named user outcome).
- **DIVERGE artefacts**: not present (no
  `docs/feature/trace-lookup-by-id-v0/diverge/` directory). The feature
  was scoped directly into DISCUSS by the brief, which is honest for a
  thin slice on top of an already-validated API. Recorded here as a
  noted absence, NOT a blocker; the slice's user job is named in the
  brief and traceable to ADR-0048's operator persona ("Sara Okafor, on
  call for tenant acme-prod"). The brief is itself a job restatement
  in the operator's voice ("an operator with a `trace_id` in hand").
- **Research depth**: lightweight (per task brief).
- **DISCUSS author**: `nw-product-owner` (Luna).
- **DISCUSS date**: 2026-05-27.

## What this feature opens

The trace-query-api crate (`crates/trace-query-api/src/lib.rs`) today
exposes ONE read shape on the `/api/v1/traces` route: a window-by-service
query, `GET /api/v1/traces?service=&start=&end=`. An on-call operator
mid-incident who already holds a specific `trace_id` (from prism, from a
log line, from a frontend error report, from telemetry forwarded by
another system) is forced to filter client-side after a broader
window-and-service query. That client-side filter pays the wire cost of
spans they did not ask for and demands the operator estimate a window
they no longer need to estimate.

The ray substrate already carries the seam that closes this gap.
`ray::TraceStore::get_trace(&tenant, &trace_id) -> Result<Vec<Span>,
TraceStoreError>` exists at `crates/ray/src/store.rs:72` with the right
semantics: "Return every span sharing `trace_id` for this tenant. Empty
trace returns `Ok(Vec::new())`." The durable adapter
`FileBackedTraceStore` implements it; the in-memory adapter implements
it. NO ray change is needed.

The DELIVER is therefore parse-and-wire on the HTTP boundary, the same
shape as `log-query-severity-filter-v0` (which parse-and-wired one
optional `min_severity` parameter onto `query_with` without touching
lumen). Here we parse a `trace_id` and wire it to `get_trace`. The
ray trait is untouched.

## Read-first checklist (artefacts grounded in source code)

The DISCUSS wave was grounded in the following files, all read in full
before any artefact was written:

- [x] `docs/feature/ray-query-api-v0/discuss/user-stories.md` — story
      voice, system constraints, and the operator persona (Sara Okafor)
      this feature continues. The slice 01 walking skeleton format and
      the BDD scenario house style are reproduced.
- [x] `docs/feature/ray-query-api-v0/discuss/story-map.md` — the
      backbone (Receive request -> Resolve tenant -> Resolve query inputs
      -> Query ray -> Render spans) is the same; only step 3 grows a
      second shape and step 4 calls `get_trace` instead of `query`.
- [x] `docs/feature/ray-query-api-v0/discuss/outcome-kpis.md` — the KPI
      template and the CI-realism note (GitHub Actions ubuntu-latest).
- [x] `docs/feature/ray-query-api-v0/discuss/wave-decisions.md` — voice,
      verified-facts layout, contradiction surfacing posture.
- [x] `docs/product/architecture/adr-0048-ray-trace-query-api-contract-and-crate-layout.md`
      — confirmed: the existing `/api/v1/traces` route (Decision 3); the
      `service` parameter is REQUIRED on that route (Decision 1); the
      bare JSON array success shape (Decision 2); the
      `{status:"error", error:"<reason>"}` envelope at 400/401/500
      (Decision 3); the redaction posture stricter than logs (Decision
      2: the body must contain neither "SECRET" nor "Bearer", and never
      the raw `service`/`start`/`end` values); the `TraceStore` trait
      is UNCHANGED in slice 01 (Decision 6) and `get_trace` EXISTS but
      is NOT used by slice 01 of `ray-query-api-v0`. This feature is
      the first HTTP use of `get_trace`.
- [x] `crates/ray/src/lib.rs` — confirmed `TraceStore`, `TraceId`,
      `Span`, `ServiceName`, `TimeRange` are the public surface
      (re-exported via `pub use` at lines 57-64). No change.
- [x] `crates/ray/src/store.rs` — confirmed:
      `get_trace(&self, tenant: &TenantId, trace_id: &TraceId) ->
      Result<Vec<Span>, TraceStoreError>` at line 72; semantics in the
      doc comment line 70-71 ("Return every span sharing `trace_id`
      for this tenant. Empty trace returns `Ok(Vec::new())`"); the
      `InMemoryTraceStore` implementation at lines 182-192 confirms an
      unknown `(tenant, trace_id)` returns `Ok(Vec::new())` via
      `state.by_trace.get(&key).cloned().unwrap_or_default()`. NO ray
      change.
- [x] `crates/ray/src/span.rs` — confirmed:
      `TraceId(pub [u8; 16])` at line 65 (16 bytes, W3C trace context);
      `Serialize` for `TraceId` writes 32 lowercase hex chars at line
      67-71; `Deserialize` for `TraceId` at line 73-87 accepts EXACTLY
      32 hex characters via `hex::decode::<16>` and errors on wrong
      length or non-hex character (line 47-58). The hex codec is
      hand-rolled in-crate and is case-insensitive on the input
      (`to_digit(16)` accepts both `a-f` and `A-F`).
- [x] `crates/trace-query-api/src/lib.rs` — confirmed the handler shape;
      the `TracesParams { service, start, end }` struct at lines 115-120;
      the `read_required_service` 400 arm at lines 197-204; the
      `error_response` envelope at lines 274-280; the
      `success_response(Vec<Span>)` bare-array shape at lines 265-267;
      the `MAX_RESULT_ROWS = 100_000` constant at line 78 with the
      result-cap check at lines 178-180. The existing missing-service
      400 arm is the precedent shape for the new missing-trace_id 400.
- [x] `crates/trace-query-api/tests/slice_01_traces_read.rs` — confirmed
      the acceptance pattern: `tokio::test` + `tower::ServiceExt::oneshot`
      against the `Router`, REAL durable `FileBackedTraceStore` for the
      success and isolation arms, `FailingTraceStore` double for the
      400-before-store and 500 arms.
- [x] `crates/trace-query-api/tests/common/mod.rs` — confirmed the
      reusable helpers: `tenant(&str)`, `open_durable_store(label)`,
      `seed(store, tenant, spans)`, `span_with_ids(secs, service, name,
      trace_byte, span_byte)`, `call(router, request)`, `spans_array`,
      `is_error_envelope`, and the `FailingTraceStore` double. NEW
      request-builder helpers for the by_id shape will land here.
- [x] `crates/log-query-api/src/lib.rs` — confirmed the
      symmetric-dispatch pattern just shipped for severity (`match
      min_severity { Some(floor) => query_with(...), None => query(...) }`
      at lines 169-176). The same dispatch shape is the precedent for
      "if `trace_id` present, call `get_trace`; else call `query`" IF
      DESIGN picks endpoint-shape option (b). If DESIGN picks endpoint
      shape (a) (new path), the dispatch lives at the router level
      (two routes, two handlers).
- [x] `docs/product/architecture/adr-0050-earned-trust-read-side-caps.md`
      — confirmed: `MAX_RESULT_ROWS = 100_000` applies uniformly to all
      three read APIs (Decision 2); the result-cap check is AFTER the
      store returns and BEFORE serialisation, and breach is a NAMED 400,
      never a truncated 200 (Decision 3). For trace lookup the cap is
      plausibly never hit by a well-shaped trace, but the contract is
      uniform: a misbehaving client (deep recursive instrumentation, an
      attacker, a stray test harness) could in principle produce a
      single trace bigger than the cap. The cap still fires; FLAG 3.
- [x] `docs/product/architecture/adr-0052-log-query-severity-filter.md`
      — confirmed the recent shape for "small ADR for a thin slice
      growing an existing read contract by one parameter": a small,
      back-referencing ADR (Decision 1: parameter naming; Decision 2:
      accepted value shape; Decision 3: filter semantics; Decision 4:
      cap interaction; Decision 5: handler check order). FLAG 4 below
      recommends the same posture for this slice (ADR-0053).

## Verified facts (stories are grounded in these, read 2026-05-27)

1. `ray::TraceStore::get_trace(&tenant, &trace_id) -> Result<Vec<Span>,
   TraceStoreError>` exists today at `crates/ray/src/store.rs:72`. The
   semantics in the doc comment: "Return every span sharing `trace_id`
   for this tenant. Empty trace returns `Ok(Vec::new())`." Per-tenant
   isolation by the `(TenantId, TraceId)` key in the dual index.
2. `InMemoryTraceStore::get_trace` (`store.rs:182-192`) returns
   `Ok(Vec::new())` for an unknown `(tenant, trace_id)` key via
   `state.by_trace.get(&key).cloned().unwrap_or_default()`. The empty
   arm is honest: empty trace is `Ok([])`, not an error.
3. `ray::TraceId(pub [u8; 16])` (`span.rs:65`). 128-bit W3C trace
   context identifier. Serialises as 32 lowercase hex characters.
4. The hex codec at `span.rs:32-61` is case-insensitive on the input
   (`(byte as char).to_digit(16)` accepts both `a-f` and `A-F`). The
   length check rejects strings whose length is not exactly `N * 2`
   (line 47), so a 16-character span_id-shaped string sent to the
   trace_id slot is rejected with a length error, NOT silently
   accepted as a short trace_id.
5. `ray::Span` already derives `serde::Serialize`. The handler's
   existing `success_response(Vec<Span>)` (`lib.rs:265`) returns a
   bare JSON array of spans. The same body shape is the only
   defensible success shape for the lookup arm; field fidelity is
   the same `Span` machinery.
6. The existing handler error envelope (`error_response`,
   `lib.rs:274-280`) is `{"status":"error","error":"<reason>"}` at
   the chosen status code. ADR-0048 Decision 2 pins the redaction:
   the body must contain neither "SECRET" nor "Bearer", and never the
   raw `service`/`start`/`end` values. The new error arms for
   trace_id (missing, empty, malformed) reuse the envelope verbatim
   and extend the redaction: the raw `trace_id` value is NOT echoed.
7. `MAX_RESULT_ROWS = 100_000` is the uniform result-size cap (ADR-0050
   Decision 2; `lib.rs:78`). The cap fires AFTER `TraceStore::get_trace`
   returns and BEFORE serialisation. A `Vec<Span>` whose length is
   strictly greater than 100_000 is refused with the named 400; the
   serialisation never starts.
8. `MAX_WINDOW_SECONDS = 86_400` (`lib.rs:71`) is the window cap on the
   EXISTING `(service, start, end)` shape. For the `trace_id` lookup
   shape there is NO window: a trace is the universe of spans sharing
   that id, irrespective of when they were written. The window cap is
   inert on the lookup arm; it remains in force on the existing arm.
9. ADR-0048 Decision 1: the existing route accepts `service` as a
   REQUIRED parameter. A `trace_id`-shaped lookup is a different query
   inputs shape (no service, no window). DESIGN chooses whether the new
   shape lives on a NEW path or coexists on the SAME path with a
   conditional override; FLAG 1 below.
10. ADR-0048 Decision 6: the ray `TraceStore` trait is UNCHANGED in slice
    01 of `ray-query-api-v0`. This feature reuses `get_trace`, which the
    trait already exposes; the trait remains UNCHANGED in this feature
    too. No ray change.
11. The aperture write path persists spans into `FileBackedTraceStore`
    indexed by both `(tenant, trace_id)` and `(tenant, service)`
    (`store.rs:97-101` and `file_backed.rs`-equivalents verified via the
    `InMemoryTraceStore` adapter). The lookup arm rides the
    trace-id-keyed index.
12. The test posture mirrors the existing `slice_01_traces_read.rs`
    suite: `tokio::test` + `oneshot` against the `Router`, REAL durable
    `FileBackedTraceStore` for success and isolation, `FailingTraceStore`
    for 400-before-store proofs and the 500 arm.

## Key decisions taken in DISCUSS

1. **The lookup arm is a new way to ask, not a new way to answer.** The
   response shape stays a bare JSON array of `Span`s in ascending
   `start_time_unix_nano` order, the same shape the existing window
   read returns. There is no envelope. There is no parent/child
   topology. There is no per-service projection. The user value is the
   QUESTION ("give me trace X"), not a new presentation; trace
   assembly remains explicitly deferred.

2. **Unknown trace_id is `200 []`, NEVER `404`.** A `404` would mean
   "the endpoint at this URL does not exist"; the endpoint DOES exist
   and is responding. "The trace was never seen for this tenant" is
   the calm-empty arm, semantically identical to "the window has no
   spans" on the existing route. ADR-0048 Decision 2 already pins the
   bare JSON array shape and the empty-arm-is-200 posture for the
   existing route; the lookup arm honours the same posture for the
   same reason. This is a HARD decision in DISCUSS; DESIGN does not
   get to revisit it.

3. **Tenant fail-closed, identical to the existing route.** The
   resolved-tenant seam at `lib.rs:134-142` is reused. A request with
   no resolvable tenant is refused (HTTP 401), trace_id or not. No
   trace_id is read against an unresolved tenant.

4. **trace_id validation is the FIRST validation, even before tenant
   resolve.** Actually: the existing order is (1) resolve tenant
   (401 fail-closed) -> (2) read required parameter -> (3) parse and
   validate -> (4) cap -> (5) store. The same order is honoured by the
   lookup arm: 401 fail-closed first, then 400 for missing/malformed
   trace_id. This preserves the existing audit trail and avoids
   leaking "the trace_id is valid" timing to an unscoped caller.

5. **Cross-tenant isolation: a trace_id present for tenant A is `200 []`
   for tenant B.** The `(tenant, trace_id)` index keys the trace under
   the writing tenant; a different tenant's lookup against the same
   trace_id MUST return the empty arm (not a 404, not a 200 with the
   other tenant's spans). The test posture is identical to the
   existing US-03 cross-tenant arm.

6. **The error envelope for malformed trace_id is the existing envelope,
   with the existing redaction posture extended.** Reason names the
   fault ("invalid trace_id format"), never echoes the raw value, and
   the body contains neither "SECRET" nor "Bearer". The reason text is
   a literal class label, not a hex-encoded ban list.

7. **The cap still applies.** `MAX_RESULT_ROWS = 100_000` fires on a
   `get_trace` result whose length strictly exceeds the cap, with the
   same named 400 the existing route returns ("result exceeds 100000
   rows"). A trace is plausibly far under the cap (a typical web
   request trace is dozens of spans, an unusually deep one hundreds);
   the cap is defensive against pathological clients, not an expected
   path. The window cap is inert on the lookup arm (no window). FLAG 3
   confirms.

## Flags to DESIGN (recommendations stated, decisions deferred)

### FLAG 1 — Endpoint shape

Two defensible options. Both ride `TraceStore::get_trace` and produce
identical responses; the difference is URL surface.

- **Option (a) — new path**:
  `GET /api/v1/traces/by_id?trace_id=<32-char-hex>`. The existing route
  `/api/v1/traces` keeps its current contract (window + service)
  unchanged. The new route serves the lookup arm only. The two handlers
  are independent and the existing acceptance suite is untouched.
- **Option (b) — extend the existing route**: keep `/api/v1/traces`,
  add a `trace_id` query parameter. When `trace_id` is present it
  OVERRIDES `service`+`start`+`end` (which become unread); when absent
  the existing behaviour is identical to today. One endpoint, two
  shapes, dispatched on the presence of `trace_id`.

**Recommendation: option (a), the new path.** Three reasons:

1. **Honesty about query inputs.** The two shapes ask different
   questions (one a window-by-service filter, the other a single-key
   lookup). A separate path makes that honest at the URL; an extended
   parameter hides one question inside the body of the other and
   asks the operator to read the precedence rule to know what they
   asked for.
2. **No silent fall-through on the existing route.** Option (b) needs
   a precedence rule ("trace_id wins"). A request that fat-fingers
   `service` and accidentally supplies a stale `trace_id` would be
   served by the lookup arm against the operator's expectation. Option
   (a) makes the operator name the shape in the URL.
3. **No risk of the existing acceptance suite shifting under DELIVER.**
   The existing 18 scenarios in `slice_01_traces_read.rs` keep firing
   verbatim; the new path adds its own scenarios in a sibling test
   file. Easier to keep slice 01 of `ray-query-api-v0` PASSING through
   DELIVER.

Option (b) is reasonable; it is the lumen severity filter precedent
("one optional parameter on the existing route"). The asymmetry here
is that severity is a FILTER on the same query shape, while trace_id
is a DIFFERENT query shape. DESIGN owns the call.

### FLAG 2 — `trace_id` wire format

OTel pins `trace_id` to a 128-bit value rendered as exactly 32
lowercase hex characters (W3C trace context). The existing
`ray::TraceId` deserialiser at `span.rs:73-87` and the hex codec at
`span.rs:42-60` accept BOTH cases (`a-f` and `A-F`) for the four
characters per byte, and REQUIRE the length be exactly 32.

- **Option (a)**: accept 32 hex characters, case-insensitive on the
  hex digits (rendered lowercase on the way back when echoed in a
  span). Reject any other length, including the W3C span_id length
  (16 chars).
- **Option (b)**: also accept 16-character W3C span_id values (helpful
  to an operator who fat-fingers and pastes a span_id by mistake) and
  reject other lengths. Returns 400 with "invalid trace_id format:
  span_ids are 16 characters, not 32" or similar diagnostic.
- **Option (c)**: 32 lowercase hex characters only, reject uppercase.

**Recommendation: option (a), 32-char hex case-insensitive on the hex
digits.** Three reasons:

1. **Matches the precedent.** The lumen severity filter accepted
   case-insensitive on the six OTel names (ADR-0052 Decision 2). The
   operator's clipboard tends to vary in case across logging
   pipelines; rejecting "FFFF..." while accepting "ffff..." is
   user-hostile for a parameter that names the same identifier either
   way.
2. **Aligns with the ray substrate.** `hex::decode::<16>` already
   accepts both cases; the HTTP boundary should not invert or relax
   the substrate's rule.
3. **Reject other lengths CRISPLY, including the 16-char span_id
   mistake.** Option (b)'s clever diagnostic is tempting but echoes a
   property of the raw value into the error text. The redaction
   posture (ADR-0048 Decision 2) says no raw values in error text.
   Option (a) keeps the error a literal class label ("invalid
   trace_id format") and lets the operator look up the correct shape
   in the OTel spec. FLAG 2 still surfaces this trade-off for DESIGN.

### FLAG 3 — Cap interaction

`MAX_RESULT_ROWS = 100_000` (ADR-0050 Decision 2). A typical trace is
dozens of spans; a pathologically deep instrumentation or an attacker
could in principle produce a single trace whose span count exceeds
the cap.

- **Option (a)**: the cap fires uniformly, the same NAMED 400
  ("result exceeds 100000 rows") the existing route returns.
- **Option (b)**: relax the cap on the lookup arm because a trace is
  semantically one unit and partial-trace is misleading.

**Recommendation: option (a), uniform cap.** Two reasons:

1. **ADR-0050 Decision 3 is "REFUSE, never TRUNCATE".** Relaxing the
   cap on the lookup arm makes the contract asymmetric (the other two
   routes refuse over-cap, this one does not) and invites the
   misbehaving client. The cap is defensive, not aspirational.
2. **The cap is plausibly never hit in practice for a well-shaped
   trace.** The cost of preserving the uniform refusal is zero on the
   happy path and a clear 400 on the pathological path. The
   asymmetry of option (b) buys little for a non-existent customer.

FLAG 3 is the call DESIGN may revisit; the recommendation is the
conservative one.

### FLAG 4 — ADR-0053 versus refinement of ADR-0048

The slice is the first HTTP use of `get_trace` and the first additional
shape on the trace-query-api crate since ADR-0048. Two options:

- **Option (a)**: a small ADR-0053 ("trace lookup by id"), back-
  referencing ADR-0048 in the same shape ADR-0052 back-referenced
  ADR-0047. Decisions: endpoint shape, wire format, cap interaction,
  envelope reuse, handler check order. Six small decisions.
- **Option (b)**: in-place edit of ADR-0048 to add the lookup arm.

**Recommendation: option (a), small ADR-0053.** ADRs in this
repository are immutable (the convention set by ADR-0001 and honoured
by every ADR including ADR-0049, ADR-0050, ADR-0051, ADR-0052). The
slice grows the read contract; the growth lands as a new ADR with a
back-reference, NOT as an in-place edit. ADR-0053 is the next free
number (ADR-0052 is the latest).

## Coherence check

- **CLI vocabulary**: this is an HTTP backend; "CLI vocabulary" maps to
  parameter names. The new parameter is `trace_id`. The name matches
  the OTel spec field name verbatim, matches `ray::TraceId`, matches
  `Span::trace_id` in the serialised body, and reads as a noun the
  operator already knows.
- **Voice across stories**: Sara Okafor stays the persona. The
  emotional arc remains "anxious mid-incident -> calm with the spans
  in hand". Each story restates the SAME outcome from a different
  angle (happy, calm-empty, fail-closed, malformed, cross-tenant).
- **Domain language**: `trace_id`, `tenant`, `span`, `service` are
  named in the story bodies and the BDD scenarios. No marketing words.
- **Shared artefacts**: the trace_id parameter name and the
  `/api/v1/traces` route prefix appear in tests, in the handler, and
  in operator-facing docs. Single source of truth: ADR-0053 once
  drafted; until then, this `wave-decisions.md` and the slice
  artefact.

## Contradictions surfaced

None. The brief, the verified ray surface, ADR-0048, and ADR-0050 are
mutually consistent. The slice does not contradict any accepted ADR.

Specifically checked:

- **ADR-0048 Decision 1 (the existing `/api/v1/traces` requires
  `service`)**: NOT contradicted. Under FLAG 1 option (a) the new path
  is `/api/v1/traces/by_id` and has its own parameter rule; the
  existing route still requires `service`. Under option (b) the
  precedence rule ("trace_id wins, service unread") is an ADDITIVE
  growth, not a contradiction.
- **ADR-0048 Decision 6 (the ray `TraceStore` trait is UNCHANGED)**:
  NOT contradicted. `get_trace` already exists on the trait; the slice
  uses it for the first time on the HTTP boundary.
- **ADR-0050 Decision 2 (uniform result-size cap)**: NOT contradicted.
  FLAG 3 recommends option (a), the cap applies uniformly.
- **ADR-0050 Decision 3 ("REFUSE, never TRUNCATE")**: NOT
  contradicted. The lookup arm refuses with a NAMED 400 on cap breach.

## OUT of scope for slice 01 (deferred, declared)

- Tree assembly: root + children topology, parent/child resolution
  beyond the bare-array shape. The response is a plain array; the
  operator (or prism) does the assembly client-side. Trace assembly
  is a separate later slice, not in this feature.
- Per-service filter inside a trace. A trace can carry spans from
  multiple services; a filter such as `?service=checkout` on the
  by_id arm is a later slice.
- Batch lookup of multiple trace_ids. `?trace_id=A,B,C` and similar
  shapes are a later slice; slice 01 is one trace_id per request.
- Complex URL path routing (e.g. `GET /api/v1/traces/<trace_id>`
  REST-style). FLAG 1's options are query-string shapes; a path-
  segment shape is a separate flag a later slice can revisit.
- Cap relaxation specifically for the lookup arm. FLAG 3 keeps the
  cap uniform; a per-arm cap is a later decision with measurement
  data the v0 platform does not yet have.

## Risks (surfaced, not managed here)

- **Surface drift between the existing acceptance suite and the new
  arm.** Mitigation: under FLAG 1 option (a) the existing 18
  scenarios are untouched; the new scenarios land in a sibling test
  file.
- **Operator pasting a span_id by mistake.** Mitigation: the 16-char
  rejection is crisp and the error names the class. FLAG 2 weighs
  whether to make the diagnostic more helpful at the cost of
  redaction posture.
- **The DIVERGE artefacts are absent.** Recorded; the slice's user job
  is named in the brief and traceable to ADR-0048's operator persona,
  so the absence is a noted gap, not a blocker.

## Handoff inputs to DESIGN

- This `wave-decisions.md` (the four flags, the recommendations, the
  contradiction check).
- `user-stories.md` (five stories with Elevator Pitch After lines
  naming the real HTTP entry point and BDD scenarios).
- `story-map.md` (the backbone, walking skeleton, priority rationale).
- `outcome-kpis.md` (the four KPIs and the CI realism note).
- `../slices/slice-01-trace-lookup-by-id.md` (the walking-skeleton
  scenario, the learning hypothesis, the in / out scope tables).
