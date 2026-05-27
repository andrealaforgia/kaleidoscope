# ADR-0053 — trace-query-api lookup-by-id sibling route

- **Status**: Accepted
- **Date**: 2026-05-27
- **Author**: `nw-solution-architect` (Morgan)
- **Feature**: `trace-lookup-by-id-v0`
- **Supersedes**: none
- **Superseded by**: none
- **Related**: ADR-0048 (the ray trace-query-api contract and crate
  layout; Decision 1 pins the existing `/api/v1/traces` route and its
  required `service`; Decision 2 pins the bare-JSON-array success
  shape, the `{status:"error", error}` error envelope, and the
  redaction posture this ADR reuses verbatim; Decision 6 records the
  `TraceStore` trait is UNCHANGED and that `get_trace` EXISTS but is
  NOT used in slice 01 of `ray-query-api-v0`; cited as the originating
  contract this ADR GROWS by one sibling path, NOT modified). ADR-0050
  (the read-side Earned-Trust caps; Decision 2 places `MAX_RESULT_ROWS
  = 100_000` uniformly across the three read crates, AFTER the store
  returns and BEFORE serialisation; Decision 3 fixes REFUSE-not-TRUNCATE;
  Decision 4 places the cap on the user-observed vector; cited as the
  cap interaction precedent on the new arm, NOT modified). ADR-0052
  (the stylistic sibling; the layout of a small ADR for a parse-and-wire
  thin slice growing an existing read contract is mirrored here; cited
  as the style precedent, NOT modified).

## Context

The ray trace-query-api (`crates/trace-query-api/src/lib.rs`, route
`GET /api/v1/traces`) requires a `service` parameter and answers a
window-by-service question (ADR-0048 Decision 1). For an on-call SRE
holding a trace_id from a log line, an alert payload, or a parent
service's response header, "every in-window trace for service X" is
the wrong question; the operator's question is single-key, "give me
every span sharing this trace_id for my tenant". The substrate
already carries the seam:
`ray::TraceStore::get_trace(&tenant, &trace_id) -> Result<Vec<Span>,
TraceStoreError>` exists at `crates/ray/src/store.rs:72` and is
honoured by both the `InMemoryTraceStore` and the durable
`FileBackedTraceStore` adapters; the per-tenant `(TenantId, TraceId)`
key gives cross-tenant isolation as a property of the substrate, not
the boundary. ADR-0048 Decision 6 stated the seam exists but is NOT
used in slice 01 of `ray-query-api-v0`; this slice is the first
HTTP-boundary use.

ADRs in this repository are immutable (the convention is set by
ADR-0001 and honoured by every preceding ADR including ADR-0049 /
ADR-0050 / ADR-0051 / ADR-0052). The contract is growing by one new
sibling path on the existing crate (a new accepted parameter
`trace_id`, a new 400 reason class `"invalid trace_id"`, a new
order-of-checks on a sibling route); the growth therefore lands as
a new ADR with back-references to ADR-0048 and ADR-0050, neither
modified. ADR-0053 is the next free number (`ls
docs/product/architecture/adr-0053*` returns no hits;
`adr-0052-log-query-severity-filter.md` is the latest).

## Decision

### 1. New separate path `/api/v1/traces/by_id` (FLAG 1)

The lookup arm is mounted as a sibling route on the existing
`router`, NOT as a branched dispatch inside the existing
`handle_traces`. The new route constant is `TRACES_BY_ID_ROUTE =
"/api/v1/traces/by_id"`; the new handler is `handle_traces_by_id`;
the existing `TRACES_ROUTE` and `handle_traces` are UNCHANGED. The
two routes share `ApiState { store, tenant }`. Separation of
concerns at the URL surface: the two routes ask two different
questions (window-by-service vs single-key lookup); a request that
fat-fingers `service` and supplies a stale `trace_id` is refused
out loud on the existing route rather than silently served against
the operator's expectation by a precedence rule in a unified
handler. The existing 18 acceptance scenarios in
`tests/slice_01_traces_read.rs` keep firing verbatim.

### 2. `trace_id` wire format: 32 hex characters, case-insensitive (FLAG 2)

The accepted shape is exactly 32 hex characters (`[0-9a-fA-F]{32}`),
matching the OTel / W3C trace context spec (128-bit trace_id = 16
bytes = 32 hex chars) and matching the substrate codec at
`crates/ray/src/span.rs:42-60` which accepts both `a-f` and `A-F`
via `(byte as char).to_digit(16)`. Length != 32 returns 400; any
non-hex character returns 400; empty returns 400; missing returns
400. The literal class label `"invalid trace_id"` is the entire
error reason on every malformed-input path. The raw parameter
value is NEVER echoed (redaction; ADR-0048 Decision 2 extended to
the new parameter). The clever diagnostic "span_ids are 16 chars,
not 32" is rejected — it leaks a property of the raw value into
the error text; the operator's path to the right shape is the OTel
spec, not a clever 400.

### 3. Uniform `MAX_RESULT_ROWS` applies to the lookup arm (FLAG 3)

ADR-0050 Decision 2 (`MAX_RESULT_ROWS = 100_000`, uniform across
the three read crates), Decision 3 (REFUSE, never TRUNCATE), and
Decision 4 (the cap measures what the user observes) are all
PRESERVED on the new arm. The result-cap check fires AFTER
`store.get_trace` returns and BEFORE serialisation; `spans.len() >
MAX_RESULT_ROWS` returns 400 with the existing reason text
`"result exceeds 100000 rows"`. A typical trace is dozens of spans
and the cap is plausibly never hit on the happy path; a misbehaving
client (pathologically deep recursive instrumentation, a stray test
harness, a deliberate replay attack) producing a single trace
whose span count exceeds the cap is refused out loud, same as the
existing window arm and the sibling `query-api` / `log-query-api`
arms. NO window cap (the lookup arm has no `start` / `end`
parameters; `MAX_WINDOW_SECONDS` is inert on this arm).

### 4. ADR-0053 is the durable record of the contract growth (FLAG 4)

The new sibling path, the new accepted parameter, the new 400
reason class, and the order-of-checks on the new arm are recorded
here. ADR-0048 and ADR-0050 are CITED, NOT modified. Operators
reading the read-side trace contract land on ADR-0048 (the
window-by-service arm) and on ADR-0053 (the lookup-by-id arm) as
the two complementary records; ADR-0050 governs the cap interaction
on both arms.

## Consequences

### Positive

- The operator's single-key trace-pivot job is served at the HTTP
  boundary, not by curl-and-grep against the window arm with a
  service guess. KPI-1 (one HTTP call from trace_id to spans) is
  the measurable target.
- The existing route, its parameters, and its 18 acceptance
  scenarios are UNCHANGED; KPI-2 (zero broken clients on the
  existing arm) is guaranteed by construction.
- The redaction posture (ADR-0048 Decision 2) is preserved on the
  new arm: the raw `trace_id` parameter value is NEVER echoed; the
  one literal class label `"invalid trace_id"` covers every
  malformed-input path.
- The cap interaction (ADR-0050 Decision 2, Decision 3, Decision 4)
  is preserved on the new arm: REFUSE-not-TRUNCATE; cap fires
  AFTER the store and BEFORE serialisation; cap measures what the
  user observes.
- No new envelope, no new status code, no new tag, no new external
  dependency, no new module, no new file under `crates/ray/src/`,
  no change to `ray::TraceStore` trait signatures (Gate 2 `cargo
  public-api` confirms).
- Fail-closed tenancy and cross-tenant isolation are inherited
  from the substrate's `(TenantId, TraceId)` key; the boundary
  preserves them.

### Negative

- `~10` LOC of scaffolding are duplicated across the two handlers
  in the same source file (the tenancy match, the
  `error_response` / `success_response` calls, the result-cap
  check). The duplication is bounded, mutation-tested in place,
  and now under genuine rule-of-three pressure (M-5 in ADR-0048
  Decision 5; the third instance of the parse-and-wire pattern
  after `query-api`, `log-query-api`, and the original
  `trace-query-api`). The `query-http-common` extraction is
  annotated as DEFERRED for a near-future slice; this slice does
  not pay the extraction cost speculatively.
- The lookup arm is single-key, not range or set; an operator
  who has a list of trace_ids must issue one request per id at
  slice 01. Bulk lookup is explicitly OUT of scope and DEFERRED.
- The result cap is invisible until tested. An operator cannot
  distinguish "the cap fired" from "the trace genuinely has
  this many spans" without reading the source; the acceptance
  scenario for the cap-400 on the lookup arm pins it observably.

## Alternatives considered

### (a) Rejected: branched dispatch on the existing route

Add `trace_id: Option<String>` to `TracesParams`; if present,
ignore `service` / `start` / `end` and answer the lookup; if
absent, answer the window. For: one route, one handler. Against:
silent fall-through on a fat-fingered request where both
`service` and `trace_id` are present is operator-hostile (which
question is answered?); a precedence rule in the body of one
handler hides the contract growth at the URL surface; the
existing 18 acceptance scenarios would need re-reading for the
precedence rule. Rejected.

### (b) Rejected: span_id as the sole lookup key

Accept a 16-hex `span_id` instead of (or alongside) the
`trace_id` and answer "give me the trace containing this span".
For: aligns with stack-trace-style debugging where the
operator's clipboard often carries a span_id. Against: the
substrate seam exposed at slice 01 is `get_trace(tenant,
trace_id)`, not `get_trace_containing_span(tenant, span_id)`;
the latter would require either a new `TraceStore` method (out
of scope) or a full-scan in the adapter (no per-tenant index by
span_id today). The trace_id surface is the operator's
clipboard for the OTel-native debug path. Rejected.

### (c) Rejected: in-place edit of ADR-0048

Grow ADR-0048 with a new Decision 7 documenting the lookup arm.
For: the contract lives in one ADR. Against: ADRs in this
repository are immutable (ADR-0001 convention honoured by every
preceding ADR including ADR-0049 / ADR-0050 / ADR-0051 /
ADR-0052); the amendment would create a precedent that drifts.
Rejected.

## References

- ADR-0048 (trace-query-api contract and crate layout) — CITED,
  NOT modified. Decision 1 (existing route requires `service`),
  Decision 2 (envelope and redaction posture), Decision 5 (M-5
  `query-http-common` rule-of-three deferral), Decision 6
  (`TraceStore` trait unchanged; `get_trace` exists but unused
  in slice 01 of `ray-query-api-v0`).
- ADR-0050 (read-side Earned-Trust caps) — CITED, NOT modified.
  Decision 2 (`MAX_RESULT_ROWS = 100_000` uniform), Decision 3
  (REFUSE not TRUNCATE), Decision 4 (cap measures what the user
  observes).
- ADR-0052 (log-query-api `min_severity` filter parameter) —
  CITED as the immediate stylistic sibling for the layout of a
  small ADR on a parse-and-wire thin slice growing an existing
  read contract. NOT modified.
- `docs/feature/trace-lookup-by-id-v0/discuss/user-stories.md`
- `docs/feature/trace-lookup-by-id-v0/discuss/wave-decisions.md`
- `docs/feature/trace-lookup-by-id-v0/discuss/story-map.md`
- `docs/feature/trace-lookup-by-id-v0/discuss/outcome-kpis.md`
- `docs/feature/trace-lookup-by-id-v0/design/wave-decisions.md`
- `docs/feature/trace-lookup-by-id-v0/design/application-architecture.md`
