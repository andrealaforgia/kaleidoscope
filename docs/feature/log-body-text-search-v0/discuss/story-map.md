# Story Map: log-body-text-search-v0

## User: Sara Mendez (on-call SRE, tenant `acme-prod`); Marcus Webb (platform engineer, automated incident classifier); Priya Raman (support engineer triaging customer reports)

## Goal: Narrow the log read at the HTTP boundary to records whose body contains a known substring, so that mid-incident triage, automated classification, and customer-ticket triage receive only the records carrying the error string in hand, in the same shape as today's response.

## Backbone

The slice's backbone is the canonical parse-and-wire shape established by
log-query-severity-filter-v0 and now executed for the fourth time on a
read-side endpoint (first time on this endpoint AFTER M-5
`query-http-common` shipped). Three columns:

| 1. Parse | 2. Wire | 3. Verify |
|---|---|---|
| US-04 reject empty `body_contains` (`?body_contains=` -> 400 with `invalid body_contains`); US-03 accept missing `body_contains` (None preserves today's behaviour). The parse arm runs AFTER `min_severity` parse and BEFORE the store touch. | US-01 dispatch a composed predicate through `query_with(&tenant, range, predicate)` carrying `body_contains` (and `min_severity` if also present). When no filter is present, fall through to `query(&tenant, range)`. The lumen `Predicate` either grows a `body_contains` builder (FLAG 3 recommended) or the slice applies the substring filter handler-side after `query`. | US-01 happy path narrows the response; US-02 unmatched substring returns the calm `[]`; US-05 case-sensitive matching pinned; US-06 cross-tenant isolation invariant holds. The result cap (UNCHANGED) measures the post-filter records vector. |

---

### Walking Skeleton

The slice is BROWNFIELD on `/api/v1/logs`. The walking skeleton was built
across four earlier slices: log-query-api slice 01 (read endpoint), slice
02 (caps), log-query-severity-filter-v0 (first optional parameter), and
M-5 (`query-http-common` extraction). No new skeleton is needed.

The thinnest end-to-end demonstration of THIS slice's value:

> Seed tenant `acme-prod` with a mix of records inside `[1716200000s,
> 1716200060s)`, including two records whose `body` contains `"kafka
> timeout"` and four records whose `body` does not. Issue
> `GET /api/v1/logs?start=1716200000&end=1716200060&body_contains=kafka%20timeout`.
> Receive HTTP 200 with a bare JSON array containing ONLY the two matching
> records, in ascending `observed_time_unix_nano` order. The four
> non-matching records are excluded.

This walks all three backbone columns at once: parse the new parameter
(col 1), dispatch through `query_with` with a composed predicate (col 2),
verify the response is the narrowed bare array (col 3). US-01 IS the demo
behaviour.

### Release 1 (slice 01, THIN): all six stories ship together

All six stories ship in a single slice because they are mutually dependent
proofs of one behaviour, not independent deliverables. The carpaccio
discipline here is the same as log-query-severity-filter-v0: a single
parse-and-wire slice on one optional parameter, with five mutually
reinforcing scenarios that lock the behaviour against named mutants and
named platform invariants.

| Story | Backbone column | Outcome KPI link | Notes |
|---|---|---|---|
| US-01 walking skeleton (`body_contains=kafka timeout` narrows the response) | 1, 2, 3 | KPI-1 (substring matches are honest) | The demoable behaviour. |
| US-02 unmatched substring returns the calm empty array (NOT 404, NOT 500) | 3 | KPI-1 (correctness on the empty arm) | Distinguishes "absent" from "malformed" from "broken". |
| US-03 default unchanged (no parameter -> today's behaviour) | 1 | KPI-2 (zero behaviour regression on existing requests) | Backward-compat contract. |
| US-04 empty `body_contains` is a redacted 400 | 1 | KPI-3 (`query-http-common` reuse; no envelope re-implementation) | Mirrors the unknown-severity 400 from ADR-0052. |
| US-05 case-sensitive matching pinned by acceptance test | 3 | KPI-4 (case-sensitivity discoverable) | Documents the platform's posture via a test. |
| US-06 cross-tenant isolation holds for `body_contains` | 2, 3 | KPI-2 (platform invariant unchanged) | Pins the existing per-tenant isolation invariant against the new filter arm. |

## Scope Assessment: PASS — 6 stories, 1 bounded context (`log-query-api`); 1 incidental lumen surface extension (FLAG 3) gated behind ADR-0055; estimated 1 day

Carpaccio gate signals (mirrors log-query-severity-filter-v0):

- 6 user stories (well under the 10-story threshold).
- 1 primary bounded context (`crates/log-query-api`). One INCIDENTAL
  surface extension on `crates/lumen` if FLAG 3 lands as recommended
  (`Predicate::body_contains` builder + one arm in `Predicate::matches`);
  scoped as additive, no breaking change, governed by Gate 2 `cargo
  public-api`.
- Walking skeleton requires 1 integration point (HTTP route -> existing
  `LogStore::query_with` seam).
- Estimated effort: 1 day end-to-end (parse one parameter, branch one
  handler arm, optionally extend `lumen::Predicate` with one builder + one
  arm, write six acceptance scenarios).
- Single user outcome: a body-substring-narrowed read on `GET /api/v1/logs`.

All five signals well below the oversized threshold. NO split needed.

## Priority Rationale

The six stories ship together because they are mutually dependent proofs
of one behaviour, not independent deliverables. The order below drives
acceptance-test enablement during DELIVER, per the one-at-a-time outer
loop convention already in use in
`tests/slice_01_logs_read.rs` and
`tests/slice_01_severity_filter.rs`:

1. **US-01 first** (skeleton). The riskiest assumption is that the
   existing `query_with` seam suffices when composed with a new
   `body_contains` filter. US-01 derisks it. Once US-01 is green, the
   slice is demoable.
2. **US-03 second** (default unchanged). The backward-compat contract.
   Derisks "did we accidentally change the no-parameter response, or the
   `min_severity`-only response?". A byte-equality assertion against the
   slice-prior fixture; if this is red, US-01 cannot ship.
3. **US-04 third** (empty-string 400). The only new error arm the slice
   introduces. Derisks the redaction posture and the parse-before-store
   discipline. Mirrors the unknown-severity 400 from
   log-query-severity-filter-v0.
4. **US-02 fourth** (calm empty). Derisks the empty post-filter arm; the
   `[]` shape is the existing contract from ADR-0047 Decision 1 and the
   slice MUST honour it.
5. **US-05 fifth** (case-sensitivity). Derisks the `>=` mutation surface
   of FLAG 2; the test pins the case-sensitive recommendation in
   acceptance.
6. **US-06 last** (cross-tenant). Derisks the platform invariant against
   the new arm. Comes last because the EXISTING `query_with` seam already
   enforces tenant-first lookup
   (`crates/lumen/src/store.rs:166-180`); the test is a regression net,
   not a behaviour change.

## Out-of-scope (DECLARED; carried into next slices)

The following are EXPLICITLY out for slice 01 and named so DESIGN does not
re-discover them as gaps:

- Regex matching on `body` (separate future feature
  `log-body-regex-search-vN`).
- Case-insensitive matching (FLAG 2; a future slice may add a separate
  `body_contains_ci=<string>` parameter or a `case_sensitive=false` flag).
- Matching across multiple fields (`body OR attributes`, etc.).
- Matching on `severity_text`, `attributes`, or `resource_attributes` (each
  a separate slice if and when it earns a third call site).
- Multiple substrings per request (`body_contains=foo,bar` or repeated
  parameters); the slice accepts ONE substring per request.
- Unicode normalisation (the slice compares Rust `String` bytes).
- Configurable maximum substring length.
- Narrowed-read adoption counter (`body_contains`-bearing request count
  vs total). Consistent with ADR-0050 Decision 8 (the platform ships no
  live observability of its own at v0/v1; the contract IS the signal).
- Post-filter record-count histogram. Same posture as above.
- Per-pillar cap tuning (ADR-0050 forward-looking scope).
- A new ADR-0055 written by DISCUSS. The recommendation is in FLAG 6
  (`wave-decisions.md`); DESIGN authors the ADR if it pins FLAG 3 as a
  lumen surface extension.
