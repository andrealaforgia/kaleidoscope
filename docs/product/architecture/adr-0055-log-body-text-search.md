# ADR-0055 — log-query-api `body_contains` filter parameter and `lumen::Predicate` body-substring arm

- **Status**: Accepted
- **Date**: 2026-05-27
- **Author**: `nw-solution-architect` (Morgan)
- **Feature**: `log-body-text-search-v0`
- **Supersedes**: none
- **Superseded by**: none
- **Related**: ADR-0047 (the lumen log-query-api contract; this
  ADR grows the contract by one optional parameter on the same
  route, with the bare-JSON-array success shape and the
  `{status:"error", error}` envelope reused verbatim; cited, NOT
  modified). ADR-0050 (the read-side Earned-Trust caps; Decision
  4 places the result-cap AFTER the store returns and BEFORE
  serialisation on the post-filter `Vec<LogRecord>`; this ADR
  slots the body-substring filter at the store call so the cap
  continues to measure what the user observes; cited, NOT
  modified). ADR-0052 (the immediate sibling — `min_severity`
  optional parameter on the same route; this ADR follows the
  same parse-and-wire shape, composes conjunctively with
  `min_severity` via `Predicate::matches`, and pins the
  filter-BEFORE-cap interaction in the same posture; cited, NOT
  modified). ADR-0054 (the M-5 `query-http-common` extraction;
  this slice is the FIRST consumer of the shared scaffold born
  AFTER the extraction shipped — the cap consts, the four
  `REASON_*` consts, the `error_response` helper, the
  `resolve_tenant_or_refuse` seam, and the `parse_time_range`
  parser are ALL reused via `query_http_common::`; cited, NOT
  modified).

## Context

The lumen log-query-api (`crates/log-query-api/src/lib.rs`, route
`GET /api/v1/logs?start=&end=`) returns every in-window
`LogRecord` for the resolved tenant as a bare JSON array
(ADR-0047 Decision 1). ADR-0052 grew the contract with the
optional `min_severity` floor; an on-call SRE mid-incident can
now ask for "WARN and above" without piping through `jq`. The
floor narrows by severity but not by content: an operator with a
specific error string in hand (paging alert text, runbook step,
customer report) still receives every WARN-or-above record in
the window, including unrelated ERROR records from a different
incident in the same minute. The typical workaround is
`curl ... | jq 'map(select(.body | contains("kafka timeout")))'`,
which pays the full payload cost client-side and the full
bandwidth cost on the wire.

The lumen substrate carries the seam to push the predicate into
the store: `lumen::LogStore::query_with(&tenant, range, predicate)`
exists at `crates/lumen/src/store.rs:89`; both adapters
(`InMemoryLogStore` at `crates/lumen/src/store.rs:159-180` and
`FileBackedLogStore` at `crates/lumen/src/file_backed.rs:229-250`)
already route per record through `predicate.matches(r)`. The
predicate today (`crates/lumen/src/predicate.rs:25-28`) carries
`service: Option<String>` and `min_severity: Option<SeverityNumber>`
only; a body-substring filter requires the predicate to grow one
additive field.

The `LogRecord.body` field is a plain `String` at
`crates/lumen/src/record.rs:54`; `String::contains(&str)` is the
substring primitive, byte-wise and case-sensitive by default.
The slice is therefore a parse-and-wire growth of the HTTP read
contract — ONE optional query-string parameter on
`GET /api/v1/logs` whose value is a byte-substring matched
against `body` — combined with ONE additive lumen surface change
(one field, one builder, one `matches` arm, one `is_empty`
clause). No new lumen module. No new envelope. No new status
code. No new tag. No new external dependency.

ADR-0047 is the read-side contract for logs; growing it by one
optional parameter is a contract change. ADRs in this repository
are immutable (the convention is set by ADR-0001 and honoured by
every preceding ADR including ADR-0052 and ADR-0054). The growth
therefore lands as a new ADR with back-references to ADR-0047,
ADR-0050, ADR-0052, and ADR-0054, not as an in-place edit.
ADR-0055 is the next free number (`ls
docs/product/architecture/adr-0055*` returns no hits; ADR-0054
is the latest).

## Decision

### 1. Wire parameter name: `body_contains` (FLAG 1)

The query-string parameter on `GET /api/v1/logs` is spelled
`body_contains`. The name aligns with the filter semantics
(`String::contains` over the `body` field) and reads as a
natural-language phrase ("body contains X"). A typo or unknown
shape is irrelevant at slice 01 because the parameter accepts
any non-empty bounded `String`; only the empty value and the
over-cap value are rejected (Decisions 4 and 5).

### 2. Substring matching, NOT regex (FLAG 1 from DISCUSS)

Slice 01 ships `String::contains(&str)` byte-substring matching
ONLY. Regex matching is a separate future slice
(`log-body-regex-search-vN`) with its own ReDoS budget, its own
expression-grammar contract (PCRE vs RE2 vs the `regex` crate),
and its own performance posture. Substring matching is the
simplest predicate over a `String` field, the most predictable
in cost, and the lowest-surprise default.

### 3. Case-sensitive matching, byte-wise (FLAG 2 from DISCUSS)

The match is byte-wise via `String::contains`:
`body_contains=KAFKA` does NOT match a record whose body is
`kafka timeout`. Rationale: grep is operator muscle memory and
is case-sensitive by default; a case-folding default risks
false-positive matches on platform boilerplate (a customer
substring `INFO connection refused` should NOT match the
platform's own `severity_text: "INFO"` text). Case-insensitive
matching is a future slice (a separate `body_contains_ci=<string>`
parameter or a `case_sensitive=false` flag); slice 01's KPI-4
acceptance test pins the case-sensitive rule with a named
scenario so operators learn the posture from where they will
look.

### 4. Empty `body_contains` is a 400; literal envelope; redacted

`?body_contains=` arrives as `Some("")` from serde. The handler
rejects it with HTTP 400 and the literal envelope
`{"status":"error","error":"invalid body_contains"}` via
`query_http_common::error_response`. Rationale: an empty
substring is meaningless on `String::contains` (every string
contains the empty substring, so the filter would silently
match every record, observably indistinguishable from no
filter); the slice refuses the ambiguity out loud, symmetric
with ADR-0052 Decision 5 on the severity parameter. The reason
text is a static literal; the (empty) raw value is NEVER
interpolated.

### 5. Length cap on `body_contains` is 1024 bytes; same literal envelope

The handler rejects any non-empty value whose byte length
strictly exceeds 1024 bytes. The rejection uses the SAME
literal envelope as the empty-string arm: HTTP 400 with
`{"status":"error","error":"invalid body_contains"}`. No second
reason class is introduced; the redaction posture is the same
(the raw oversize value is NEVER echoed; the response body
does not contain any byte of the value, and does not contain a
length number). Rationale: an unbounded substring length lets
a malicious client ship megabytes inside a query-string
parameter; the 1024-byte cap is large enough to accommodate any
honest error string a human or runbook would carry (full kafka
stack traces, full sentence reasons, full URL paths) and small
enough to refuse abuse. The cap value (1024 bytes) is in the
same order of magnitude as the axum stack's request-line and
header limits, so the cap is internally consistent rather than
novel. The boundary is INCLUSIVE: a value of EXACTLY 1024 bytes
is served; a value of 1025 bytes or more is refused (mirrors
the inclusive cap-boundary posture from ADR-0050 Decision 1 on
`MAX_WINDOW_SECONDS`).

### 6. Filter BEFORE the result cap (FLAG 3 from DISCUSS)

The body-substring filter runs at the store call (via
`query_with`) and therefore drops non-matching records BEFORE
the `records.len() > MAX_RESULT_ROWS` check from ADR-0050
Decision 2. The cap measures what the user observes (the
post-filter records, not the upstream raw row count), which is
exactly what ADR-0050 Decision 4 already says ("The check
measures what the user observes (matrix entries for metrics;
bare-array records / spans for logs / traces), not the upstream
raw row count.") and what ADR-0052 Decision 4 pins for the
sibling severity filter. An operator running
`body_contains=kafka%20timeout` against a tenant with 200_000
unrelated in-window records and 50 matching records receives
the 50 matching records, NOT a cap-400 caused by the 200_000
unrelated records. The result-cap check stays exactly where it
is in the handler
(`crates/log-query-api/src/lib.rs:180-185`); only the source
of the `Vec<LogRecord>` it measures changes when a predicate is
applied.

### 7. Order of handler checks: parse `body_contains` AFTER `min_severity`, BEFORE the store

The handler's check order grows by one step, placed AFTER the
existing `min_severity` parse and BEFORE the store call:

1. Fail-closed tenancy (existing 401 arm — UNCHANGED;
   `crates/log-query-api/src/lib.rs:120` via
   `query_http_common::resolve_tenant_or_refuse`).
2. `parse_time_range` (existing 400 arm for non-numeric or
   inverted — UNCHANGED; ADR-0054 / ADR-0047).
3. Window cap (existing 400 arm:
   `end_secs - start_secs > MAX_WINDOW_SECONDS` — UNCHANGED;
   ADR-0050 Decision 1).
4. `parse_min_severity` if present (existing 400 arm for
   unknown name — UNCHANGED; ADR-0052 Decision 5).
5. **NEW**: parse `body_contains` if present. An empty value
   or a value over 1024 bytes is a 400 with
   `{"status":"error","error":"invalid body_contains"}`; the
   store is NEVER touched on this path. The raw parameter
   value is NEVER echoed (redaction; ADR-0047 Decision 1).
6. Dispatch the store call:
   - If `min_severity` is `Some(floor)` AND `body_contains` is
     `Some(target)`:
     `state.store.query_with(&tenant, range, &Predicate::new().min_severity(floor).body_contains(target))`.
   - If only `min_severity` is `Some(floor)`:
     `state.store.query_with(&tenant, range, &Predicate::new().min_severity(floor))`
     (the existing branch from ADR-0052).
   - If only `body_contains` is `Some(target)`:
     `state.store.query_with(&tenant, range, &Predicate::new().body_contains(target))`.
   - If both are `None`: `state.store.query(&tenant, range)`
     (the existing fall-through branch).
7. Result cap (existing 400 arm: `records.len() > MAX_RESULT_ROWS`
   — UNCHANGED; ADR-0050 Decision 2; measures the post-filter
   vector when a predicate was used).
8. `success_response(records)` (existing 200 arm — UNCHANGED).

The new parse step is its OWN gate; it is NOT folded into
`parse_min_severity` or `parse_time_range`. The store is NEVER
touched on the empty-or-oversize body_contains 400 path; the
acceptance scenario in
`docs/feature/log-body-text-search-v0/discuss/user-stories.md`
§ "An empty body_contains value is a redacted 400" pins this
with a no-store-call assertion.

### 8. Error envelope reuse; redaction inherited

The empty-and-oversize-body_contains 400 reuses the existing
`{status:"error", error:"<reason>"}` envelope (ADR-0047
Decision 1; ADR-0050 Decision 7; ADR-0052 Decision 1) via
`query_http_common::error_response`. The reason text names the
class (`"invalid body_contains"`) WITHOUT echoing the raw
parameter value: a client sending a 2048-byte
`body_contains=<2048-byte payload>` receives
`{"status":"error","error":"invalid body_contains"}` and the
response body does NOT contain any byte of the payload and
does NOT contain a length number. The redaction posture is
symmetric with the existing `parse_min_severity` redaction
(ADR-0052 Decision 1, Verification §) and with the
cap-reason redaction from ADR-0050 Decision 7.

No new status code. No new envelope field. No new reason class
beyond `"invalid body_contains"` (one literal, reused for both
the empty-string and the over-cap arms). Prism's `isPromError`
already handles the envelope.

### 9. Parse helper: a free function in `log-query-api`, NOT on lumen

The body-substring parse helper is a free function in
`crates/log-query-api/src/lib.rs`:

```text
const MAX_BODY_CONTAINS_LEN: usize = 1024;

fn parse_body_contains(raw: &str) -> Result<String, &'static str>
```

The function lives next to `parse_min_severity`; it is NOT a
method on `lumen::Predicate`. The lumen crate stays free of
HTTP-shaped string parsing and free of byte-length-cap policy
(the cap belongs at the boundary; the predicate belongs in the
substrate). Acceptance and unit tests in
`crates/log-query-api/tests/slice_01_body_contains.rs` and in
the inline `mod tests` of `crates/log-query-api/src/lib.rs`
exercise the parser by name; the parse-helper spec at
`docs/feature/log-body-text-search-v0/design/parse-helper-spec.md`
pins the test surface.

### 10. Lumen `Predicate` grows ONE field, ONE builder, ONE `matches` arm, ONE `is_empty` clause

`lumen::Predicate` (`crates/lumen/src/predicate.rs:25-28`)
today carries `service: Option<String>` and
`min_severity: Option<SeverityNumber>` only. This ADR grows
the predicate additively:

- One new field: `body_contains: Option<String>`.
- One new builder method:
  `pub fn body_contains(mut self, s: impl Into<String>) -> Self`
  mirroring the existing `service(name)` and
  `min_severity(sev)` builders in shape and style.
- One new arm in `Predicate::matches`
  (`crates/lumen/src/predicate.rs:53-66`):
  `if let Some(target) = self.body_contains.as_deref() { if !record.body.contains(target) { return false; } }`.
  Placed alongside the existing two arms; conjunction is
  commutative, so the placement is not load-bearing for
  correctness, only for readability.
- One new clause in `is_empty`
  (`crates/lumen/src/predicate.rs:70-72`): the existing
  `self.service.is_none() && self.min_severity.is_none()` is
  extended to `... && self.body_contains.is_none()`.

The change is FIELD-ADDITIVE on the public surface (Gate 2
`cargo public-api` shows one new pub method on `Predicate`; the
existing constructors and builders remain byte-identical; an
old caller is byte-compatible because `Predicate::new()`
continues to return an empty predicate that matches every
record). The `lumen::LogStore` trait signatures stay
byte-identical to the prior tag.

### 11. Both adapters light up automatically; zero impl change

`InMemoryLogStore::query_with` at
`crates/lumen/src/store.rs:159-180` already routes per record
through `predicate.matches(r)` at line 175. The extended
predicate's new `matches` arm fires automatically; no edit to
`crates/lumen/src/store.rs` is required.

`FileBackedLogStore::query_with` at
`crates/lumen/src/file_backed.rs:229-250` already routes per
record through `predicate.matches(r)` at line 245. Same: no
edit to `crates/lumen/src/file_backed.rs` is required.

The slice's lumen surface change is therefore concentrated in
ONE file (`crates/lumen/src/predicate.rs`); the adapter files
are untouched.

### 12. Composition with `min_severity` is conjunctive AND

`Predicate::matches` returns `true` only when every set arm
passes (it is a sequence of early-return-false guards followed
by a `true`). Slice 01's three set-filter shapes (empty,
`min_severity` only, `body_contains` only, both) all compose
via the existing arm-by-arm AND. When both filters are present,
a record passes if and only if it satisfies BOTH; the order of
arms in `matches` is irrelevant because AND is commutative. The
acceptance suite SHALL include a scenario where both parameters
are present (e.g. `min_severity=ERROR&body_contains=kafka%20timeout`)
and the response contains only ERROR-or-above records whose
body contains `kafka timeout`.

### 13. NO new event, NO new metric, NO new dashboard

Consistent with ADR-0050 Decision 8, ADR-0052, and ADR-0054:
at v0/v1 the platform has no live observability stack of its
own; the contract IS the signal. No counter is incremented on
empty-or-oversize body_contains refusals beyond the existing
`tracing::error!` calls for store failures (and the
empty-or-oversize body_contains 400 does NOT emit a store-error
tracing event because the store was not the cause). Narrowed-
read adoption counters and post-filter record-count histograms
are explicitly deferred to a successor slice once a live
observability consumer exists.

## Alternatives considered

### Filter location A (rejected): handler-side post-`query_with`

Apply the substring filter handler-side on the `Vec<LogRecord>`
returned by `query_with`, leaving `lumen::Predicate`
byte-identical (zero `cargo public-api` diff on lumen). For:
narrowest possible surface change on `lumen`. Against:

- Splits the predicate semantics across two crates (the lumen
  predicate carries the severity filter; the `log-query-api`
  handler carries the substring filter); the conjunctive
  composition is no longer expressed in ONE place.
- Breaks the "the predicate IS the filter" invariant the lumen
  surface established with `service` and `min_severity` (every
  filter at the store boundary, the handler is parse-and-wire
  only).
- Prevents the v1 columnar substrate from pushing the
  substring scan into the storage adapter where it belongs
  (an index, a Bloom filter, a Tantivy posting list); the v1
  adapter would have to expose a different shape (or the
  predicate would have to grow `body_contains` at that point
  anyway, which is THIS decision deferred).
- Inverts the filter-BEFORE-cap interaction (Decision 6): if
  the substring filter is post-store and post-cap, the cap
  would fire on the pre-filter raw row count, contradicting
  ADR-0050 Decision 4. If the substring filter is post-store
  BUT pre-cap, the cap shape is correct but the predicate
  shape splits.

Rejected.

### Filter location B (accepted): EXTEND `lumen::Predicate`

Add ONE field, ONE builder, ONE `matches` arm, ONE `is_empty`
clause. The predicate stays the single source of truth for
"how a record is filtered"; the cap interaction is correct by
construction (the store returns post-filter records; the
handler caps the returned vector); the v1 columnar substrate
inherits the predicate seam without surface change. Accepted.

### Length cap A (rejected): no cap

For: the smallest possible parser; let the axum stack's
request-line limit be the de facto cap. Against: the axum
limit is on the request line as a whole (HTTP/1.1's typical
8 KiB), shared across every query-string parameter and
header; relying on it puts the body_contains cap on a shared
budget rather than a parameter-local one; a malicious client
can ship the maximum allowed value as `body_contains=` and
the platform has no defence in the parser. Rejected.

### Length cap B (rejected): 4096 bytes or 64 KiB

For: more headroom for legitimate long error strings (e.g. a
truncated stack trace pasted from a runbook). Against: the
substring purpose is to NARROW the result set; an operator
who needs to match a 4096-byte string would likely be better
served by the future regex slice or a different shape; a
larger cap moves the protection further from honest usage
without buying recall. Rejected at this slice; a successor
slice may raise the cap if real usage demands it.

### Length cap C (accepted): 1024 bytes

The middle path: large enough for any human-typed or
runbook-pasted error string (a 1024-byte string spans roughly
12-15 lines of text or a full kafka stack frame line); small
enough to refuse megabyte abuse. Internally consistent with
the axum stack's per-component limits. Accepted.

### Helper return shape A (rejected): `Result<String, String>`

For: matches the existing `parse_min_severity` shape exactly
(`Result<SeverityNumber, String>`). Against: the entire
failure surface is two reason texts and both are static
literal constants (Decision 4 and Decision 5); a `String` Err
arm pays the allocation cost on every failure without
buying anything; a mutation that returns a different reason
is harder to catch with `String` (the reviewer has to read
the construction) than with `&'static str` (the reviewer
reads the literal). Rejected.

### Helper return shape B (accepted): `Result<String, &'static str>`

The `Err` arm is a `&'static str` literal; the success arm
is an owned `String` (a fresh copy of the operator's input,
ready to be moved into the predicate via the `Into<String>`
builder bound). Accepted. The shape diverges from
`parse_min_severity` deliberately because the body-contains
failure surface is strictly literal-only; the divergence is
in the comment above the function.

### ADR shape A (rejected): amend ADR-0047

The slice grows the read-side log API contract by one
optional parameter; the natural home is ADR-0047. For: the
contract lives with the originating ADR. Against: ADRs in
this repository are immutable (the convention is set by
ADR-0001 and honoured by every preceding ADR including
ADR-0049, ADR-0050, ADR-0051, ADR-0052, ADR-0053, ADR-0054);
the amendment would create a precedent that drifts.
Rejected.

### ADR shape B (rejected): no ADR, only a wave-decisions note

For: the smallest documentation surface for a slice. Against:

- The contract change is visible on the wire (a new optional
  parameter, a new 400 reason class, a new accepted-value
  posture).
- The lumen surface change is visible in `cargo public-api`
  diff (a new pub builder method on `Predicate`).
- An operator reading ADR-0047 for the read contract should
  find the parameter cross-referenced from a durable ADR;
  the ADR is the durable cross-reference.

Rejected.

### ADR shape C (accepted): new ADR-0055, cites 0047, 0050, 0052, 0054, none modified

One ADR records the parameter, the substring-vs-regex pin,
the case-sensitivity pin, the empty-and-oversize 400, the
length cap value, the filter-BEFORE-cap interaction, and the
lumen predicate surface diff. Four precedents are cited with
section pointers; immutability is preserved. Accepted.

## Consequences

### Positive

- **The operator's "records carrying this string" job is
  served at the HTTP boundary, not client-side.** Sara Mendez
  stops piping through
  `jq 'map(select(.body | contains("kafka timeout")))'`; the
  payload reduction is honest (every returned record carries
  the substring; every record carrying the substring is
  returned), measured by KPI-1.
- **First post-extraction real-world validation of
  `query-http-common` (ADR-0054, M-5).** The slice consumes
  the shared scaffold without re-implementing any of it;
  KPI-3's CI-static-grep assertions and the under-30-LOC
  line-count budget on `crates/log-query-api/src/lib.rs` are
  the honest measure that the shared crate paid for itself.
- **No new envelope, no new status code, no new client-side
  change.** The bare JSON array on success (ADR-0047 Decision
  1) and the `{status:"error", error:"invalid body_contains"}`
  envelope are reused verbatim; Prism's `isPromError` and
  every existing curl / jq client continue to work as today.
- **Lumen surface grows additively only.** The `LogStore`
  trait signatures stay byte-identical to the prior tag; the
  `Predicate` struct grows ONE field, ONE builder, ONE
  `matches` arm, ONE `is_empty` clause; an old caller that
  built a `Predicate` via `new().service(...).min_severity(...)`
  is byte-compatible.
- **Both adapters light up automatically.**
  `InMemoryLogStore::query_with` and
  `FileBackedLogStore::query_with` route through
  `predicate.matches(r)`; zero impl edit is required in
  either store file.
- **No new module, no new crate, no new external
  dependency.** The slice is parse + wire inside
  `crates/log-query-api/src/lib.rs` plus a four-line edit in
  `crates/lumen/src/predicate.rs`.
- **Backward compatibility preserved.** A parameter-less
  request is byte-equal to the slice-prior response for the
  same inputs; every existing acceptance scenario in
  `tests/slice_01_logs_read.rs`, `tests/slice_02_caps.rs`,
  and `tests/slice_01_severity_filter.rs` stays green
  unchanged (KPI-2).
- **Cap interaction preserved.** The result cap still
  measures what the user observes (ADR-0050 Decision 4); an
  operator's narrowed read receives all matching records up
  to the cap, not a cap-400 caused by upstream noise.
- **Redaction posture preserved and extended.** The
  empty-and-oversize body_contains 400 body never contains
  the raw parameter value; the same literal envelope serves
  both arms; the symmetric redaction tests cover both.
- **Composition with `min_severity` is honest at the
  predicate boundary.** When both parameters are present,
  the conjunctive AND lives in ONE place
  (`Predicate::matches`), not split across the handler and
  the store.

### Negative

- **The match is byte-substring only.** Operators who need
  whole-word matching, regex matching, or
  case-insensitive matching are not served at slice 01; the
  use cases are explicitly deferred (substring vs regex per
  Decision 2; case-sensitivity per Decision 3). A successor
  slice adds the regex parameter or the case-folding flag.
- **The match is on `body` only.** Records whose
  identifying string lives in `severity_text`, in
  `attributes`, or in `resource_attributes` are not served
  at slice 01; the fields are explicitly out of scope.
- **The length cap is fixed at 1024 bytes.** Operators with
  legitimately long substrings (a truncated stack trace
  pasted from a runbook) hit the 400 and have to break the
  substring into shorter probes; the cap value is
  conservative and may rise in a successor slice if real
  usage demands.
- **The cap interaction is correct but invisible until
  tested.** An operator cannot distinguish "the cap fired
  before the filter" from "the cap fired after the filter"
  without reading the source; the acceptance scenario
  ("The filter applies BEFORE the result cap" against a
  200_000 non-matching + 50 matching fixture) pins it
  observably, and the cap-budget interaction is
  mutation-tested.
- **The lumen public surface grows.** `cargo public-api`
  diff is non-empty (one new pub builder method on
  `Predicate`); the diff is additive only and the existing
  surface is byte-identical, but the crafter MUST snapshot
  the new baseline as part of the DELIVER wave (Gate 2).

### Trade-off summary

The slice trades a wider matching grammar (regex,
case-insensitive, multi-field) and an unbounded substring
length for a small, honest, ADR-pinned parse + wire growth of
the HTTP contract combined with a minimal additive lumen
predicate extension; it buys server-side payload reduction,
operator muscle-memory honesty, and the first post-extraction
real-world validation of `query-http-common` (M-5). The
`LogStore` trait, the envelope, the cap interaction, the
redaction posture, and every existing client are preserved
unchanged.

## Verification

- A workspace grep for `body_contains`, `parse_body_contains`,
  and `"invalid body_contains"` in
  `crates/log-query-api/src/lib.rs` returns the expected
  single occurrences after slice 01 lands; today: zero hits.
- The slice-01 acceptance suite in
  `crates/log-query-api/tests/slice_01_body_contains.rs`
  (NEW, DISTILL-wave output) exercises:
  - The walking-skeleton happy path
    (`body_contains=kafka%20timeout` against a six-record
    fixture returns only the two matching records in
    ascending observed-time order; US-01).
  - The calm-empty path (a substring no record's body
    contains returns 200 with `[]`, NEVER 404, NEVER 500;
    US-02).
  - The default-unchanged path (a request with no
    `body_contains` parameter returns every in-window
    record; byte-equal to the slice-prior response shape;
    US-03).
  - The empty-body_contains 400 with redaction (a request
    with `?body_contains=` returns 400 with the literal
    envelope; the store is NOT touched; US-04).
  - The case-sensitive pin (`body_contains=KAFKA` against a
    `kafka timeout` fixture returns 200 with `[]`; the body
    of the matching record never appears; US-05).
  - The cross-tenant isolation (tenant B receives `[]` when
    querying for a substring that exists in tenant A's
    records and is absent from tenant B's; the substring
    of tenant A's record never appears in tenant B's
    response body; US-06).
  - The conjunctive composition with `min_severity` (a
    request with both parameters present returns only
    records matching BOTH).
- The oversize-body_contains 400 with redaction (a request
  with a 2048-byte `body_contains` value returns 400 with
  the literal envelope; the body does NOT contain any byte
  of the value; the store is NOT touched). The acceptance
  scenario is added by the DISTILL wave per the
  parse-helper spec.
- The case-sensitivity inline unit tests in
  `crates/log-query-api/src/lib.rs` next to the existing
  `parse_min_severity_*` tests pin the parser's behaviour
  per the parse-helper spec.
- Gate 2 `cargo public-api` confirms:
  - `lumen::LogStore` trait method signatures stay
    byte-identical to the prior tag.
  - `lumen::Predicate` grows ONE new pub method
    (`body_contains`); the existing surface is byte-identical.
  - `crates/log-query-api/src/lib.rs`'s public surface is
    byte-identical (the `LogsParams` struct is private; the
    `body_contains` field addition does NOT appear in the
    public-api diff).
- **Earned-Trust enforcement (three orthogonal layers
  reproduced from ADR-0049 / ADR-0050 / ADR-0051 / ADR-0052
  Verification)**: (a) subtype / compile-time check (the
  predicate's new `matches` arm references `record.body`
  which is a `String` per `crates/lumen/src/record.rs:54`;
  removing the field or changing the type fails the
  compile); (b) AST structural check via the acceptance
  suite's per-scenario reference (the suite references
  `body_contains` by literal in the URL and in the assertion
  text; a mutant that drops the parse step or the dispatch
  arm is killed by the walking-skeleton happy path); (c)
  behavioural gold-test via the slice-01 suite (six
  scenarios plus the conjunctive-composition scenario plus
  the oversize-redaction scenario). A single-layer bypass is
  caught by at least one of the other two.
- Mutation testing: `cargo mutants` scoped to the modified
  files via the existing `gate-5-mutants-log-query-api` and
  `gate-5-mutants-lumen` workflows at the 100% kill-rate
  gate (ADR-0005 Gate 5; CLAUDE.md). Primary mutation
  targets:
  - The substring boundary on `record.body.contains(target)`:
    a `contains` -> `starts_with` mutant must be killed by a
    fixture record whose body has the substring in the
    MIDDLE; a `contains` -> `ends_with` mutant must be
    killed by a fixture record whose body has the substring
    at the START.
  - The case-sensitivity boundary: a `String::contains` ->
    `to_lowercase().contains` mutant must be killed by the
    `KAFKA` != `kafka` scenario.
  - The empty-string rejection: a mutant that treats
    `Some("")` as `None` (or returns `Ok(String::new())`)
    must be killed by the empty-string 400 scenario and the
    inline unit test.
  - The length-cap boundary: a `>` -> `>=` mutant on the
    1024-byte cap must be killed by the
    `parse_body_contains_accepts_input_at_exactly_the_cap`
    unit test (1024 bytes is INCLUSIVELY accepted).
  - The redaction on the 400 arms: a mutant that
    interpolates the raw value into the reason text is
    killed by the byte-equality assertion against the
    literal envelope.
  - The order-of-checks: a mutant that calls the store
    BEFORE parsing `body_contains` is killed by the
    no-store-call assertion on the empty-string 400 arm.
  - The dispatch branch: a mutant that calls `query` even
    when `body_contains` is `Some` is killed by the
    walking-skeleton happy path, where the response would
    otherwise contain every in-window record.
  - The `is_empty` clause: a mutant that drops the new
    `&& self.body_contains.is_none()` is killed by a unit
    test asserting `Predicate::new().body_contains("x").is_empty() == false`.

## External-integration handoff

None. The parse helper is in-process string matching; the
store call uses an in-process trait method
(`lumen::LogStore::query_with`) against the durable
`FileBackedLogStore`, which is a first-party library, not a
network service. No third-party API is consumed; no new
external dependency is introduced; no consumer-driven
contract test recommendation. The existing ADR-0047
Earned-Trust startup probe continues to run unchanged (the
probe issues a parameter-less empty-range `query`; the slice
does not alter the probe).

## Relationship to ADR-0047, ADR-0050, ADR-0052, and ADR-0054

- **ADR-0047** is the originating read-side contract for
  logs. Its bare-JSON-array success shape (Decision 1), its
  `{status:"error", error}` error envelope (Decision 1), its
  redaction posture (Decision 1), and its statement that
  `query_with(predicate)` exists on `LogStore` (Decision 5)
  are ALL PRESERVED. ADR-0055 is the second HTTP-boundary
  use of `query_with` (after ADR-0052) and reuses the
  envelope and the redaction verbatim. Cited, NOT modified.
- **ADR-0050** is the read-side Earned-Trust caps. Decision
  2 (the result cap measures the response the user
  observes), Decision 4 (the cap measures what the user
  observes, not the upstream raw row count), and Decision
  7 (the symmetric redaction extension) are PRESERVED. The
  body-contains filter slots at the store call (via
  `query_with`) so the post-filter `Vec<LogRecord>` is what
  the cap measures; the cap location, value, and reason
  text stay byte-identical. Cited, NOT modified.
- **ADR-0052** is the immediate sibling — the
  `min_severity` optional parameter on the same route.
  ADR-0055 follows the same parse-and-wire shape (one
  parameter, one parse helper, one new dispatch arm, one
  new 400 reason class, filter-BEFORE-cap interaction);
  the conjunctive AND composition with `min_severity` via
  `Predicate::matches` is the established pattern. Cited,
  NOT modified.
- **ADR-0054** is the M-5 `query-http-common` extraction.
  ADR-0055's slice is the FIRST consumer of the shared
  scaffold born AFTER the extraction shipped. The cap
  consts, the four `REASON_*` consts, the
  `error_response` helper, the
  `resolve_tenant_or_refuse` seam, and the
  `parse_time_range` parser are ALL reused via
  `query_http_common::`. KPI-3's static-grep CI assertions
  and the under-30-LOC line-count budget on
  `crates/log-query-api/src/lib.rs` are the honest measure
  that the shared crate paid for itself. Cited, NOT
  modified.

## Forward-looking scope

Slice 01 ships the single-substring body filter on the
`LogRecord.body` field, byte-wise, case-sensitive, length cap
1024 bytes. Successor slices (separately roadmapped, NOT this
ADR's scope) MAY:

- Add regex matching on `body` via a separate parameter
  (e.g. `body_regex=<pattern>`); the design wave for that
  slice MUST weigh the ReDoS posture and pick a backend
  (the `regex` crate's grammar, RE2, or PCRE).
- Add case-insensitive matching via a separate parameter
  (e.g. `body_contains_ci=<string>`) or a paired
  `case_sensitive=false` flag.
- Add matching against `severity_text`, `attributes`, or
  `resource_attributes` (each a separate slice if and
  when it earns a third call site).
- Add multi-substring matching
  (`body_contains=foo,bar` with OR or AND semantics; the
  shape and the operator are design decisions for that
  slice).
- Raise the length cap above 1024 bytes if real operator
  usage demands.
- Push the substring scan into the v1 columnar substrate's
  indexes (a Tantivy posting list, a Bloom filter, a
  prefix index); the predicate seam absorbs the change
  without surface diff.

Each successor change is a separate slice with its own ADR;
the cross-references will name ADR-0055 as the originating
`body_contains` parameter contract.
