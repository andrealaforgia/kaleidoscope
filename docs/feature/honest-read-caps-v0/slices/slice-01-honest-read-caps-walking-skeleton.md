# Slice 01: honest read caps walking skeleton on all three read APIs

British English. No em dashes. No emoji.

## Origin

This slice is the walking skeleton of M-2 in
`docs/product/architecture/residuality-analysis.md` (commit 50e20b5)
and item 2 in `docs/residuality-followups-roadmap.md` (commit
820176d). M-1 (`earned-trust-fsync-probe-v0`) just closed; M-2
lands one feature wave later, per the roadmap order.

The current handlers parse `start` / `end` and reject non-numeric
or inverted bounds (`crates/query-api/src/lib.rs:201`,
`crates/log-query-api/src/lib.rs:139`,
`crates/trace-query-api/src/lib.rs:175`), but impose NO upper bound
on `end - start`, and serialise WHATEVER the store returns
regardless of size. The residuality analysis flagged this as the
S13 row of the incidence matrix; all three columns (`QM`, `QL`,
`QT`) read `D no upper bound on window`. This slice closes that
gap for ALL THREE read APIs in one wave because the cap pattern is
the same shape even though each crate keeps its own `TimeRange`
type.

## Slice goal

Two compile-time caps applied to all three read APIs at the same
parse-and-validate seam the existing inverted-bounds 400 already
lives at:

- **WINDOW CAP** (`MAX_WINDOW_SECONDS`): a request with
  `end - start > MAX_WINDOW_SECONDS` is refused with 400
  BEFORE the store is touched, using the existing
  `{status:"error", error:"<reason>"}` envelope.
- **RESULT-SIZE CAP** (`MAX_RESULT_ROWS`): a response whose
  store result exceeds `MAX_RESULT_ROWS` is refused with 400
  BEFORE serialisation, using the same envelope. NOT
  truncated, NOT `X-Truncated`-headed, NOT silently empty.

The error text never echoes the raw `start`, the raw `end`, the
raw query (in `query-api`), the raw pattern (`query-api` regex),
the raw `service` (`trace-query-api`), or any forwarded
`Authorization` / "SECRET" / "Bearer" value. Storage trait
surfaces (`pulse::MetricStore`, `lumen::LogStore`,
`ray::TraceStore`) are unchanged.

## Walking-skeleton entry point

The THREE existing HTTP endpoints, exercised via the tower
`oneshot` acceptance-test pattern each crate already uses:

- `GET /api/v1/query_range?query=...&start=...&end=...` on the
  `query-api` binary (Prometheus matrix endpoint;
  `crates/query-api/src/lib.rs:66`).
- `GET /api/v1/logs?start=...&end=...` on the `log-query-api`
  binary (bare JSON array; `crates/log-query-api/src/lib.rs:63`).
- `GET /api/v1/traces?service=...&start=...&end=...` on the
  `trace-query-api` binary (bare JSON array, service required;
  `crates/trace-query-api/src/lib.rs:64`).

No new HTTP path, no new query parameter, no new event, no new
metric. The observable output is the 400 body
`{status:"error", error:"<names the breached cap>"}` and the
absence of a store call (for the window cap) or a truncated 200
(for the result cap).

## Stories in this slice

All five stories land in slice 01 atomically (see `discuss/story-map.md`
"Priority Rationale" for why):

- **US-01** (P1): query-api window cap.
- **US-02** (P1, atomic): log-query-api window cap.
- **US-03** (P1, atomic): trace-query-api window cap, with the
  existing service-required 400 firing first.
- **US-04** (P2, atomic): result-size cap on all three crates,
  refuse rather than truncate.
- **US-05** (P2, atomic): redaction on the cap reasons, no raw
  window / query / pattern / service / forwarded-header values in
  any cap 400 body.

All five stories live in `discuss/user-stories.md` with full LeanUX
shape, three domain examples each, and 4-5 BDD scenarios each.

## Learning hypothesis

We believe that two compile-time caps applied at the existing
parse-then-check seam in each of the three handlers, returning the
existing `{status:"error", error:"<reason>"}` envelope BEFORE the
store is touched (window cap) or BEFORE serialisation (result
cap), are enough to close the S13 self-DoS surface for the three
read APIs without:

- changing any storage trait,
- adding any new event / metric / dashboard,
- renegotiating the read contract with Prism (which already
  handles the same envelope shape for matcher errors per ADR-0042
  lines 220-229), or
- requiring the `query-http-common` extraction the residuality
  analysis (and ADR-0048 Decision 5) defers to M-5.

We will know we are right when:

- 100 percent of over-window requests in the acceptance suite
  refuse with the named 400 AND the store's `query` is NOT called
  on the rejected path (asserted via a LyingStore double in each
  crate).
- 100 percent of over-result requests refuse with the named 400
  AND no truncated 200, no `X-Truncated`, no silent empty.
- 100 percent of within-cap requests succeed with the existing
  envelopes (no false positives).
- 100 percent of the new cap 400 reasons pass the per-crate
  redaction tests.
- 100 percent of mutants in the changed files are killed by the
  per-crate mutation gates on `query-api`, `log-query-api`,
  `trace-query-api`.
- 0 changes to `pulse::MetricStore`, `lumen::LogStore`,
  `ray::TraceStore` trait signatures.

We will know we are wrong if:

- The chosen window cap (FLAG 1 in `wave-decisions.md`) is too
  tight for legitimate dashboards in the field (false positives
  Prism users notice). Escalation: re-pick the value in a
  successor slice, or move to env-driven configurability (slice
  02, declared OUT).
- The chosen result cap (FLAG 2) is too tight for legitimate
  exports. Escalation: same.
- DESIGN concludes the right shape on result cap breach is
  TRUNCATE rather than REFUSE (FLAG 3). Re-frame the relevant
  scenarios; the DISCUSS-time LIKELY recommendation was REFUSE
  with 400.

## Carpaccio taste-tests (nine independent demonstrations)

Three demonstrations for EACH of the three crates, giving nine
total. The slice is "done" when all nine pass AND the per-crate
mutation gate is 100 percent kill on the changed files for each of
the three crates (ADR-0005 Gate 5; CLAUDE.md).

1. **Window over the cap refuses BEFORE the store** (one scenario
   per crate). A LyingStore is wired into the test; a request with
   `end - start > MAX_WINDOW_SECONDS` returns the cap 400, and the
   LyingStore's `query` method is NEVER called (asserted; if it
   had been called, the response would be the 500 from
   `PersistenceFailed`, not the 400 cap arm).

2. **Result over the cap refuses AT serialisation** (one scenario
   per crate). A real or stubbed store is seeded with
   `MAX_RESULT_ROWS + 1` rows / records / spans inside an in-cap
   window; the response is the cap 400, NOT a truncated 200, NOT
   an `X-Truncated`-headed 200, NOT a silent empty.

3. **Redaction on cap refusal** (one scenario per crate). The cap
   400 body contains no raw `start`, no raw `end`, no raw query
   text (`query-api`), no raw pattern (`query-api`), no raw
   `service` (`trace-query-api`), no "SECRET", no "Bearer", no
   forwarded `Authorization` value. The `trace-query-api` posture
   is stricter (also no "SECRET", no "Bearer" anywhere in the
   body).

## Flagged to DESIGN

Four items are FLAGGED to DESIGN, NOT decided by DISCUSS (see
`discuss/wave-decisions.md` for the rationale on each):

1. **EXACT window cap value**. Candidates: 6h (21_600), 24h
   (86_400, the residuality analysis's named default), 7d
   (604_800). DISCUSS recommends ONE value across the three crates
   for slice 01.
2. **EXACT result-size cap value**. Candidates: 10_000, 100_000
   (the residuality analysis's named order of magnitude),
   1_000_000. DISCUSS recommends ONE value across the three crates.
3. **REFUSE vs TRUNCATE on result-cap breach**. DISCUSS's LIKELY
   recommendation is REFUSE with 400; DESIGN owns the alternative.
4. **NEW ADR-0050 vs amend ADR-0042 / 0047 / 0048 individually**.
   DISCUSS's LIKELY recommendation is one new ADR-0050 documenting
   the cap policy as a cross-cutting read-API contract refinement,
   cross-referenced from the three contracts.

## Out of scope (deferred and DECLARED)

- **Runtime-tuned caps**. Caps are compile-time constants for
  slice 01. Env-driven configurability (e.g.
  `KALEIDOSCOPE_QUERY_MAX_WINDOW_SECONDS`,
  `KALEIDOSCOPE_LOG_QUERY_MAX_WINDOW_SECONDS`,
  `KALEIDOSCOPE_TRACE_QUERY_MAX_WINDOW_SECONDS`, `_MAX_RESULTS`)
  is deferred to slice 02 / a successor feature.
- **Telemetry on refusals beyond the existing envelope**. No
  counter, no dashboard, no new structured event. A refusal IS
  the signal.
- **Changes to the tower `oneshot` test pattern**. Acceptance
  tests for the new 400 arms mirror the shape of the existing
  matcher-400 and inverted-bounds-400 tests.
- **Renegotiation with Prism**. Prism receives an honest 400 with
  the same envelope shape its `isPromError` already handles. No
  downgraded-to-success trick, no special header, no special
  status code.
- **`query-http-common` extraction**. M-5 in the residuality
  analysis (ADR-0048 Decision 5) explicitly defers the
  rule-of-three refactor. Slice 01 duplicates the cap check
  three times because the three `TimeRange` types differ.
- **Caps on parameters other than window-span and result-size**.
  No cap on the regex matcher complexity (ReDoS residue already
  secured by ADR-0046). No cap on `service` length. No cap on
  matcher count.

## Effort

Estimated 1 day total for slice 01. The residuality analysis
estimated "~30 LOC per crate"; with three crates, two caps each,
plus the redaction tests, the change is small. The breakdown:

- US-01 (query-api window cap): roughly 0.25 days.
- US-02 (log-query-api window cap): roughly 0.25 days.
- US-03 (trace-query-api window cap): roughly 0.25 days.
- US-04 (result-size cap, all three): roughly 0.5 days (the
  enforcement is at three different post-store points).
- US-05 (redaction on cap reasons, all three): roughly 0.25 days
  (mirror the existing redaction-test shape).

All five stories ship atomically because the per-crate mutation
gate evaluates the whole crate after the change.
