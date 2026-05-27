# ADR-0052 — log-query-api `min_severity` filter parameter

- **Status**: Accepted
- **Date**: 2026-05-27
- **Author**: `nw-solution-architect` (Morgan)
- **Feature**: `log-query-severity-filter-v0`
- **Supersedes**: none
- **Superseded by**: none
- **Related**: ADR-0047 (the lumen log-query-api contract; Decision 1
  pins the bare-JSON-array success shape, the
  `{status:"error", error}` envelope, and the redaction posture
  this ADR reuses; Decision 5 records that
  `LogStore::query_with(predicate)` exists but is NOT used in slice
  01 of `lumen-query-api-v0`; this ADR is the first HTTP-boundary
  use of that seam and reproduces the envelope and the redaction
  verbatim; cited as the originating contract this ADR GROWS by one
  optional parameter, NOT modified). ADR-0050 (the read-side
  Earned-Trust caps; Decision 4 places the result-cap AFTER the
  store returns and BEFORE serialisation on the post-filter
  `Vec<LogRecord>`; this ADR slots the severity filter at the store
  call so the cap continues to measure what the user observes;
  cited as the cap interaction precedent, NOT modified).

## Context

The lumen log-query-api (`crates/log-query-api/src/lib.rs`, route
`GET /api/v1/logs?start=&end=`) returns every in-window
`LogRecord` for the resolved tenant as a bare JSON array
(ADR-0047 Decision 1). For an on-call SRE mid-incident on an
INFO-heavy tenant, "every in-window record" is mostly INFO and
DEBUG noise that drowns the WARN and ERROR records they actually
need; the typical workaround is `curl ... | jq 'map(select(
.severity_number >= 13))'`, which pays the full payload cost
client-side and the full bandwidth cost on the wire.

The lumen substrate already carries the seam to filter at the
store. `lumen::LogStore::query_with(&tenant, range, predicate)`
exists at `crates/lumen/src/store.rs:89`; `Predicate::min_severity(
SeverityNumber)` exists at `crates/lumen/src/predicate.rs:46`
with the correct `>=` semantics at `:61` (a record is rejected
when `record.severity_number < floor`; equality at the floor
passes). The OTel SeverityNumber ladder is fixed at
`crates/lumen/src/record.rs:32-39`: `TRACE=1`, `DEBUG=5`,
`INFO=9`, `WARN=13`, `ERROR=17`, `FATAL=21`. The DISCUSS wave
verified each of these by direct read (`discuss/wave-decisions.md`
Read-first checklist).

The slice is therefore a parse-and-wire growth of the HTTP read
contract: ONE optional query-string parameter on
`GET /api/v1/logs` whose value is one of the six OTel severity
names; the handler maps the name to the corresponding
`SeverityNumber` constant, constructs the predicate, and calls
`query_with` instead of `query`. No lumen change. No new module.
No new envelope. No new status code. No new tag. No new external
dependency.

ADR-0047 is the read-side contract for logs; growing it by one
optional parameter is a contract change. ADRs in this repository
are immutable (the convention is set by ADR-0001 and honoured by
every preceding ADR including ADR-0049, ADR-0050, and ADR-0051).
The growth therefore lands as a new ADR with a back-reference to
0047, not as an in-place edit. ADR-0052 is the next free number
(`ls docs/product/architecture/adr-0052*` returns no hits;
`adr-0051-pulse-per-tenant-cardinality-watermark.md` is the
latest; 0052 is the next).

## Decision

### 1. Wire parameter name: `min_severity` (FLAG 1)

The query-string parameter on `GET /api/v1/logs` is spelled
`min_severity`. The name aligns with the filter semantics (`>=`
on the SeverityNumber ladder, i.e. a floor) and with the lumen
builder method name `Predicate::min_severity(SeverityNumber)`. A
typo or unknown value is rejected with HTTP 400 and the existing
envelope (Decision 5 below).

### 2. Accepted values: the six OTel severity names, case-insensitive, no aliases (FLAG 2)

The parameter accepts the six OTel severity level names: `TRACE`,
`DEBUG`, `INFO`, `WARN`, `ERROR`, `FATAL`. The match is
case-insensitive: `WARN`, `warn`, `Warn`, `wArN` all map to
`SeverityNumber::WARN`. NO aliases are accepted at slice 01:
`WARNING` is NOT a synonym of `WARN`; `err` is NOT a synonym of
`ERROR`; `critical` is NOT a synonym of `FATAL`. Case-insensitivity
matches operator muscle memory and OTel-spec capitalisation
variations across SDKs; alias rejection keeps a typo a typo (the
platform refuses out loud rather than silently coercing).

### 3. Filter `>=` semantics; the boundary is INCLUSIVE

A record whose `severity_number` is greater than or equal to the
requested floor is INCLUDED. A record whose `severity_number` is
strictly less than the floor is EXCLUDED. The boundary is `>=`,
NOT `>`: `min_severity=WARN` includes records at exactly
`SeverityNumber::WARN` (13). This is the existing semantics of
`lumen::Predicate::min_severity` at `crates/lumen/src/predicate.rs:60-63`,
preserved verbatim; the HTTP boundary does not invert or relax
the substrate's rule.

### 4. Filter BEFORE the result cap (FLAG 3)

The severity filter runs at the store call (via `query_with`)
and therefore drops below-floor records BEFORE the
`records.len() > MAX_RESULT_ROWS` check from ADR-0050 Decision 2.
The cap measures what the user observes (the post-filter records,
not the upstream raw row count), which is exactly what ADR-0050
Decision 4 already says: "The check measures what the user
observes (matrix entries for metrics; bare-array records / spans
for logs / traces), not the upstream raw row count." An operator
running `min_severity=ERROR` against an INFO storm receives every
matching ERROR record up to the cap, not a cap-400 caused by an
INFO storm the request explicitly asked to filter out. The
result-cap check stays exactly where it is in the handler
(`crates/log-query-api/src/lib.rs:153`); only the source of the
`Vec<LogRecord>` it measures changes when the parameter is
present.

### 5. Order of handler checks: parse severity AFTER window parse, BEFORE the store

The handler's check order grows by one step, placed BEFORE the
existing store call:

1. Fail-closed tenancy (existing 401 arm — UNCHANGED).
2. `parse_time_range_seconds` (existing 400 arm for non-numeric
   or inverted — UNCHANGED).
3. Window cap (existing 400 arm: `end_secs - start_secs > MAX_WINDOW_SECONDS`
   — UNCHANGED; ADR-0050 Decision 1).
4. **NEW**: parse `min_severity` if present. Mapping is the six
   OTel names, case-insensitive, no aliases. An unknown value is
   a 400 with `{"status":"error","error":"unknown severity"}`;
   the store is NEVER touched on this path. The raw parameter
   value is NEVER echoed (redaction; ADR-0047 Decision 1).
5. Dispatch the store call:
   - If `min_severity` is `Some(floor)`:
     `state.store.query_with(&tenant, range, &Predicate::new().min_severity(floor))`.
   - If `min_severity` is `None`: `state.store.query(&tenant, range)`
     (existing call — UNCHANGED).
6. Result cap (existing 400 arm: `records.len() > MAX_RESULT_ROWS`
   — UNCHANGED; ADR-0050 Decision 2; measures the post-filter
   vector when a predicate was used).
7. `success_response(records)` (existing 200 arm — UNCHANGED).

The new parse step is its OWN gate; it is NOT folded into
`parse_time_range_seconds` (the time parser stays a time parser).
The store is NEVER touched on the unknown-severity 400 path; the
acceptance scenario in `user-stories.md` § "An unknown severity
name is a redacted 400" pins this with a no-store-call
assertion.

### 6. Error envelope reuse; redaction inherited

The unknown-severity 400 reuses the existing
`{status:"error", error:"<reason>"}` envelope (ADR-0047 Decision
1; ADR-0050 Decision 7). The reason text names the class
("unknown severity") WITHOUT echoing the raw parameter value: a
client sending `min_severity=WARNING` (a non-OTel alias) receives
`{"status":"error","error":"unknown severity"}` and the body
does NOT contain the literal substring `WARNING`. The redaction
posture is symmetric with the existing
`the_bounds_error_never_echoes_the_raw_value` test
(`crates/log-query-api/src/lib.rs:292`) and with the cap-reason
redaction from ADR-0050 Decision 7.

No new status code. No new envelope field. No new reason class
beyond `"unknown severity"`. Prism's `isPromError` already
handles the envelope.

### 7. Parse helper: a free function in `log-query-api`, NOT on lumen

The string-to-`SeverityNumber` mapping is a free function in
`crates/log-query-api/src/lib.rs`:

```text
fn parse_min_severity(raw: &str) -> Result<SeverityNumber, String>
```

The function lives next to `parse_time_range_seconds` and
`parse_epoch_seconds`; it is NOT a method on `lumen::Predicate`.
The lumen crate stays free of HTTP-shaped string parsing (the
parsing belongs at the boundary, the predicate belongs in the
substrate). The function name uses the same shape as
`parse_time_range_seconds`: an unambiguous verb-then-noun that
names what is parsed and is the natural place for the
case-insensitive mapping. Acceptance tests in the new
`tests/slice_01_severity_filter.rs` will reuse this name.

### 8. Wiring: extend `LogsParams`; branch the handler; one new 400 arm

The existing `LogsParams` struct (`crates/log-query-api/src/lib.rs:107`)
grows one additive field `min_severity: Option<String>`. The
serde `Deserialize` derive automatically deserialises a missing
parameter as `None`. The handler grows ONE new conditional 400
arm (the unknown-severity arm), ONE new dispatch branch (call
`query_with` when the parsed floor is `Some`, fall through to
`query` when `None`), and uses the existing `error_response` and
`success_response` helpers unchanged. The change is parse + wire,
NOT a refactor of the handler shape.

### 9. The lumen `LogStore` trait is UNCHANGED

`pulse::MetricStore`, `lumen::LogStore`, `ray::TraceStore` trait
signatures stay byte-identical to the prior tag (Gate 2
`cargo public-api`). The slice uses the existing
`LogStore::query_with(&tenant, range, &predicate)` method
(`crates/lumen/src/store.rs:89`) and the existing
`Predicate::min_severity(SeverityNumber)` builder
(`crates/lumen/src/predicate.rs:46`). No new trait method, no
new module, no new file under `crates/lumen/src/`.

### 10. NO new event, NO new metric, NO new dashboard

Consistent with ADR-0050 Decision 8 and ADR-0047's posture: at
v0/v1 the platform has no live observability stack of its own;
the contract IS the signal. No counter is incremented on
unknown-severity refusals beyond the existing `tracing::error!`
calls for store failures (and the unknown-severity 400 does NOT
emit a store-error tracing event because the store was not the
cause). Narrowed-read adoption counters and post-filter
record-count histograms are explicitly deferred to a successor
slice once a live observability consumer exists.

## Alternatives considered

### Parameter name A (rejected): `level`

Short, familiar in many log frameworks. For: terse on the wire.
Against: ambiguous semantics — `level=WARN` could mean "exactly
WARN" (a set filter) or "WARN and above" (a floor); operators
and clients have seen both conventions across tools (syslog
`-p` is "exactly"; many log aggregators are "and above"). The
slice's semantics are unambiguously "and above" (a floor). The
name `level` invites a wrong intuition. Rejected.

### Parameter name B (rejected): `severity_min`

Same semantic clarity as `min_severity` (a floor). For: pairs
naturally with a hypothetical future `severity_max` for ranges.
Against: ranges are explicitly OUT of scope at slice 01 and
deferred indefinitely (`user-stories.md` § "OUT of scope"); the
hypothetical pairing buys no readability now; `min_severity`
matches the lumen builder method name verbatim
(`Predicate::min_severity`), which reduces cognitive translation
between the wire spelling and the substrate spelling. Rejected.

### Parameter name C (accepted): `min_severity`

Aligns with the semantics (`>=` floor); aligns with the lumen
builder method name (`Predicate::min_severity`); reads as a
natural-language phrase ("minimum severity"); no false-friend
intuition with set filters or verbosity descriptions. Accepted.

### Case-sensitivity A (rejected): strict (only `WARN`)

For: simplest possible mapping; one table key per name. Against:
operators commonly type the names in any case from muscle memory
across tools (`syslog -p warn`, OTel SDK `WARN`, ad-hoc
`Warn`); requiring exact upper-case is a friction tax with no
correctness benefit (the six names are unambiguous in any case).
Rejected.

### Case-sensitivity B (rejected): case-insensitive with aliases

For: forgiving on typos (`WARNING` -> `WARN`, `err` -> `ERROR`).
Against: aliases mask the platform's contract; a future
`severity_text`-based filter (a separately-roadmapped slice;
DISCUSS § "OUT of scope") may want to distinguish a user-defined
text label `"WARNING"` from the OTel name `WARN`; pre-coercing
them at slice 01 forecloses that future cleanly. Honest-cap
posture: a typo is a typo, refused with a named 400. Rejected.

### Case-sensitivity C (accepted): case-insensitive, no aliases

The middle path: the case-insensitivity matches operator muscle
memory; the alias rejection matches the honest-refusal posture
(ADR-0050's Earned-Trust framing). Accepted.

### Filter ordering A (rejected): AFTER the result cap

Run the result-cap check on the raw store output BEFORE the
filter. For: simpler invariant on the cap (it always measures
the raw row count). Against: an INFO storm eats the cap budget
on a request that explicitly asked to filter INFO out; an
operator running `min_severity=ERROR` against a tenant with
150_000 INFO and 50_000 ERROR records in-window receives a
cap-400 instead of the 50_000 matching ERRORs; the cap measures
upstream noise instead of what the user observes. Directly
contradicts ADR-0050 Decision 4 ("The check measures what the
user observes ... not the upstream raw row count"). Rejected.

### Filter ordering B (accepted): BEFORE the result cap

The filter runs at the store call via `query_with` (so the
below-floor records never enter the returned `Vec<LogRecord>`);
the cap check then measures the post-filter vector
(`records.len() > MAX_RESULT_ROWS`); the operator running
`min_severity=ERROR` against the INFO storm receives the 50_000
ERROR records. Aligns with ADR-0050 Decision 4. Accepted.

### Parse-helper location A (rejected): a new method on `lumen::Predicate`

Push the name-to-`SeverityNumber` mapping into the lumen crate
(e.g. `Predicate::min_severity_named(name: &str) -> Result<Self,
Error>`). For: the mapping lives next to the predicate. Against:
the lumen crate is HTTP-shape-free today (no string parsing of
wire-shaped inputs anywhere); a name mapping introduces a
boundary concern (HTTP parameter strings) into a substrate
crate; would require widening lumen's public surface; the
mapping is `log-query-api`-local at slice 01 and may genuinely
belong in a future `query-http-common` crate (ADR-0048 Decision
5, M-5) rather than in lumen. Rejected.

### Parse-helper location B (accepted): a free function in `crates/log-query-api/src/lib.rs`

The mapping lives next to `parse_time_range_seconds` and
`parse_epoch_seconds`; same module, same shape (a parse function
returning `Result<T, String>` where the `Err` arm is the reason
text used by `error_response`); `lumen` stays HTTP-free. A
future `query-http-common` extraction (deferred) is the natural
later home; this slice does not pay that cost speculatively.
Accepted.

### ADR shape A (rejected): amend ADR-0047

The slice grows the read-side log API contract by one optional
parameter; the natural home is ADR-0047. For: the cap lives with
the contract. Against: ADRs in this repository are immutable (the
convention is set by ADR-0001 and honoured by every preceding
ADR including ADR-0049 / ADR-0050 / ADR-0051); the amendment
would create a precedent that drifts. Rejected.

### ADR shape B (rejected): no ADR, a wave-decisions reference back to ADR-0047

For: the smallest documentation surface for a one-day slice.
Against: the contract change is visible on the wire (a new
optional parameter, a new 400 reason class, a new accepted-value
set); an operator reading ADR-0047 for the read contract should
find the parameter cross-referenced from there; the ADR is the
durable cross-reference. Rejected.

### ADR shape C (accepted): new ADR-0052, cites 0047 and 0050, neither modified

One ADR records the parameter, the case-sensitivity, the
boundary, and the cap interaction; two precedents are cited with
section pointers; immutability is preserved. Accepted.

## Consequences

### Positive

- **The operator's "WARN or worse" job is served at the HTTP
  boundary, not client-side.** Sara Mendez stops piping through
  `jq 'map(select(.severity_number >= 13))'`; Marcus Webb's
  automation shrinks its 30-second poll payload by the
  INFO-to-WARN-and-above ratio. KPI-1 (5x payload reduction on
  the named fixture) is the measurable target.
- **No new envelope, no new status code, no new client-side
  change.** The bare JSON array on success (ADR-0047 Decision 1)
  and the `{status:"error", error:"unknown severity"}` envelope
  are reused verbatim; Prism's `isPromError` and every existing
  curl / jq client continue to work as today.
- **No lumen change.** The `LogStore` trait signatures stay
  byte-identical to the prior tag; the existing `query_with` and
  `Predicate::min_severity` seams are used as designed; no new
  trait method, no new module, no new file under
  `crates/lumen/src/`. Gate 2 `cargo public-api` confirms.
- **No new module, no new crate, no new external dependency.**
  The slice is parse + wire inside the existing
  `crates/log-query-api/src/lib.rs`. The `query-http-common`
  extraction (ADR-0048 Decision 5, M-5) is HONOURED as deferred.
- **Backward compatibility preserved.** A parameter-less request
  is byte-equal to the slice-prior response for the same inputs;
  every existing acceptance scenario in
  `tests/slice_01_logs_read.rs` and `tests/slice_02_caps.rs`
  stays green unchanged (KPI-2).
- **Cap interaction preserved.** The result cap still measures
  what the user observes (ADR-0050 Decision 4); an operator's
  narrowed read receives all matching records up to the cap, not
  a cap-400 caused by upstream noise.
- **Redaction posture preserved.** The unknown-severity 400 body
  never contains the raw parameter value; the symmetric
  redaction tests cover it.

### Negative

- **The accepted set is exactly six names.** Operators
  accustomed to `WARNING` (a frequent OTel SDK
  `severity_text` value, distinct from the OTel
  `SeverityNumber::WARN` name) receive a 400 instead of an
  implicit alias; this is an intentional choice
  (Decision 2, case-sensitivity B alternative). A successor
  slice may add `severity_text` filtering on a different
  parameter once a user need exists; aliases are NOT added on
  the `min_severity` parameter (the parameter is OTel-name-shape
  by construction).
- **The filter is a single floor, NOT a range or set.** An
  operator who wants "WARN and ERROR but NOT FATAL" is not
  served at slice 01; the use case is rare (FATAL is a strict
  superset of relevance in any triage view) and explicitly
  deferred (`user-stories.md` § "OUT of scope"). A successor
  slice may grow the parameter or add a sibling.
- **The parse helper duplicates a small mapping that may belong
  in `query-http-common`.** The six-name table is ~10 lines;
  the duplication is bounded and mutation-tested in place; a
  future extraction is clean once a second HTTP read pillar
  exposes severity filtering (the trace pillar's `severity` is
  on spans, not the same shape).
- **The cap interaction is correct but invisible until tested.**
  An operator cannot distinguish "the cap fired before the
  filter" from "the cap fired after the filter" without reading
  the source; the acceptance scenario
  ("The filter applies BEFORE the result cap" in
  `user-stories.md`) pins it observably with a 150_000 INFO +
  50_000 ERROR fixture, and the cap-budget interaction is
  mutation-tested.

### Trade-off summary

The slice trades a wider alias set and a richer filter grammar
for a small, honest, ADR-pinned parse + wire growth of the HTTP
contract; it buys server-side payload reduction and operator
muscle-memory honesty at the cost of one new accepted-name
parameter, one new 400 reason class, one new parse helper, one
extended struct field, and one branched dispatch. The lumen
trait, the envelope, the cap interaction, the redaction
posture, and every existing client are preserved unchanged.

## Verification

- A workspace grep for `min_severity`, `parse_min_severity`, and
  `"unknown severity"` in `crates/log-query-api/src/lib.rs`
  returns the expected single occurrences after slice 01 lands;
  today: zero hits.
- The slice-01 acceptance suite in
  `crates/log-query-api/tests/slice_01_severity_filter.rs`
  (NEW, DISTILL-wave output) exercises:
  - The walking-skeleton happy path (a `min_severity=WARN`
    request against a mixed INFO/WARN/ERROR fixture returns only
    WARN and ERROR records in ascending observed-time order;
    US-01 Scenario 1).
  - The default-unchanged path (a request with no
    `min_severity` parameter returns every in-window record;
    byte-equal to the slice-prior response shape; US-02).
  - The boundary-inclusive case (a record at exactly the floor
    is INCLUDED; kills the `>=` -> `>` mutant on the predicate
    boundary; US-03 first scenario).
  - The just-below-floor case (a record one notch below the
    floor is EXCLUDED; US-03 second scenario).
  - The unknown-severity 400 with redaction (a request with
    `min_severity=WARNING` returns 400 with the existing
    envelope; the body does NOT contain `WARNING`; the store is
    NOT touched; US-05).
  - The filter-BEFORE-cap interaction (a 150_000 INFO +
    50_000 ERROR fixture with `min_severity=ERROR` returns 200
    with the 50_000 ERROR records, NOT a cap-400; reuses the
    `BulkLogStore` pattern from `tests/slice_02_caps.rs:86`;
    US-04).
- The case-insensitivity per-name table (`WARN`, `warn`, `Warn`,
  `wArN` all map to `SeverityNumber::WARN`) is pinned by inline
  unit tests in `crates/log-query-api/src/lib.rs` next to the
  existing `parse_time_range` tests, killing any mutant that
  drops or renames one of the six accepted names.
- Gate 2 `cargo public-api` confirms `lumen::LogStore`'s three
  method signatures are byte-identical to the prior tag. The
  `LogsParams` struct is `pub(crate)`; the `min_severity` field
  addition does NOT appear in the public-api diff.
- **Earned-Trust enforcement (three orthogonal layers reproduced
  from ADR-0049 / ADR-0050 / ADR-0051 Verification)**: (a)
  subtype / compile-time check (the case-insensitive match maps
  to the existing `SeverityNumber::TRACE` ... `SeverityNumber::FATAL`
  constants; removing any of the six match arms fails the
  compile at the test-site reference); (b) AST structural check
  via the acceptance suite's per-name reference (the suite
  references each of the six accepted names by literal; a
  mutant that drops one is killed by the per-name acceptance
  scenario); (c) behavioural gold-test via the slice-01 suite
  (the walking-skeleton happy path, the boundary scenarios, the
  unknown-severity 400, the filter-BEFORE-cap interaction). A
  single-layer bypass is caught by at least one of the other
  two.
- Mutation testing: `cargo mutants` scoped to the modified file
  via the existing `gate-5-mutants-log-query-api` workflow at
  the 100% kill-rate gate (ADR-0005 Gate 5; CLAUDE.md). Primary
  mutation targets:
  - The `>=` boundary on the severity floor (a `>=` -> `>`
    mutant must be killed by the boundary-inclusive scenario at
    exactly the floor; a `>=` -> `<` mutant must be killed by
    the WARN-includes-ERROR scenario).
  - The six-name mapping table (a mutant that drops or renames
    any one of `TRACE`, `DEBUG`, `INFO`, `WARN`, `ERROR`,
    `FATAL` is killed by the per-name acceptance assertion).
  - The case-insensitivity (a mutant that compares with
    `eq` instead of `eq_ignore_ascii_case` is killed by the
    `warn` / `Warn` per-case-form acceptance assertion).
  - The redaction on the unknown-severity 400 (a mutant that
    echoes the raw parameter value into the reason text is
    killed by the redaction substring assertion).
  - The order-of-checks (a mutant that calls `query` BEFORE
    parsing `min_severity` is killed by the no-store-call
    assertion on the unknown-severity 400 arm).
  - The dispatch branch (a mutant that calls `query` even when
    `min_severity` is `Some` is killed by the walking-skeleton
    happy path, where the response would otherwise contain INFO
    records).

## External-integration handoff

None. The parse helper is in-process string matching; the store
call uses an in-process trait method
(`lumen::LogStore::query_with`) against the durable
`FileBackedLogStore`, which is a first-party library, not a
network service. No third-party API is consumed; no new
external dependency is introduced; no consumer-driven contract
test recommendation. The existing ADR-0047 Earned-Trust startup
probe continues to run unchanged (the probe issues a
parameter-less empty-range `query`; the slice does not alter
the probe).

## Relationship to ADR-0047 and ADR-0050

- **ADR-0047** is the originating read-side contract for logs.
  Its bare-JSON-array success shape (Decision 1), its
  `{status:"error", error}` error envelope (Decision 1), its
  redaction posture (Decision 1), and its statement that
  `query_with(predicate)` exists but is NOT used in slice 01 of
  `lumen-query-api-v0` (Decision 5) are ALL PRESERVED. ADR-0052
  is the first HTTP-boundary use of `query_with` and reuses the
  envelope and the redaction verbatim. ADR-0047 grows by one
  optional parameter on the route; the growth is recorded in
  ADR-0052, not by editing ADR-0047. Cited, NOT modified.
- **ADR-0050** is the read-side Earned-Trust caps. Decision 2
  (the result cap measures the response the user observes,
  AFTER the store returns and BEFORE serialisation) and
  Decision 4 (the cap measures what the user observes, not the
  upstream raw row count) are PRESERVED. The severity filter
  slots at the store call (via `query_with`) so the
  post-filter `Vec<LogRecord>` is what the cap measures; the
  cap location in the handler does not move; the cap value does
  not change. Cited, NOT modified.

## Forward-looking scope

Slice 01 ships the single-floor severity filter on the six OTel
names. Successor slices (separately roadmapped, NOT this ADR's
scope) MAY:

- Add `severity_text` filtering on a different parameter for
  user-defined text labels (e.g. `severity_text=WARNING` where
  the operator's records carry the custom string `"WARNING"`).
- Add severity RANGES via a sibling parameter (e.g.
  `max_severity`) or a paired shape (e.g.
  `severity_in=WARN,ERROR`); the range surface is OUT of slice
  01 and DEFERRED indefinitely until a real use case emerges.
- Add body regex or substring filtering and record-attribute
  filtering; the substrate (`lumen::Predicate`) already
  supports `service(name)` for resource-attribute filtering,
  but the HTTP boundary does not expose it at slice 01.
- Extract the parse helper and the six-name mapping into the
  deferred `query-http-common` crate (ADR-0048 Decision 5, M-5)
  once a second HTTP read pillar exposes severity filtering.
- Lift the accepted set to env-driven aliases (e.g.
  `KALEIDOSCOPE_LOG_QUERY_SEVERITY_ALIASES`) once operator
  feedback names a genuine need.

Each successor change is a separate slice with its own ADR;
the cross-references will name ADR-0052 as the originating
`min_severity` parameter contract.
