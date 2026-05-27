# Wave Decisions: honest-read-caps-v0 (DISCUSS)

British English. No em dashes. No emoji.

## Origin and frame

This is M-2 in the residuality analysis
(`docs/product/architecture/residuality-analysis.md`, commit 50e20b5)
and item 2 in the residuality follow-up roadmap
(`docs/residuality-followups-roadmap.md`, commit 820176d). M-1
(`earned-trust-fsync-probe-v0`) just closed. The roadmap is explicit
that M-2 lands one feature wave after M-1.

The three read APIs (`query-api`, `log-query-api`, `trace-query-api`)
have NO per-request window cap and NO result-size cap. A request
asking for a one-year window, or a request whose store fan-out yields
millions of rows, traverses the store, allocates the matching rows /
records / spans in memory, serialises them, and returns them. The
incidence matrix in the residuality analysis records this as the S13
row (and adds S04 / S14 amplification at the read side): every cell
under `QM` / `QL` / `QT` reads `D no upper bound on window`. This is
a self-DoS surface; a misconfigured client (or an attacker probing)
can melt the platform without forging credentials or breaching the
fail-closed tenancy or the regex-engine ReDoS residue.

Honest caps convert the slow / OOM path into a predictable 400 with
the same envelope shape the existing matcher and bounds 400s already
use:
`{status:"error", error:"<reason>"}`. The error text never echoes the
forwarded header, the raw query, the raw pattern, or the raw
`service` value; redaction is consistent with ADR-0048's stricter
posture.

### Reads checklist

- [x] `docs/product/architecture/residuality-analysis.md` (M-2
  framing in "Resilience modifications (prioritised)"; the S13 row of
  the incidence matrix; the bullet "the **S13 columns QM / QL / QT**
  all degrade" under "Notable cells"; the v0/v1 closing summary
  "**capacity residues** (no caps on window or result size, no
  cardinality watermark, no fsync probing)").
- [x] `docs/residuality-followups-roadmap.md` (this feature's
  position as item 2 of 3, the rationale "the three crates share the
  cap pattern even though they keep their own time-range types", and
  the ground rule "full nWave per feature").
- [x] `crates/query-api/src/lib.rs` (the `handle_query_range` handler
  at line 146; the existing `parse_time_range` 400 arm at line 161;
  the `error_response` envelope at line 249 emitting
  `{status:"error", error:"<reason>"}`).
- [x] `crates/query-api/src/composition.rs` (the env-driven posture:
  `KALEIDOSCOPE_QUERY_TENANT`, `KALEIDOSCOPE_QUERY_ADDR`,
  `KALEIDOSCOPE_QUERY_STATIC_DIR`; the precedence pattern any new
  `_MAX_*` env would mirror).
- [x] `crates/log-query-api/src/lib.rs` (the `handle_logs` handler
  at line 104; `parse_time_range` at line 139; `error_response` at
  line 183; the comment "Parse and validate the window BEFORE the
  store is touched: a malformed or inverted window is a 400 that
  never runs a query" naming the exact place the window cap belongs).
- [x] `crates/log-query-api/src/composition.rs` (the same env-driven
  posture: `KALEIDOSCOPE_LOG_QUERY_TENANT`,
  `KALEIDOSCOPE_LOG_QUERY_ADDR`).
- [x] `crates/trace-query-api/src/lib.rs` (the `handle_traces`
  handler at line 115; the sibling pattern where the `service`
  required parameter is already validated at line 133 BEFORE the
  store, naming the exact spot the window cap and the result cap
  fit; `error_response` at line 223 with stricter redaction).
- [x] `crates/trace-query-api/src/composition.rs` (env posture
  `KALEIDOSCOPE_TRACE_QUERY_TENANT`, `KALEIDOSCOPE_TRACE_QUERY_ADDR`).
- [x] `docs/product/architecture/adr-0042-query-api-contract-and-promql-subset.md`
  (the existing 400 arms for parse-error and inverted-bounds; the
  matcher 400 the cap 400 mirrors in envelope shape).
- [x] `docs/product/architecture/adr-0047-lumen-log-query-api-contract-and-crate-layout.md`
  (Decision 1 reproducing the `{status:"error", error}` envelope and
  the redaction posture).
- [x] `docs/product/architecture/adr-0048-ray-trace-query-api-contract-and-crate-layout.md`
  (Decision 1 reproducing the same envelope with the stricter
  redaction; the comment "the error text never contains a forwarded
  Authorization / SECRET / Bearer token" the cap 400 must honour).
- [x] `docs/feature/earned-trust-fsync-probe-v0/discuss/` (M-1
  precedent: structure, voice, the FLAG-not-DECIDE pattern, the
  walking-skeleton entry-point shape).

The residuality analysis's framing of the gap is consistent with what
the three handlers do today. The current `parse_time_range` rejects
a non-numeric or inverted bound BEFORE the store is touched, but
imposes no upper bound on the window size; the store is then asked
for `[start, end)` over whatever range arrived, however large, and
the store returns whatever rows it has, however many. No
contradiction was discovered during DISCUSS.

## DIVERGE status

No DIVERGE artefacts at `docs/feature/honest-read-caps-v0/diverge/`.
The job statement is taken from the residuality analysis and the
roadmap: "refuse out loud when a read request exceeds a sensible
window or result size, before the store is touched". JTBD was
explicitly NOT requested by the invoking prompt; the journey is
grounded in the residuality analysis and the existing ADR vocabulary
(the 400 arm envelope shape, the redaction posture) instead.

Risk noted: without DIVERGE there is no separate ODI scoring; the
opportunity priority is taken from the roadmap rather than derived.
This is proportionate: this is item 2 in a numbered three-item
roadmap, not a competition between candidate features.

## Scope: SLICE 01 THIN (walking skeleton)

ONE feature wave puts TWO caps on ALL THREE read API crates,
together, because the cap pattern is the same shape even though the
three crates keep their own `TimeRange` types (`pulse::TimeRange`,
`lumen::TimeRange`, `ray::TimeRange`):

- **WINDOW CAP**. The request's `[start, end)` cannot span more than
  N seconds. If `end - start > N`, the handler returns 400 with
  `{status:"error", error:"window exceeds N seconds"}` (or the
  equivalent named reason; the exact wording is a DESIGN detail)
  BEFORE the store is touched. The cap is enforced at the same place
  the existing inverted-bounds 400 lives: immediately after
  `parse_time_range` succeeds, before the call into
  `store.query(...)`.
- **RESULT-SIZE CAP**. The response cannot contain more than M rows
  (`query-api` matrix entries, `log-query-api` `LogRecord`s,
  `trace-query-api` `Span`s). The breach is a 400 (the LIKELY
  recommendation; see FLAG 3) rather than a truncation: an honest
  refusal is consistent with the platform's "no fabricated empty"
  posture (A-D6 in the analysis). Where the cap is enforced (after
  the store query but before serialisation, or via a `limit + 1`
  trick at the store boundary if a store predicate exists) is a
  DESIGN detail.

Same error envelope shape as the existing matcher 400s and the
existing inverted-bounds 400s: `{status:"error", error:"<reason>"}`.
No raw value echo of the requested window (no echoing of `start` /
`end`), of the raw query (in `query-api`), of the regex pattern
(redaction precedent from ADR-0046 / ADR-0042 Decision 9), of the
raw `service` value (redaction precedent from ADR-0048 Decision 1),
or of a forwarded Authorization header. Never silent degradation to
empty: an over-spec request is a NAMED 400, not a calm 200 `[]`.

The residuality analysis estimated ~30 LOC per crate. Slice 01
covers all three together because they share the cap pattern, but
each crate keeps its own `TimeRange` type: the `query-http-common`
extraction deferred by ADR-0048 Decision 5 is explicitly NOT
anticipated here (M-5 in the analysis is a separate v1-roadmap item).

### OUT of scope (deferred and DECLARED)

- **Runtime-tuned caps**. Caps are compile-time constants for slice
  01 (per-crate `const MAX_WINDOW_SECONDS: u64 = ...;` and
  `const MAX_RESULT_ROWS: usize = ...;`, or their equivalent). The
  env-driven configurability (e.g.
  `KALEIDOSCOPE_QUERY_MAX_WINDOW_SECONDS`, `_MAX_RESULTS`) that the
  residuality analysis mentioned as the eventual shape is DEFERRED
  to a later slice or successor feature. Slice 01 ships honest caps;
  a future slice makes them tunable.
- **Telemetry / metrics on refusals**. Beyond the existing error
  envelope, no counter, no dashboard, no new structured event. A
  refusal IS the signal; the operator sees the 400 and knows the
  cap fired.
- **Any change to the tower `oneshot` test pattern**. Acceptance
  tests for the new 400 arm mirror the shape of the existing
  matcher-400 and inverted-bounds-400 tests in each crate (see
  ADR-0048 lines 393-404 for the precedent shape).
- **Any renegotiation with Prism**. Prism receives an honest 400,
  the same envelope shape its `isPromError` already handles for
  matcher errors (ADR-0042 lines 220-229). No downgraded-to-success
  trick, no special header, no special status.
- **Cross-cutting `query-http-common` extraction**. M-5 in the
  residuality analysis (and ADR-0048 Decision 5) explicitly defers
  the rule-of-three refactor. Slice 01 will duplicate the cap check
  three times because the three `TimeRange` types differ; the
  duplication is the deliberate cost ADR-0048 named.
- **Caps on parameters other than window-span and result-size**.
  No cap on the regex matcher complexity (the RE2 engine is already
  the residue for ReDoS, ADR-0046). No cap on the `service` length
  in traces. No cap on the number of label matchers. These are
  separate cost characteristics with separate trade-offs and are
  outside the M-2 framing.

## Walking-skeleton entry point

The three EXISTING HTTP endpoints, exercised via the tower `oneshot`
acceptance-test pattern each crate already uses:

- `GET /api/v1/query_range?query=...&start=...&end=...` on
  `query-api`. The Prometheus matrix endpoint.
- `GET /api/v1/logs?start=...&end=...` on `log-query-api`. The bare
  JSON array endpoint.
- `GET /api/v1/traces?service=...&start=...&end=...` on
  `trace-query-api`. The bare JSON array endpoint with the service
  filter already validated.

The After line of each story names one of these endpoints, with the
observable output being the 400 body
`{status:"error", error:"<names the breached cap>"}` at the listener
the existing tower `oneshot` pattern already binds.

## Flagged to DESIGN (DISCUSS does NOT decide these)

1. **EXACT WINDOW CAP VALUE**. Candidates surfaced:
   - 6 hours (24 * 3600 / 4 seconds) - the most conservative; the
     same value the residuality analysis used as a default upper
     bound for ad hoc queries against an interactive front-end.
   - 24 hours (86_400 seconds) - the residuality analysis's named
     starting default ("e.g. 24h default"). The default a metrics
     dashboard typically requests.
   - 7 days (604_800 seconds) - more permissive; fits trace-query
     better because a service filter narrows the result.
   The right value depends on per-pillar cardinality expectations:
   `pulse` (per-series points) can probably tolerate longer than the
   others; `trace-query-api` already narrows by `service` so the
   fan-out is narrower. The SAFEST default for slice 01 is the
   SAME value across all three crates, with the freedom for DESIGN
   to differ them later. DISCUSS recommends DESIGN pick one value
   (likely 86_400 = 24h) and apply it to all three; DESIGN owns the
   pick.

2. **EXACT RESULT-SIZE CAP VALUE**. Candidates surfaced:
   - 10_000 rows / records / spans - tight; fits an interactive UI
     comfortably.
   - 100_000 - the residuality analysis's named order of magnitude.
   - 1_000_000 - the upper bound at which "this is too much to
     serialise to JSON over HTTP in one response" becomes obviously
     true.
   The right value depends on the same per-pillar expectations as
   the window cap, plus the JSON serialisation cost per row (a
   `LogRecord` carries more bytes than a matrix entry, a `Span`
   typically more than a `LogRecord`). DISCUSS recommends DESIGN
   pick ONE value across all three (likely 100_000) and adjust per
   crate only if a concrete measurement supports it.

3. **REFUSE vs TRUNCATE on result-cap breach**. The two honest
   options:
   - **REFUSE with 400** (LIKELY recommendation): the client knows
     the query was wrong-sized; the response is the
     `{status:"error", error:"<names the cap>"}` envelope; the
     client either narrows the window, narrows the matchers (for
     `query-api`), or narrows the `service` filter (for
     `trace-query-api`). Consistent with "no fabricated empty"
     (A-D6) and with the existing matcher-400 / bounds-400
     posture.
   - **TRUNCATE with 200 + `X-Truncated: true`**: convenient for
     dashboards that want to display "the first 100k spans" and
     a "this is truncated" badge. Cheaper for the client but
     breaks the contract's clean three-way "200 / 200 empty / 4xx"
     story: a truncated 200 is a 200 that LIES about completeness.
   DISCUSS's LIKELY recommendation to DESIGN is REFUSE with 400.
   Stated as a flag, not a decision; DESIGN owns the pick and
   records the rationale in the ADR (FLAG 4).

4. **NEW ADR or REFINEMENT of an existing ADR**. The caps belong to
   the read-API contract. The shape of the change is a new section
   in the three contracts (ADR-0042, ADR-0047, ADR-0048) saying
   "the window cap is N seconds; the result cap is M rows; over the
   cap is 400 with this envelope". Three candidate paths:
   - **NEW ADR-0050** documenting the cap policy as a cross-cutting
     read-API contract refinement (likely; the change spans three
     contracts).
   - **REFINEMENT** of ADR-0042 / 0047 / 0048 individually (heavier;
     three ADR amendments).
   - **NEW ADR-0050 + amendments to 0042 / 0047 / 0048** (the
     widest blast radius; more bookkeeping than the change
     warrants).
   DISCUSS's LIKELY recommendation is ONE new ADR (ADR-0050) that
   names the cap policy and is cross-referenced from the three
   contracts. Stated as a flag, not a decision; DESIGN confirms ADR
   number and writes the ADR.

These four items are FLAGGED, NOT DECIDED, by DISCUSS.

## Learning hypothesis

We believe that TWO compile-time caps (window-span and result-size)
applied to ALL THREE read APIs at the same parse-then-check seam the
existing inverted-bounds 400 already lives at, returning the
existing `{status:"error", error:"<reason>"}` envelope BEFORE the
store is touched (window cap) or AFTER the store result is known but
BEFORE serialisation (result-size cap), will close the S13 self-DoS
surface without changing the store traits, without telemetry, and
without renegotiation with Prism.

We will know we are right when:

- A request to any of the three endpoints with `end - start` greater
  than the configured cap returns 400 with the named envelope and
  NEVER calls the store (asserted by an acceptance test that injects
  a `LyingMetricStore` / `LyingLogStore` / `LyingTraceStore` and
  observes that the store's `query` method is not invoked).
- A request whose store result would exceed the configured result
  cap returns 400 with the named envelope (LIKELY recommendation;
  see FLAG 3) and NOT a truncated 200, and not a 200 `[]`.
- The error body never contains the raw window values (`start`,
  `end`), the raw query, the raw pattern, the raw `service`, or any
  forwarded Authorization header (redaction symmetric to ADR-0048
  Decision 1).
- The per-crate mutation gate stays at 100 percent on the changed
  files (ADR-0005 Gate 5).

We will know we are wrong if:

- The chosen window cap value is too tight for legitimate dashboard
  use (false positives on real queries) - escalation path: re-pick
  the value in a successor slice, or move the cap to env-driven
  config a slice later.
- The chosen result cap value is too tight for legitimate exports
  (false positives on real exports) - escalation path: same.
- DESIGN concludes the right shape for the result cap is TRUNCATE
  rather than REFUSE - re-frame FLAG 3 in DESIGN and document the
  rationale in the new ADR; the DISCUSS-time LIKELY recommendation
  was REFUSE, not a decision.

## Risks

| Risk | Probability | Impact | Mitigation |
|---|---|---|---|
| Chosen window cap is too tight, blocking legitimate queries. | Medium | High (UX regression on Prism dashboard). | Slice 01 caps are compile-time constants; tightening or loosening is a one-line PR. The env-driven configurability is explicitly deferred to a later slice (declared OUT). |
| Chosen result cap is too tight, blocking legitimate exports. | Medium | Medium (workaround: client narrows window or service). | Same. |
| Result cap enforcement requires a store-trait method (e.g. `query_limit`) that does not exist today. | Low | Medium (would expand blast radius beyond slice 01). | DISCUSS pins "no trait change" in System Constraints; DESIGN must enforce the result cap AFTER the store query (in the handler) for slice 01, NOT push the cap into the store. A future slice MAY push it down. |
| Cap reason text leaks the raw window values. | Low | High (redaction regression). | Redaction tests mirror the existing `the_bounds_error_never_echoes_the_raw_value` test in each crate (`crates/query-api/src/lib.rs:303`, `crates/log-query-api/src/lib.rs:244`, `crates/trace-query-api/src/lib.rs:291`). The new cap-test asserts the body contains neither the requested `start`, `end`, nor any forwarded header value. |
| ADR-0050 (or the refinement) is written but the cap policy drifts between the three crates because the time-range types differ. | Low | Medium (correctness drift). | Slice 01 picks ONE window cap and ONE result cap value applied identically across the three crates; DESIGN decides whether to differ them later. The deferred `query-http-common` extraction (M-5 / ADR-0048 Decision 5) is the eventual home for a shared cap-check. |
| `query-api`'s matrix translation already happens AFTER the store query (`matrix::to_matrix(rows)`), so a result-size cap on the matrix entries is conceptually different from a cap on store rows. | Medium | Low (slice 01 picks one of the two and is explicit). | DISCUSS recommends DESIGN cap on the FINAL serialised count (matrix entries for `query-api`, records for logs, spans for traces) so the user-observable refusal matches what the user would have seen in the response body. DESIGN owns the exact line. |

## Carpaccio taste-tests (three independent demonstrations)

Three things slice 01 must prove for EACH of the three crates,
giving nine independent demonstrations total:

1. **Window over the cap refuses BEFORE the store**. A request with
   `end - start > MAX_WINDOW_SECONDS` to one of the three endpoints
   returns 400 with `{status:"error", error:"<names the cap>"}`. A
   `LyingStore` (one whose `query` always errors) is configured;
   the test asserts the response is the cap 400, NOT the
   `PersistenceFailed` 500 the lying store would produce if `query`
   had been called. This is the proof that the cap fires BEFORE the
   store.
2. **Result over the cap refuses AT serialisation**. A request whose
   store result would exceed `MAX_RESULT_ROWS` (seeded into a real
   `FileBackedMetricStore` / `FileBackedLogStore` / `FileBackedTraceStore`,
   or stubbed in the same shape as the existing store tests)
   returns 400 with the named envelope, NOT a truncated 200, NOT a
   silent empty.
3. **Redaction on cap refusal**. The 400 body contains no raw
   `start`, no raw `end`, no raw query text (`query-api`), no raw
   pattern (`query-api`), no raw `service` (`trace-query-api`), and
   no forwarded Authorization / SECRET / Bearer values. Mirrors the
   shape of the existing redaction tests at
   `crates/query-api/src/lib.rs:303`,
   `crates/log-query-api/src/lib.rs:244`,
   `crates/trace-query-api/src/lib.rs:291`.

Each taste-test is one acceptance scenario per crate; the slice is
"done" when all nine pass AND the per-crate mutation gate is 100
percent kill on the changed files for each of the three crates
(ADR-0005 Gate 5; CLAUDE.md).

## Honest contradiction check

The residuality analysis's framing of this gap was checked against
the three handlers. The framing is consistent:

- The three `parse_time_range` functions
  (`crates/query-api/src/lib.rs:201`,
  `crates/log-query-api/src/lib.rs:139`,
  `crates/trace-query-api/src/lib.rs:175`) reject non-numeric and
  inverted bounds and saturate seconds-to-nanos, but impose NO upper
  bound on `end - start`. A request with `start=0` and
  `end=31_536_000` (one calendar year) parses cleanly and reaches
  `store.query(...)`.
- The three handlers serialise WHATEVER the store returns; there is
  no upper bound on the number of rows / records / spans before
  serialisation. A store with a million rows in the requested window
  produces a million-row response.
- The error envelope shape is uniform across all three crates
  (`{status:"error", error:"<reason>"}`), so the cap 400 has a
  precedent envelope.
- The redaction posture is also uniform but slightly stricter on
  `trace-query-api` (no "SECRET", no "Bearer", no raw `service`); the
  cap 400 must honour the stricter posture there.

No contradiction surfaced that DISCUSS could not resolve.

## Changelog

- 2026-05-27: feature `honest-read-caps-v0` DISCUSS wave artefacts
  written by Luna. Four items flagged to DESIGN; one walking-skeleton
  slice declared (`slice-01-honest-read-caps-walking-skeleton.md`)
  covering five user stories across the three existing read API
  endpoints.
