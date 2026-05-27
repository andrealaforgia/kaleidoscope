# ADR-0050 — Earned-Trust on the read side: per-request window cap and result-size cap on all three read APIs

- **Status**: Accepted
- **Date**: 2026-05-27
- **Author**: `nw-solution-architect` (Morgan)
- **Feature**: `honest-read-caps-v0`
- **Supersedes**: none
- **Superseded by**: none
- **Related**: ADR-0042 (the metrics query-api contract, the
  `{status:"error", error}` envelope, the fail-closed tenancy, the
  Earned-Trust probe; cited as the originating read-side contract this
  ADR REFINES at the policy-of-refusal seam, NOT modified). ADR-0047
  (the lumen log-query-api contract; Decision 1 reproduces the envelope
  and the redaction posture; cited, NOT modified). ADR-0048 (the ray
  trace-query-api contract; Decision 1 reproduces the envelope with
  STRICTER redaction; Decision 5 defers the `query-http-common`
  extraction this ADR explicitly honours; cited, NOT modified).
  ADR-0049 (the immediately prior Earned-Trust sibling on the WRITE
  side: probe-must-honour-fsync; the same Earned-Trust principle
  applied to a different boundary; cited as the immediate precedent,
  NOT modified). ADR-0005 (the five CI gates including Gate 5 100%
  mutation kill on modified files; Gate 2 `cargo public-api` byte
  identity on store trait signatures).

## Context

The three read APIs (`query-api`, `log-query-api`, `trace-query-api`)
accept any `[start, end)` window and return any number of rows. The
current handlers (`crates/query-api/src/lib.rs:146`,
`crates/log-query-api/src/lib.rs:104`,
`crates/trace-query-api/src/lib.rs:115`) parse `start` and `end` and
reject non-numeric or inverted bounds via `parse_time_range`, but
impose NO upper bound on `end - start`. They then serialise WHATEVER
the store returns, with no upper bound on the number of rows. A
request with `start=0&end=31536000` (one calendar year) parses cleanly
and reaches the store; the store walks the in-window data and the
handler serialises the result. Best case: the listener saturates for
the duration of the read. Worst case (S04 / S14 amplification at the
read side): the process OOMs.

The residuality analysis
(`docs/product/architecture/residuality-analysis.md`, commit 50e20b5)
records this as the S13 row of the incidence matrix: every cell under
`QM` / `QL` / `QT` reads `D no upper bound on window`. The follow-up
roadmap (`docs/residuality-followups-roadmap.md`) lists this as item 2
of 3 (M-2). It is a SELF-DoS surface: a misconfigured client (a
hand-edited Grafana dashboard, an automated script with a wide
default), or an attacker probing, can melt the platform without
forging credentials or breaching the fail-closed tenancy of ADR-0042 /
0047 / 0048 or the regex-engine ReDoS residue of ADR-0046.

ADR-0049 made the Earned-Trust claim CODE on the WRITE side: a probe
that honours fsync, a write-path that actually calls `sync_all`. This
ADR makes the same claim CODE on the READ side: the platform refuses
**out loud** when the request asks for too much, BEFORE the store is
touched (window cap) and BEFORE serialisation (result cap), with the
same `{status:"error", error}` envelope Prism's `isPromError` already
handles for matcher-400 and bounds-400 errors. Honest caps convert
the slow / OOM path into a predictable 400 in the same envelope shape
the existing 400 arms use; the operator (or the client) sees the named
class of refusal and re-narrows.

ADRs in this repository are immutable (superseded, never edited).
ADR-0042 / 0047 / 0048 / 0049 are Accepted and referenced as
precedents; they are NOT modified. ADR-0050 is the next free number
(`ls docs/product/architecture/adr-0050*` returns no hits;
`adr-0049-earned-trust-honour-fsync.md` is the latest; 0050 is the
next).

## Decision

### 1. Window cap: 86_400 seconds (24 hours), uniform across the three crates

Every request to `/api/v1/query_range`, `/api/v1/logs`, or
`/api/v1/traces` whose `end_secs - start_secs > MAX_WINDOW_SECONDS`
(where `MAX_WINDOW_SECONDS = 86_400`) is refused with HTTP 400 and
the existing `{status:"error", error:"<reason>"}` envelope, where the
reason names the cap (the literal value is NOT echoed) before the
store is touched. The boundary is `>`, not `>=`: a window of exactly
`MAX_WINDOW_SECONDS` is served. The same numeric value applies to all
three crates at slice 01; per-pillar tuning (e.g. traces 1h because
`service` already narrows; logs 6h because logs are bytes-heavier per
record) is DEFERRED to a successor slice with measurement data the
v0/v1 platform does not yet have.

### 2. Result-size cap: 100_000 (rows / records / spans), uniform across the three crates

Every request whose response would carry more than
`MAX_RESULT_ROWS = 100_000` items (matrix entries for `query-api`,
`LogRecord`s for `log-query-api`, `Span`s for `trace-query-api`) is
refused with HTTP 400 and the existing
`{status:"error", error:"<reason>"}` envelope, where the reason names
the cap, **AFTER the store has answered** (so the platform knows the
size) and **BEFORE serialisation** (so the JSON encoding cost of the
over-cap result is not paid). The boundary is `>`, not `>=`: a
response of exactly `MAX_RESULT_ROWS` is served. The same numeric
value applies to all three crates at slice 01.

### 3. REFUSE, never TRUNCATE

The result-cap breach is a NAMED 400. **No `X-Truncated` header. No
partial 200. No silent empty.** A truncated 200 is the read-side
equivalent of a buffered fsync that lies (ADR-0049): the response
claims success while delivering a fraction of the matching set; the
client either misses the truncation signal (most do) and treats the
partial array as complete, or honours it and implements a "retry with
narrower window" loop the platform could simply have asked for. The
contract's existing three-way distinction (200 with data, 200 with
`[]` calm-empty, 4xx with named error) is preserved; the cap-400
slots into the third arm. Prism's `isPromError` already handles the
envelope. The decision is anchored by Earned Trust (Principle 12 of
this agent's methodology): an architecture that hides degradation is
dishonest with the people who use it.

### 4. Enforcement points: in the handler, NOT in the store

The two cap checks live in each handler at two specific seams:

- **Window check**: IMMEDIATELY AFTER `parse_time_range` returns
  `Ok(range)` and IMMEDIATELY BEFORE the call to
  `state.store.query(...)`. On `trace-query-api` the existing
  `read_required_service` 400 still fires BEFORE the window check
  (handler order preserved). The window check is its OWN gate; it is
  NOT folded into `parse_time_range` (the parser stays a parser; the
  cap-check is a policy decision distinct from input parsing).
- **Result check**: IMMEDIATELY AFTER the store returns AND AFTER any
  in-handler filtering (matrix translation for `query-api`) AND
  IMMEDIATELY BEFORE `success_response(...)`. The check measures
  what the user observes (matrix entries for metrics; bare-array
  records / spans for logs / traces), not the upstream raw row count.

This placement preserves the carpaccio invariant the DISCUSS wave
named: a `LyingStore` whose `query()` returns `PersistenceFailed`
must produce a cap-400 (not a 500) on an over-window request; the
store is NEVER touched on the cap-refusal path.

### 5. NO store-trait change; the duplication across the three crates is deliberate

The cap checks ride OUTSIDE the storage traits. `pulse::MetricStore`,
`lumen::LogStore`, `ray::TraceStore` signatures are byte-identical to
the prior tag (Gate 2 `cargo public-api` confirms). The three
handlers carry independent cap-check copies, even though the shape is
identical, because each handler keeps its own `TimeRange` type
(`pulse::TimeRange`, `lumen::TimeRange`, `ray::TimeRange`) and the
cross-cutting `query-http-common` extraction is the explicit
deferral named in ADR-0048 Decision 5 (M-5 in the residuality
follow-up roadmap; a SEPARATE future feature). Slice 01 honours that
deferral. Once `query-http-common` exists, the cap pattern is the
natural first thing to live there.

### 6. Caps signature: two `pub const` per crate's `lib.rs`

In each of `crates/query-api/src/lib.rs`,
`crates/log-query-api/src/lib.rs`, and
`crates/trace-query-api/src/lib.rs`, alongside the existing route
constants:

```rust
pub const MAX_WINDOW_SECONDS: u64 = 86_400;
pub const MAX_RESULT_ROWS: usize = 100_000;
```

The constants are `pub` so the acceptance suite can address them by
name (the boundary tests at `MAX_WINDOW_SECONDS` and
`MAX_WINDOW_SECONDS + 1`; the result-cap analogues). Slice 01 has:
**NO shared crate** (ADR-0048 Decision 5 deferral); **NO config
struct** (no runtime tuning); **NO env override** (no
`KALEIDOSCOPE_QUERY_MAX_WINDOW_SECONDS` family at slice 01; explicitly
named OUT in the DISCUSS wave). A successor slice MAY lift the
constants to env-driven configurability via the existing
`composition::resolve_tenant` posture.

### 7. Redaction posture: SYMMETRIC with each crate's existing posture

The two new cap reasons inherit each crate's existing redaction
posture, asserted by mirror tests of the existing
`the_bounds_error_never_echoes_the_raw_value` /
`the_service_error_never_echoes_the_raw_service_value_or_a_credential`
patterns:

- `query-api`: the cap-400 body contains no raw `start`, no raw `end`,
  no raw query text, no raw regex pattern, no forwarded
  `Authorization` / `Bearer` value, no "SECRET".
- `log-query-api`: same as `query-api` minus the query/pattern parts;
  no raw window values, no forwarded credential.
- `trace-query-api`: stricter; the body must NOT contain "SECRET" or
  "Bearer" anywhere, and never the raw `service` value.

The cap reason names the breached class ("window exceeds maximum",
"result exceeds maximum" or the crafter's named equivalent within the
constraint) without echoing the numeric value of the cap or of the
request; the reason text is stable across cap-value tunings.

### 8. NO new event, NO new metric, NO new dashboard

The 400 envelope IS the signal; the operator sees the named class
and re-narrows. No counter is incremented; no structured event is
emitted on a cap refusal beyond the existing `tracing::error!` calls
already present for store failures (and the cap refusal does NOT
emit a store-error tracing event because the store was not the
cause). At v0/v1 the platform has no live observability stack of its
own; a refusal IS the signal.

## Uniform error envelope

The two new 400 arms reuse the existing
`{status:"error", error:"<reason>"}` envelope:

```json
{"status": "error", "error": "window exceeds maximum"}
```

(or `"result exceeds maximum"` for the result cap; or the crafter's
equivalent named class within the redaction constraint of decision 7).

The envelope is the same shape ADR-0042 lines 220-229 pin as
Prism's `isPromError`-handled error class; ADR-0047 Decision 1 and
ADR-0048 Decision 1 reproduce the envelope for the log and trace
APIs. **No new envelope shape, no new status code, no new error
class is introduced by this ADR.**

## Alternatives considered

### Window cap value A (rejected): 6 hours (21_600 seconds)

The most conservative bound. For: the smallest window protects against
the widest range of misconfigured clients; aligns with an interactive
ad-hoc-query default. Against: cuts the typical Grafana / Prism "24h
today" panel below its natural span; would generate false positives
the operator routes around by raising the cap, defeating the cap
rather than honouring it. Rejected.

### Window cap value B (rejected): 7 days (604_800 seconds)

The most permissive bound. For: covers a week-long incident window;
fits the trace pillar's `service`-narrowed fan-out; reduces false
positives. Against: large enough that even with the result cap the
worst-case fan-out comfortably saturates the listener at v0/v1; the
v0/v1 platform has no telemetry to calibrate this against. Rejected
as too generous against an untested lifetime.

### Window cap value C (accepted): 24 hours (86_400 seconds)

The residuality analysis's own named default. The smallest cap that
does NOT cut a normal "today" dashboard layout; the cap above which
the operator's intent is "archival sweep" rather than "live
dashboard". Accepted.

### Result-cap value A (rejected): 10_000

Tight enough for an interactive UI panel. For: defends the read path
against any plausible fan-out. Against: a typical metrics query with
normal label cardinality (say 20 labels with two to ten distinct
values each) can fan out beyond 10_000 matrix entries on a 24h
window without the user doing anything pathological; the cap would
bite legitimate queries. Rejected.

### Result-cap value B (rejected): 1_000_000

Generous bound for batch/export use cases. For: covers most exports
without a cap-refusal. Against: at one kilobyte per row (small for
logs and spans; large for metric matrix entries), a million-row
response is a gigabyte of JSON allocated in the handler before
serialisation; the platform has no streaming JSON encoder at the read
side today; the entire vector is allocated, serialised, then sent. A
gigabyte is OOM territory on small dev hardware. Rejected as unsafe
at v0/v1.

### Result-cap value C (accepted): 100_000

The residuality analysis's named order of magnitude; the typical
sweet spot in similar systems (Prometheus remote-read chunk size,
Loki query-range default, Tempo per-trace limit). At one kilobyte per
row, 100_000 is a 100 MB worst case, still serialisable. A reads-fixture
check confirmed no typical existing test fixture in any of the three
crates exceeds this (the widest test fixture seeds five records; the
threshold is a factor of 20_000 above any current acceptance test).
Accepted.

### Result-cap response A (rejected): TRUNCATE with 200 + `X-Truncated: true`

Convenient for dashboards that want a "first N rows + truncation
badge". For: cheaper for the client; preserves the response on a
borderline-oversized query. Against: violates Earned Trust (the
response claims success while delivering a fraction); silently
collapses the contract's three-way 200/200-empty/4xx distinction
(ADR-0042 / 0047 / 0048 all pin it); the client either misses the
header and acts on partial data, or honours it and implements a
narrowing-retry loop the platform could simply have asked for.
Rejected.

### Result-cap response B (accepted): REFUSE with named 400

Consistent with the existing matcher-400 / bounds-400 envelope;
preserves the three-way contract distinction; the operator sees the
named class and re-narrows. Accepted.

### ADR shape A (rejected): amend ADR-0042 / 0047 / 0048 individually

Three ADR amendments recording the cap policy on each contract
separately. For: the cap lives in each contract. Against: ADRs are
immutable in this repository (ADR-0049 Context paragraph 4); the
amendment would create three separate documents that drift over
time; the cap policy is cross-cutting (the SAME two caps across the
three crates) and belongs in ONE place. Rejected.

### ADR shape B (rejected): NEW ADR-0050 PLUS three amendments to 0042 / 0047 / 0048

For: maximum cross-reference density. Against: three amendments
violate the immutability rule; the cross-reference is already
sufficient (this ADR cites ADR-0042 / 0047 / 0048 / 0049 explicitly,
with section pointers). Rejected as gratuitous bookkeeping.

### ADR shape C (accepted): NEW ADR-0050 alone; ADR-0042 / 0047 / 0048 / 0049 cited and NOT modified

One ADR records the cross-cutting cap policy; the four precedents are
cited with section pointers; immutability is preserved. Accepted.

### Enforcement A (rejected): cap-check inside `parse_time_range`

Folding the window cap into `parse_time_range` saves one `if`
statement per handler. For: smaller diff. Against: blurs the boundary
between parse-error (malformed input) and policy-refusal (well-formed
input but too wide); a future tweak to the cap value would touch the
parser, which is also responsible for the existing
non-numeric / inverted 400 arms; the parser stays a parser. Rejected.

### Enforcement B (rejected): push a `limit` argument into the three store traits

Push the result-cap into `MetricStore::query` / `LogStore::query` /
`TraceStore::query` so the store stops scanning at the limit. For:
lower memory peak on the store side. Against: requires changing three
store trait signatures (Gate 2 `cargo public-api` would catch and
fail; would force coordinated changes in the ingest crates and the
store implementations); ADR-0048 Decision 5 already deferred the
cross-cutting refactor; v0/v1 takes the simpler handler-side
enforcement and records the deferral. Rejected for slice 01.

### Slice scope A (rejected): one crate first, the other two later

Land caps on `query-api` only; do `log-query-api` and
`trace-query-api` as follow-up slices. For: smallest blast radius for
the first PR. Against: leaves S13 partially closed; the roadmap
explicitly bundled the three crates into one feature ("the three
crates share the cap pattern even though they keep their own
time-range types"). Rejected.

### Slice scope B (accepted): three crates in one slice

All three crates ship the same cap pattern in one walking-skeleton
slice; compile-time constants; redaction tests on the new reasons.
Accepted.

## Consequences

### Positive

- **The Earned-Trust claim becomes code on the READ side.** Where
  ADR-0049 made fsync-honesty real on the WRITE side, ADR-0050 makes
  refusal-on-overreach real on the READ side. The S13 row of the
  residuality incidence matrix transitions from `D no upper bound on
  window` to `S window cap refuses at the handler` for all three
  read-API columns (QM, QL, QT) in one slice.
- **No new envelope, no new event, no client-side change.** Prism's
  `isPromError` already handles the existing `{status:"error", error}`
  envelope; the cap-400 is a new reason string inside an existing
  shape.
- **No store-trait change, no blast radius.** `pulse::MetricStore`,
  `lumen::LogStore`, `ray::TraceStore` trait signatures are
  byte-identical to the prior tag.
- **Operational legibility.** The operator sees a named class of
  refusal; the client narrows the window, the matchers, or the
  service; the next request serves. The 400 IS the signal; no
  dashboard wiring or alert threshold is required.
- **Redaction posture preserved.** The two new reasons honour each
  crate's existing redaction posture; A-U3 (header echo in error
  bodies) stays blocked at the new 400 arms.
- **Per-feature mutation 100% on the modified files** (ADR-0005 Gate
  5; CLAUDE.md). Existing `gate-5-mutants-query-api`,
  `gate-5-mutants-log-query-api`, `gate-5-mutants-trace-query-api`
  cover the changed lines via `--in-diff`; no new CI job.

### Negative

- **Caps are compile-time at slice 01.** A misjudged value (too tight,
  too generous) requires a code change and a redeploy. The mitigation
  is a successor slice that lifts the constants to env-driven via the
  existing `composition::resolve_tenant` posture; explicitly named in
  the DISCUSS OUT list.
- **Uniform across the three crates.** Per-pillar differentiation
  (e.g. traces 1h, logs 6h, metrics 24h) would be more honest about
  per-pillar cardinality expectations but requires measurement data
  the v0/v1 platform does not yet have. A successor slice tunes per
  crate when telemetry exists.
- **The duplication across the three crates is deliberate.** Three
  cap-check copies live in three handlers because the
  `query-http-common` extraction is deferred (ADR-0048 Decision 5).
  Future maintainers should resist factoring the cap-check early
  without also extracting the time-range types.
- **A within-window request can still be slow if the result is just
  under the cap.** 99_999 matrix entries / log records / spans is
  served in full at v0/v1; the cap defends against catastrophic
  overreach, not gradual response inflation. A successor slice may
  introduce a streaming encoder or a paginated endpoint with its own
  contract.

### Trade-off summary

The refinement is intentionally narrow: it adds two compile-time
constants and two `if` statements per handler, with redaction-symmetric
named-cap reasons. The trade-off is "configurability now" against "an
honest, testable, deployable cap policy now". v0/v1 takes the latter
and records every deferral.

## Verification

- A workspace grep for `MAX_WINDOW_SECONDS`, `MAX_RESULT_ROWS`, and
  the cap-check reason strings in `crates/query-api/src/lib.rs`,
  `crates/log-query-api/src/lib.rs`,
  `crates/trace-query-api/src/lib.rs` returns the two `pub const`
  declarations per crate AND two `if`-arm hits per crate after slice
  01 lands. Today: zero hits.
- The slice-01 acceptance suite per crate (a new test file under
  `crates/<crate>/tests/`, DISTILL-wave output) exercises:
  - Within-cap happy path (window strictly less than
    `MAX_WINDOW_SECONDS`, result strictly less than
    `MAX_RESULT_ROWS`).
  - Over-window 400 with `LyingStore` whose `query()` returns
    `PersistenceFailed`; the test asserts the cap-400 fires AND the
    lying store's `query()` was NEVER called.
  - Over-result 400 with a real `FileBack...Store` seeded with
    `MAX_RESULT_ROWS + 1` records inside an in-cap window; the test
    asserts the cap-400 fires, NO `X-Truncated` header, NO partial
    200, NO silent empty.
  - Boundary inclusive at `MAX_WINDOW_SECONDS` (served) and exclusive
    at `MAX_WINDOW_SECONDS + 1` (refused); same for the result cap.
  - Redaction on the two new cap reasons; symmetric with each crate's
    existing posture (the `trace-query-api` reason additionally
    excludes "SECRET", "Bearer", and the raw `service`).
  - On `trace-query-api` specifically: the missing-service 400 still
    fires BEFORE the new window cap 400 (handler order preserved).
- Gate 2 `cargo public-api` confirms `pulse::MetricStore`,
  `lumen::LogStore`, `ray::TraceStore` trait signatures are
  byte-identical to the prior tag. The two new `pub const` per crate
  appear in the public-API diff as new informational items, NOT
  breaking changes.
- **Earned-Trust enforcement at the read-side (three orthogonal
  layers reproduced from ADR-0049 Verification)**: (a) subtype check
  at compile time (the cap check is two `if` statements over the
  `pub const` values; removing the constants fails the build); (b) AST
  structural check via the test file's compile-time reference to
  `MAX_WINDOW_SECONDS` and `MAX_RESULT_ROWS` (a successor pre-commit
  hook can assert the constants are referenced in each crate's tests
  in a future slice; at slice 01 the cargo build IS the check); (c)
  behavioural gold-test exercising the over-window and over-result
  paths via real and lying stores (the acceptance suite above). A
  single-layer bypass is caught by at least one of the other two.
- Mutation testing: `cargo mutants` scoped to the modified files via
  the existing `gate-5-mutants-query-api`,
  `gate-5-mutants-log-query-api`, `gate-5-mutants-trace-query-api`
  workflows at the 100% kill-rate gate (ADR-0005 Gate 5; CLAUDE.md).
  Primary mutation targets per crate:
  - The window-cap `>` boundary (a `>` -> `>=` mutant must be killed
    by the `MAX_WINDOW_SECONDS` boundary inclusive test; a `>` -> `<`
    mutant must be killed by the over-by-one test).
  - The result-cap `>` boundary (same shape).
  - The order-of-checks (a mutant that swaps window-cap and
    store-query is killed by the lying-store assertion that `query`
    was NOT called on the over-window path).
  - The cap reason strings (a mutant that empties or alters the
    named reason is killed by the redaction tests and the
    reason-substring assertions).

## External-integration handoff

None. The two cap checks are in-process arithmetic over an already-
parsed `u64` window and a `Vec::len()` result count. No third-party
API is consumed by the cap path; no new external dependency is
introduced. The existing read-API contracts under ADR-0042 / 0047 /
0048 are unaffected on the within-cap path; their store probes
(`composition::probe()` in the read APIs; the new `fsync_probe` from
ADR-0049 on the write side) continue to run unchanged.

## Relationship to ADR-0042, ADR-0047, ADR-0048, ADR-0049

- **ADR-0042** is the originating read-side contract for metrics. Its
  `{status:"error", error}` envelope (lines 220-229), its fail-closed
  tenancy (Decision 7), and its Earned-Trust probe (Decision 8) are
  all PRESERVED. The cap-400 reuses the envelope verbatim and the
  fail-closed tenancy STILL fires FIRST (`handle_query_range` line
  151). Cited, NOT modified.
- **ADR-0047** is the lumen log read-API contract. Its envelope and
  redaction posture (Decision 1) are PRESERVED; the cap-400 inherits
  the redaction. Cited, NOT modified.
- **ADR-0048** is the ray trace read-API contract. Its stricter
  redaction posture (Decision 1: no "SECRET" or "Bearer" anywhere in
  the body) is PRESERVED; the cap-400 in `trace-query-api` honours
  it. ADR-0048 Decision 5 deferred the `query-http-common`
  extraction; this ADR explicitly honours that deferral by
  duplicating the cap-check across the three handlers instead of
  introducing a shared crate. Cited, NOT modified.
- **ADR-0049** is the immediate Earned-Trust sibling on the WRITE
  side: a probe that honours fsync, a write path that actually calls
  `sync_all`. ADR-0050 applies the same principle on the READ side:
  refusal-out-loud on overreach. The two ADRs are the two halves of
  Earned-Trust-as-code at v0/v1: ADR-0049 on durability, ADR-0050 on
  refusal. Cited, NOT modified.

## Forward-looking scope

Slice 01 ships the compile-time-constant caps across the three
crates. Successor slices (separately roadmapped, NOT this ADR's
scope) MAY:

- Lift the constants to env-driven configurability via the
  existing `composition::resolve_tenant` posture
  (`KALEIDOSCOPE_QUERY_MAX_WINDOW_SECONDS`,
  `KALEIDOSCOPE_QUERY_MAX_RESULTS`, and the per-crate siblings).
- Differentiate the cap values per pillar once telemetry exists
  (e.g. traces 1h, logs 6h, metrics 24h).
- Extract the cap-check into `query-http-common` once the
  rule-of-three for cross-cutting handler concerns is paid (M-5 in
  the residuality follow-up roadmap; ADR-0048 Decision 5).
- Introduce a streaming JSON encoder or a paginated endpoint for
  legitimate large-export use cases (a SEPARATE contract under its
  own ADR, NOT a silent change to the existing routes).

Each successor change is a separate slice with its own ADR; the
cross-references will name ADR-0050 as the originating cap policy.
