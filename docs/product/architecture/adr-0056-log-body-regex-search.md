# ADR-0056 — log-query-api `body_regex` filter parameter and `lumen::Predicate` body-regex arm

- **Status**: Accepted
- **Date**: 2026-05-29
- **Author**: `nw-solution-architect` (Morgan)
- **Feature**: `log-body-regex-search-v0`
- **Supersedes**: none
- **Superseded by**: none
- **Related**: ADR-0047 (the lumen log-query-api contract; this
  ADR grows the contract by one optional parameter on the same
  route, with the bare JSON array success shape and the
  `{status:"error", error}` envelope reused verbatim; cited, NOT
  modified). ADR-0050 (the read-side Earned-Trust caps;
  Decision 4 places the result cap AFTER the store returns and
  BEFORE serialisation on the post-filter `Vec<LogRecord>`;
  this ADR slots the regex filter at the store call so the cap
  continues to measure what the user observes; cited, NOT
  modified). ADR-0052 (the `min_severity` optional parameter;
  this ADR follows the same parse-and-wire shape and composes
  conjunctively with `min_severity` via `Predicate::matches`;
  cited, NOT modified). ADR-0055 (the immediate predecessor —
  the `body_contains` optional parameter; this ADR follows the
  same single-parameter, single-parse-helper, single-dispatch-arm
  shape, reuses the 1024-byte cap value, reuses the literal
  envelope helper, and pins mutual exclusion against
  `body_contains`; cited, NOT modified). ADR-0046 (the
  `query-api` regex label matchers; the workspace's RE2-derived
  `regex` crate grammar and the compile-once-per-request shape
  are pinned here and reused for `body_regex`; cited, NOT
  modified). ADR-0054 (the M-5 `query-http-common` extraction;
  this slice is the SECOND post-extraction consumer of the
  shared scaffold after ADR-0055; cited, NOT modified).

## Context

The lumen log-query-api (`crates/log-query-api/src/lib.rs`, route
`GET /api/v1/logs?start=&end=`) today accepts the optional
`min_severity` floor (ADR-0052) AND the optional `body_contains`
byte-substring filter (ADR-0055, shipped at commit `1bfa609`).
An on-call SRE mid-incident still pays attention cost when a
single failure family is emitted in several closely-related
shapes: a kafka client library, an application wrapper, and a
platform retry handler each phrase the same underlying network
failure differently, and a `body_contains` filter only catches
one shape at a time. The typical workaround is to run three or
four `body_contains` queries and union the results in the
operator's head, or to download the entire window and grep
client-side. Both pay attention cost in seconds the operator
does not have during an active incident.

The lumen substrate carries the seam to push the predicate into
the store: `lumen::LogStore::query_with(&tenant, range,
&Predicate)` exists at `crates/lumen/src/store.rs:89`; both
adapters (`InMemoryLogStore` and `FileBackedLogStore`) already
route per record through `predicate.matches(r)`. The predicate
today (`crates/lumen/src/predicate.rs:24-33`, post-ADR-0055)
carries `service: Option<String>`, `min_severity:
Option<SeverityNumber>`, and `body_contains: Option<String>`; a
body-regex filter requires the predicate to grow one more
additive field.

The workspace already carries the `regex` crate at version `1`
as a direct dependency of `query-api`
(`crates/query-api/Cargo.toml:62`, ADR-0046 Decision 1), with
`Cargo.lock` pinned at `1.12.3`. The grammar is RE2-derived,
linear-time, and free of catastrophic backtracking. The same
grammar already powers `query-api`'s `=~` and `!~` label
matchers, so an operator who has used those matchers already
knows this slice's syntax.

The `regex` crate is NOT today a direct dependency of `lumen`
(verified by direct read of `crates/lumen/Cargo.toml`). This
slice adds it. The version specifier `"1"` mirrors
`query-api`'s spelling; the workspace's `Cargo.lock` already
pins `regex = "1.12.3"` so the new direct edge resolves to the
same lock pin with zero lockfile diff.

The slice is therefore a parse-and-wire growth of the HTTP read
contract — ONE optional query-string parameter on
`GET /api/v1/logs` whose value is a regular expression compiled
once per request and matched against `LogRecord.body` —
combined with ONE additive lumen surface change (one field, one
builder, one `matches` arm, one `is_empty` clause) AND ONE new
direct dependency on the storage crate. No new lumen module. No
new envelope. No new status code. No new tag.

ADR-0047 is the read-side contract for logs; growing it by one
optional parameter is a contract change. ADRs in this repository
are immutable (ADR-0001's convention, honoured by every
preceding ADR). The growth therefore lands as a new ADR with
back-references to ADR-0047, ADR-0050, ADR-0052, ADR-0054,
ADR-0055, and ADR-0046; none are modified. ADR-0056 is the next
free number (`ls docs/product/architecture/adr-0056*` returns
zero hits; ADR-0055 is the latest).

## Decision

### 1. Wire parameter name: `body_regex`

The query-string parameter on `GET /api/v1/logs` is spelled
`body_regex`. The name aligns with the filter semantics (a
regular expression matched against the `body` field) and reads
parallel to the immediate predecessor `body_contains`. The
parameter value is the raw pattern; URL-encoding follows the
existing handler defaults (axum / serde URL-decoded into the
`Option<String>` field on `LogsParams`).

### 2. Regex grammar: the workspace's existing `regex` crate

The grammar is the `regex` crate's default syntax (RE2-derived,
linear-time, no catastrophic backtracking). Pinning rationale:
(a) the workspace already adopts this crate for
`query-api`'s `=~` and `!~` label matchers under ADR-0046
Decision 1, so an operator who has used those matchers knows
this slice's grammar; (b) the linear-time guarantee is the
slice's ReDoS protection — a backtracking engine (PCRE) would
expose a denial-of-service surface on `body_regex` because the
pattern is user-supplied; (c) the crate is already in
`Cargo.lock` at `1.12.3` so the new direct dep on `lumen`
resolves to the same pin with zero lockfile diff. The
`regex_lite` crate was considered (Decisions /
Alternatives § C) and rejected for consistency with `query-api`.

### 3. Regex compile location: handler-side, fail-fast 400 on syntax error

The regex is compiled ONCE per request in a new
`parse_body_regex` helper in `crates/log-query-api/src/lib.rs`,
alongside `parse_min_severity` and `parse_body_contains`. A
`regex::Error` from `Regex::new(raw)` returns HTTP 400 with the
literal envelope `{"status":"error","error":"invalid body_regex"}`
via `query_http_common::error_response`. The store is NEVER
touched on the compile-failure path. Symmetric with ADR-0046
Decision 3 (`query-api` compiles its label matchers ONCE before
the row scan; verified at `crates/query-api/src/lib.rs:188-195`).

### 4. `Predicate` field type: compiled `Option<Regex>`

`lumen::Predicate` gains a new field `body_regex: Option<Regex>`
carrying the COMPILED regex, not the raw pattern string. The
handler hands the compiled `Regex` to the predicate via a new
builder `pub fn body_regex(mut self, re: Regex) -> Self`;
`Predicate::matches` calls `re.is_match(&record.body)` in a new
arm placed AFTER the existing `body_contains` arm. Rationale:
`Predicate::matches` is a hot path (per record); compiling the
regex per record would dominate the per-record match cost on
any non-trivial pattern. The compile-once-then-match-many shape
is the workspace's established pattern for user-supplied
regexes.

Load-bearing consequence: `regex::Regex` does NOT implement
`PartialEq` or `Eq`. The existing `#[derive(Debug, Clone,
Default, PartialEq, Eq)]` on `Predicate` (verified at
`crates/lumen/src/predicate.rs:24`) MUST be relaxed to
`#[derive(Debug, Clone, Default)]`. The `PartialEq` and `Eq`
traits are not exercised in the production code path; lumen
acceptance suites compare predicate effects via `matches`. The
relaxation is a public-surface change captured by Gate 2
`cargo public-api` and is part of this ADR's accepted diff.

### 5. Length cap: 1024 bytes; INCLUSIVE; same literal envelope as compile failure

The handler rejects any non-empty `body_regex` value whose byte
length strictly exceeds 1024 bytes. The rejection uses the SAME
literal envelope as the empty arm and the compile-failure arm:
HTTP 400 with `{"status":"error","error":"invalid body_regex"}`.
No second reason class is introduced; the redaction posture is
the same (the raw oversize value is NEVER echoed). A new
constant `MAX_BODY_REGEX_LEN: usize = 1024` lives next to
`MAX_BODY_CONTAINS_LEN` in `crates/log-query-api/src/lib.rs`.
The boundary is INCLUSIVE: EXACTLY 1024 bytes is served; 1025
bytes is refused. Rationale: operator-facing consistency with
the `body_contains` cap (ADR-0055 Decision 5); a single rule for
every body-related parameter. 1024 bytes is large enough for any
honest runbook-pasted regex and small enough to refuse abuse.

### 6. Empty `body_regex` is the same redacted 400

`?body_regex=` arrives as `Some("")` from serde. The handler
rejects it with HTTP 400 and the same literal envelope used for
over-cap and compile-failure: `{"status":"error","error":"invalid
body_regex"}`. Rationale: the `regex` crate's `Regex::new("")`
returns `Ok` (the empty pattern matches every position), which
would silently match every record — observably indistinguishable
from no filter at all. The slice refuses the ambiguity out loud,
symmetric with ADR-0055 Decision 4 on `body_contains` and
ADR-0052 Decision 5 on `min_severity`.

### 7. Mutual exclusion vs `body_contains`: 400 with new literal reason

When BOTH `body_contains` AND `body_regex` are present on the
same request, the handler returns HTTP 400 with the literal
envelope
`{"status":"error","error":"specify body_regex or body_contains, not both"}`
via `query_http_common::error_response`. The store is NEVER
touched on this path. The mutual-exclusion check is performed
AFTER `parse_body_contains` (so its own empty / over-cap 400
surfaces first) and BEFORE `parse_body_regex` (so an honest
cross-check 400 is not masked by a downstream
compile-failure 400 when both values are syntactically valid
but mutually-exclusively present).

Rationale: ambiguity is a symptom of client bug, not a semantic
primitive. The question "what does it mean to send BOTH a
substring filter and a regex filter on the same body?" deserves
a deliberate answer (intersection? union? error?), not a quiet
AND default that surprises the operator after the fact. Slice 01
answers "error"; future slices MAY relax to AND-compose once a
real operator use case earns the testing surface (the dispatch
grows from 6 to 8 reachable arms and `Predicate::matches`
carries both arms simultaneously).

The reason literal is NEW (it differs from `"invalid body_regex"`
because neither value is syntactically invalid; they are
mutually exclusive at this slice). The literal is symmetric with
ADR-0055's `"invalid body_contains"` in shape: a fixed
`&'static str`, served via the existing
`query_http_common::error_response`, never echoing the raw
parameter value.

### 8. Order of handler checks: parse `body_regex` AFTER `body_contains` and the cross-check, BEFORE the store

The handler's check order grows by TWO steps (one mutual-exclusion
check and one parse), placed AFTER the existing `body_contains`
parse and BEFORE the store call:

1. Fail-closed tenancy (existing 401 arm — UNCHANGED;
   `query_http_common::resolve_tenant_or_refuse`).
2. `parse_time_range` (existing 400 arm — UNCHANGED).
3. Window cap (existing 400 arm — UNCHANGED; ADR-0050).
4. `parse_min_severity` if present (existing 400 arm — UNCHANGED;
   ADR-0052).
5. `parse_body_contains` if present (existing 400 arm —
   UNCHANGED; ADR-0055).
6. **NEW** Mutual-exclusion check: if `body_contains.is_some() &&
   params.body_regex.is_some()`, return 400 with literal
   `"specify body_regex or body_contains, not both"`. Store NEVER
   touched.
7. **NEW** `parse_body_regex` if present. Empty / over-cap /
   compile-failure all return 400 with `"invalid body_regex"`.
   Store NEVER touched on any of these arms.
8. Dispatch the store call (one of 6 reachable arms by the cross
   product `min_severity x exactly-one-of {none, body_contains,
   body_regex}`).
9. Result cap (existing 400 arm — UNCHANGED; ADR-0050; measures
   the post-filter vector).
10. `success_response(records)` (existing 200 arm — UNCHANGED).

### 9. Composition with `min_severity` is conjunctive AND

When both `min_severity` AND `body_regex` are present, the
composed `Predicate::matches` enforces conjunctive AND: a record
passes iff `record.severity_number >= floor` AND
`re.is_match(&record.body)`. Arm order in `matches` is not
load-bearing because AND is commutative. The acceptance suite
SHALL include a scenario where both parameters are present.

### 10. Lumen `Predicate` grows ONE field, ONE builder, ONE `matches` arm, ONE `is_empty` clause

Mirrors ADR-0055 Decision 10 exactly:

- One new field: `body_regex: Option<Regex>`.
- One new builder method:
  `pub fn body_regex(mut self, re: Regex) -> Self`
  mirroring the existing `body_contains(s)`, `min_severity(sev)`,
  and `service(name)` builders in shape and style. The bound is
  `Regex` (not `impl Into<Regex>`) because there is no honest
  `Into<Regex>` source other than `Regex` itself.
- One new arm in `Predicate::matches` placed AFTER the existing
  `body_contains` arm:
  `if let Some(re) = self.body_regex.as_ref() { if !re.is_match(&record.body) { return false; } }`.
  Conjunction is commutative; the placement is for readability,
  not correctness.
- One new clause in `is_empty`: the existing
  `self.service.is_none() && self.min_severity.is_none() && self.body_contains.is_none()`
  becomes `... && self.body_regex.is_none()`.

The `#[derive(...)]` is relaxed per Decision 4: `PartialEq, Eq`
are dropped. `Debug, Clone, Default` remain.

The change is FIELD-ADDITIVE on the public surface (`cargo
public-api` shows one new pub method, one removed pair of
derived trait impls; the existing constructors and builders
remain byte-identical; an old caller that built a `Predicate`
via `new().service(...).min_severity(...).body_contains(...)`
is byte-compatible). The `lumen::LogStore` trait signatures
stay byte-identical to the prior tag.

### 11. Both adapters light up automatically; zero impl change

`InMemoryLogStore::query_with` (`crates/lumen/src/store.rs:159-180`)
and `FileBackedLogStore::query_with`
(`crates/lumen/src/file_backed.rs:229-250`) already route per
record through `predicate.matches(r)`. The new `body_regex` arm
fires automatically; no edit to either adapter file is required.
The slice's lumen surface change is concentrated in ONE source
file (`crates/lumen/src/predicate.rs`) plus the new direct
dependency in `crates/lumen/Cargo.toml`.

### 12. `lumen` direct dependency grows: `regex = "1"`

`crates/lumen/Cargo.toml` `[dependencies]` gains `regex = "1"`,
spelled identically to `crates/query-api/Cargo.toml:62`. The
workspace's `Cargo.lock` already pins `regex = "1.12.3"` via
`query-api`'s direct dep (ADR-0046); the new direct edge on
`lumen` resolves to the same lock pin with zero lockfile diff.
Licence compatibility: `regex` is MIT/Apache-2.0 dual-licensed;
both are compatible with `lumen`'s AGPL-3.0-or-later.

### 13. Parse helper: a free function in `log-query-api`, NOT on lumen

The body-regex parse helper is a free function in
`crates/log-query-api/src/lib.rs`:

```text
const MAX_BODY_REGEX_LEN: usize = 1024;

fn parse_body_regex(raw: &str) -> Result<Regex, &'static str>
```

The function lives next to `parse_body_contains`; it is NOT a
method on `lumen::Predicate`. The lumen crate stays free of
HTTP-shaped string parsing and free of byte-length-cap policy
(the cap belongs at the boundary; the predicate belongs in the
substrate). Lumen receives a pre-compiled `Regex` through the
builder. The parse-helper spec at
`docs/feature/log-body-regex-search-v0/design/parse-helper-spec.md`
pins the test surface.

### 14. NO new event, NO new metric, NO new dashboard

Consistent with ADR-0050 Decision 8, ADR-0052, ADR-0054, and
ADR-0055 Decision 13: at v0/v1 the platform has no live
observability stack of its own; the contract IS the signal. No
counter is incremented on any 400 arm. No tracing event fires on
the empty / over-cap / compile-failure / mutual-exclusion paths
(the store is never reached, so the existing store-error
tracing call is not invoked). Narrowed-read adoption counters
and post-filter record-count histograms are explicitly deferred
to a successor slice once a live observability consumer exists.

## Alternatives considered

### A. Compile inside `Predicate::matches` per call (rejected on cost)

For: smaller surface change on `lumen` (`Predicate` carries
`Option<String>`; no `regex` dep needed if the compile is done
inside an `if let Some(raw) = ... { Regex::new(raw).ok().and_then(...) }`
shape). Against:

- Per-record compile dominates the per-record match cost on any
  non-trivial pattern; the linear-time guarantee of `is_match`
  is wasted by an N-x-compile cost.
- The compile-failure path moves from HTTP-boundary
  (handler-side, 400) to inside the store iteration, where it
  must be either silently dropped (matching no record) or
  bubbled up as a store error (500). Both are dishonest answers
  to a client error.
- The fail-fast posture pinned by ADR-0046 Decision 3 for
  `query-api` is broken; the workspace loses its
  compile-once-per-request consistency.

Rejected.

### B. Accept both `body_contains` AND `body_regex` with AND-compose (rejected as slice 01 complexity)

For: a single request can carry both a substring pre-filter and
a regex post-filter for a coarse-then-fine narrowing. Against:

- The dispatch surface grows from 4 (today) -> 6 (this slice
  with mutual exclusion) -> 8 (with AND-compose); the
  acceptance suite must cover the additional 2 arms with their
  own conjunctive-composition scenarios.
- The semantic question "is the substring redundant with the
  regex, or a guard for it?" has no honest default; the
  operator-facing posture is unclear.
- The carpaccio slice principle is to defer optional complexity:
  ship the simpler mutual-exclusion contract first, let real
  operator use cases earn the AND-compose surface in a successor
  slice.

Rejected at slice 01; deferred to a successor slice if real
demand surfaces.

### C. `regex_lite` crate (rejected; consistency with `query-api`)

For: smaller binary, smaller direct-dep tree (lumen avoids a
heavier crate). Against:

- The workspace already adopts the full `regex` crate via
  `query-api` (ADR-0046); adding `regex_lite` to `lumen` would
  fragment the workspace's regex grammar across two crates with
  subtly different feature sets (Unicode tables, capture groups,
  inline flags).
- An operator who learns the syntax from `query-api`'s `=~`
  matchers would be surprised by a different grammar on
  `body_regex`; the muscle-memory honesty is broken.
- The `regex` crate is already in `Cargo.lock` at `1.12.3` so
  the binary-size argument is moot at the workspace level (the
  crate ships either way).

Rejected.

### D. Skip the ADR (rejected; lumen public surface and dep both grow)

For: the smallest documentation surface for a slice. Against:

- The `lumen::Predicate` public surface grows by ONE new pub
  method (`body_regex`) AND the `PartialEq, Eq` derive pair is
  removed. Both changes are visible in `cargo public-api` diff.
- The `lumen` direct-dependency tree grows by one edge (`regex
  = "1"`). The dep tree of the storage crate is visible to
  downstream consumers.
- The HTTP read contract grows by one optional parameter on the
  same route, parallel to ADR-0055; an operator reading
  ADR-0047 for the read contract should find the parameter
  cross-referenced from a durable ADR.

Each trigger independently warrants the ADR. Rejected.

## Consequences

### Positive

- **The operator's "every shape of this failure family" job is
  served at the HTTP boundary, not client-side.** Maria Santos
  stops running three or four `body_contains` queries and
  unioning the results; one regex query covers the family.
  Payload reduction is honest (every returned record matches
  the regex; every record matching the regex in the fixture is
  returned), measured by KPI-K1.
- **Fail-fast on compile errors.** A malformed pattern (an
  unbalanced paren, an invalid escape) is rejected at the HTTP
  boundary with HTTP 400 BEFORE the store is touched. The
  caller learns of the error in the time it takes to compile
  the pattern, not in the time it takes to scan the window;
  KPI-K3 pins the no-store-call assertion across the three
  rejection arms (empty, over-cap, compile failure).
- **Second post-extraction validation of `query-http-common`
  (ADR-0054 / M-5).** The slice consumes the shared scaffold
  for the second time after ADR-0055 without re-implementing
  any of it; KPI-K4 pins zero new duplications and the
  under-40-LOC budget.
- **First cross-pillar reuse of `regex` outside `query-api`.**
  The workspace's RE2-derived crate is now exercised on the
  log pillar as well; an operator who knows the `query-api`
  matchers already knows this slice's grammar (PIN 1).
- **No new envelope, no new status code, no new client-side
  change.** The bare JSON array on success and the
  `{status:"error", error}` envelope are reused verbatim;
  every existing curl / jq client continues to work as today.
- **Lumen public surface grows additively, except for the
  `PartialEq / Eq` derive relaxation.** The `LogStore` trait
  signatures stay byte-identical to the prior tag; the
  `Predicate` struct grows ONE field, ONE builder, ONE
  `matches` arm, ONE `is_empty` clause; an old caller that
  built a `Predicate` via
  `new().service(...).min_severity(...).body_contains(...)` is
  byte-compatible at the API surface.
- **Both adapters light up automatically.** Zero impl edit is
  required in either store file.
- **Backward compatibility preserved.** A parameter-less
  request is byte-equal to the slice-prior response for the
  same inputs; every existing acceptance scenario stays green
  (KPI-K2).
- **Cap interaction preserved.** The result cap still measures
  what the user observes (ADR-0050 Decision 4); an operator's
  narrowed read receives all matching records up to the cap.
- **Redaction posture preserved and extended.** Every new 400
  arm uses a literal-class reason that never contains the raw
  parameter value, including the empty, over-cap,
  compile-failure, and mutual-exclusion arms.
- **Mutation-test coverage is inherited.** The
  `gate-5-mutants-lumen` workflow (commit `d96a807`) picks up
  the new `Predicate::body_regex` field, builder, `matches`
  arm, and `is_empty` clause via `cargo mutants --in-diff
  origin/main`; KPI-K5 pins the 100% kill rate.

### Negative

- **Lumen now depends on the `regex` crate.** A new direct
  edge on the storage crate's dependency tree; the licence is
  MIT/Apache-2.0 (compatible with AGPL-3.0-or-later) and the
  `Cargo.lock` pin does not change, but the dep surface of
  `lumen` is now larger and a future workspace audit must list
  `regex` among `lumen`'s direct dependencies.
- **The predicate carries a non-comparable type.** The
  `Predicate` struct loses its `PartialEq` and `Eq` derived
  impls because `Regex` does not implement them. Callers that
  relied on predicate equality MUST switch to comparing by
  behaviour (running `matches` against a fixture); no caller
  in the workspace did this today (verified by `grep`), but
  the relaxation is visible in `cargo public-api`.
- **Mutual exclusion may surprise some clients.** A client
  that constructs URLs by parameter concatenation could send
  BOTH `body_contains` AND `body_regex` and receive a 400; the
  surprise is honest (the dispatch is well-defined; the
  operator gets a deliberate error rather than a quiet
  AND-compose default), but it costs one debugging round
  for clients that hit it first time.
- **The match is on `body` only.** Records whose identifying
  string lives in `severity_text`, in `attributes`, or in
  `resource_attributes` are not served at slice 01; the
  fields are explicitly out of scope.
- **The length cap is fixed at 1024 bytes.** Operators with
  legitimately long regexes (a hand-tuned pattern covering
  many shapes) hit the 400 and have to simplify; the cap is
  conservative and may rise in a successor slice if real
  usage demands.

### Trade-off summary

The slice trades a single-grammar matching contract (one
`body_regex` parameter, mutually exclusive with `body_contains`),
a non-comparable predicate type, and one new direct dep on the
storage crate for: a server-side payload reduction that covers
every shape of a failure family in one request; operator
muscle-memory honesty (the same `regex` crate as `query-api`'s
label matchers); the second real-world post-extraction
validation of `query-http-common`; and a zero-`Cargo.lock`-diff
dependency growth. The `LogStore` trait, the envelope, the cap
interaction, the redaction posture, and every existing client
are preserved unchanged.

## Verification

- A workspace grep for `body_regex`, `parse_body_regex`,
  `"invalid body_regex"`, and `"specify body_regex or
  body_contains, not both"` in `crates/log-query-api/src/lib.rs`
  returns the expected single occurrences after slice 01 lands;
  today: zero hits.
- The slice-01 acceptance suite in
  `crates/log-query-api/tests/slice_01_body_regex.rs` (NEW,
  DISTILL-wave output) exercises:
  - The walking-skeleton happy path (`body_regex=kafka.*timeout`
    against an eight-record fixture returns the three matching
    records in ascending observed-time order; US-01).
  - The calm-empty path (a regex no record's body matches
    returns 200 with `[]`; US-02).
  - The default-unchanged path (a request with no `body_regex`
    parameter returns every in-window record; byte-equal to the
    slice-prior response shape; US-03).
  - The compile-failure 400 with redaction and no-store-call
    assertion (US-04a).
  - The empty-string 400 with redaction (US-04b).
  - The over-cap 400 with redaction (US-04c).
  - The case-sensitive pin (`body_regex=kafka` against a
    `KAFKA timeout` fixture returns 200 with `[]`; US-05).
  - The mutual-exclusion 400 (BOTH `body_contains` and
    `body_regex` present returns 400 with the new literal; the
    store is NOT touched; US-06).
  - The cross-tenant isolation (tenant B receives `[]` when
    querying for a regex that matches tenant A's records and
    matches no record in tenant B's window; US-07).
  - The conjunctive composition with `min_severity` (a request
    with both parameters present returns only records matching
    BOTH).
- Inline `#[cfg(test)] mod tests` in
  `crates/log-query-api/src/lib.rs` pin the `parse_body_regex`
  boundary one byte at a time (1024 accepted, 1025 rejected),
  the empty-string rejection, the compile-failure rejection,
  the redaction posture, and the case-sensitive default; see
  `parse-helper-spec.md` for the per-test list.
- Gate 2 `cargo public-api` confirms:
  - `lumen::LogStore` trait method signatures stay
    byte-identical to the prior tag.
  - `lumen::Predicate` gains ONE new pub method
    (`body_regex`); the existing builders and constructors are
    byte-identical; the `PartialEq` and `Eq` impls are removed
    (the relaxation IS part of this ADR's accepted diff).
  - `crates/log-query-api/src/lib.rs`'s public surface is
    byte-identical (the `LogsParams` struct is private; the
    `body_regex` field is private; `parse_body_regex` is
    private; `MAX_BODY_REGEX_LEN` is private).
- **Earned-Trust enforcement (three orthogonal layers
  reproduced from ADR-0055 Verification)**: (a) subtype /
  compile-time check (the predicate's new `matches` arm
  references `record.body: String` per
  `crates/lumen/src/record.rs:54` and `self.body_regex:
  Option<Regex>`; removing the field, changing the type, or
  removing the `regex` direct dep fails the compile); (b) AST
  structural check via the acceptance suite's per-scenario
  literal references to `body_regex`, `invalid body_regex`,
  and `specify body_regex or body_contains, not both`; a
  mutant that drops the parse step, the dispatch arm, or the
  mutual-exclusion check is killed by the corresponding
  scenario; (c) behavioural gold-test via the slice-01 suite.
- Mutation testing: `cargo mutants --in-diff origin/main`
  scoped to the modified files via the existing
  `gate-5-mutants-log-query-api` and `gate-5-mutants-lumen`
  workflows at the 100% kill-rate gate (ADR-0005 Gate 5).
  Primary mutation targets enumerated in `user-stories.md` §
  Technical Notes / Mutation targets.

## External-integration handoff

None. The `regex` crate is a pure-computation in-process
library; the parse helper is in-process string compilation; the
store call uses an in-process trait method
(`lumen::LogStore::query_with`) against the durable
`FileBackedLogStore`, which is a first-party library, not a
network service. No third-party API is consumed; no
consumer-driven contract test recommendation. The existing
ADR-0047 Earned-Trust startup probe continues to run unchanged.

## Relationship to ADR-0047, ADR-0050, ADR-0052, ADR-0054, ADR-0055, and ADR-0046

- **ADR-0047** is the originating read-side contract for logs.
  Its bare JSON array success shape, its `{status:"error",
  error}` error envelope, its redaction posture, and its
  statement that `query_with(predicate)` exists on `LogStore`
  are ALL PRESERVED. Cited, NOT modified.
- **ADR-0050** is the read-side Earned-Trust caps. Decision 2,
  Decision 4, and Decision 7 are PRESERVED. The body-regex
  filter slots at the store call so the post-filter
  `Vec<LogRecord>` is what the cap measures. Cited, NOT
  modified.
- **ADR-0052** is the sibling `min_severity` optional parameter.
  ADR-0056 follows the same parse-and-wire shape; the
  conjunctive AND composition with `min_severity` is the
  established pattern. Cited, NOT modified.
- **ADR-0054** is the M-5 `query-http-common` extraction.
  ADR-0056 is the SECOND post-extraction consumer of the
  shared scaffold after ADR-0055; the cap consts, the four
  `REASON_*` consts, the `error_response` helper, the
  `resolve_tenant_or_refuse` seam, and the
  `parse_time_range` parser are ALL reused via
  `query_http_common::`. KPI-K4's static-grep CI assertions
  and the under-40-LOC line-count budget are the honest
  measure that the shared crate continues to pay for itself.
  Cited, NOT modified.
- **ADR-0055** is the immediate predecessor. ADR-0056 reuses
  the cap value (1024 bytes), the parse-helper shape
  (`Result<_, &'static str>`), the no-store-call assertion
  posture, the literal-envelope redaction posture, and the
  4-arm-to-6-arm dispatch growth pattern; ADR-0056 adds the
  mutual-exclusion check between `body_contains` and
  `body_regex` to prune the 8-arm cross product back to 6
  reachable arms. Cited, NOT modified.
- **ADR-0046** is the `query-api` regex label matchers ADR.
  ADR-0056 reuses the workspace's `regex` crate grammar
  (RE2-derived, linear-time), the compile-once-per-request
  shape, and the unanchored / single-line default. Cited, NOT
  modified.

## Forward-looking scope

Slice 01 ships the single-regex body filter on
`LogRecord.body`, RE2-derived, length cap 1024 bytes, mutually
exclusive with `body_contains`. Successor slices (separately
roadmapped, NOT this ADR's scope) MAY:

- Relax the mutual exclusion to AND-compose `body_contains`
  AND `body_regex` once a real operator use case earns the
  testing surface; the dispatch grows from 6 to 8 reachable
  arms and `Predicate::matches` carries both arms
  simultaneously.
- Add a pre-compiled regex cache across requests if measured
  compile cost dominates the per-request budget; the seam is
  inside `parse_body_regex`.
- Add matching against `severity_text`, `attributes`, or
  `resource_attributes` (each a separate slice).
- Add a separate `body_regex_ci=` parameter or a paired
  `case_sensitive=false` flag if the inline `(?i)` flag is
  insufficient for the operator workflow.
- Raise the length cap above 1024 bytes if real operator
  usage demands.
- Push the regex scan into the v1 columnar substrate's
  indexes (a Tantivy regex query, a posting list, a Bloom
  filter on n-grams); the predicate seam absorbs the change
  without surface diff.

Each successor change is a separate slice with its own ADR;
the cross-references will name ADR-0056 as the originating
`body_regex` parameter contract.
