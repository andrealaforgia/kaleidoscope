# Outcome KPIs: honest-read-caps-v0

British English. No em dashes. No emoji.

## Feature objective

The three read APIs (`query-api`, `log-query-api`,
`trace-query-api`) refuse out loud with a named 400 when a request
asks for a window longer than the configured cap, or would yield
more rows / records / spans than the configured cap, instead of
saturating the listener or driving the process to OOM. The S13
self-DoS surface flagged by the residuality analysis is closed for
all three pillars in one slice. The refusal envelope is the same
shape the existing matcher and inverted-bounds 400s already use;
Prism's `isPromError` already handles it.

## Outcome KPIs

| # | Who | Does What | By How Much | Baseline | Measured By | Type |
|---|---|---|---|---|---|---|
| 1 | An operator (Maya Kowalski) or on-call SRE (Idris Mbeki) sending requests to any of the three read APIs at tenant "acme-prod" | sees the API refuse with `{status:"error", error:"window exceeds <N> seconds"}` (or named equivalent) when `end - start > MAX_WINDOW_SECONDS`, BEFORE the store is touched | 100 percent of over-window requests in the acceptance suite (US-01 Scenario 2, US-02 Scenario 2, US-03 Scenario 2) refuse with the named 400 and the store's `query` is NOT called | 0 percent today: every well-formed window is passed straight to the store (`crates/query-api/src/lib.rs:180`, `crates/log-query-api/src/lib.rs:123`, `crates/trace-query-api/src/lib.rs:145`) | Acceptance-test pass / fail in the slice-01 suite, plus a "store was queried zero times" assertion via a lying store double | Leading (outcome) |
| 2 | Same persona | sees the API refuse with `{status:"error", error:"result exceeds <M> rows"}` (or named equivalent) when the store result would exceed `MAX_RESULT_ROWS`, BEFORE serialisation | 100 percent of over-result requests in the acceptance suite (US-04 Scenario 2 across the three crates) refuse with the named 400 and NO truncated 200, NO `X-Truncated` header, NO silent empty | 0 percent today: the handler serialises WHATEVER the store returns (`success_response(result)`, `success_response(records)`, `success_response(spans)`) | Acceptance-test pass / fail; the "no truncated 200, no X-Truncated, no silent empty" assertion in US-04 Scenario 2 | Leading (outcome) |
| 3 | Same persona | sees within-cap requests served normally with no false positives (the cap is invisible on well-sized queries) | 100 percent of within-cap requests in the acceptance suite (US-01 Scenario 1, US-02 Scenario 1, US-03 Scenario 1, US-04 Scenario 1) succeed with the existing envelopes | 100 percent today (all well-sized requests succeed; the cap does not exist) | Acceptance-test pass / fail; guardrail KPI | Guardrail |
| 4 | A security reviewer reading the new cap 400 bodies | sees no raw `start`, no raw `end`, no raw query text, no raw pattern (`query-api`), no raw `service` (`trace-query-api`), no "SECRET", no "Bearer", and no forwarded `Authorization` value in any cap 400 body | 100 percent of cap 400 reasons across the three crates pass the redaction tests in US-05 | The existing bounds-error and service-error 400s pass redaction (precedent at `crates/query-api/src/lib.rs:303`, `crates/log-query-api/src/lib.rs:244`, `crates/trace-query-api/src/lib.rs:291`, `crates/trace-query-api/src/lib.rs:334`); the new cap reasons have NO redaction tests because the new code does not yet exist | The slice-01 redaction tests in each crate's `#[cfg(test)] mod tests` block | Leading (outcome) |
| 5 | A Kaleidoscope developer maintaining the changed crates | sees the per-crate mutation gate stay at 100 percent kill on the changed files for each of the three crates | 100 percent (ADR-0005 Gate 5; CLAUDE.md) | 100 percent today on the unchanged code; the new cap-check and redaction-test code does not yet exist | `cargo mutants` per-crate output on changed files in `query-api`, `log-query-api`, `trace-query-api` | Leading (secondary) |
| 6 | A reviewer comparing the public-api diff against the prior tag | sees no change to `pulse::MetricStore`, `lumen::LogStore`, or `ray::TraceStore` trait signatures | 0 trait signature changes | 0 today | The `gate-2-public-api` diff (ADR-0005 Gate 2) on each of the three crates AND on `pulse`, `lumen`, `ray` | Guardrail |

## Metric hierarchy

- **North star**: KPI 1 plus KPI 2, "the cap fires and refuses, 100
  percent in the acceptance suite". The whole feature exists to
  move these from 0 to 100 across the three crates.
- **Leading indicators**: KPI 3 (no false positives on within-cap
  requests), KPI 4 (redaction holds for the new reasons), KPI 5
  (mutation kill rate stays at 100 percent on changed files).
- **Guardrail metrics**: KPI 6 (no store-trait change; the caps
  ride entirely in the handler), plus the existing 400-arm
  regression tests in each crate (the existing bounds-error 400
  must still fire on a non-numeric or inverted bound; the existing
  matcher 400 on `query-api` must still fire on a bad regex; the
  existing service-required 400 on `trace-query-api` must still
  fire on a missing or empty `service`).

## Measurement plan

| KPI | Data source | Collection method | Frequency | Owner |
|---|---|---|---|---|
| 1 | Slice-01 acceptance suite across `query-api`, `log-query-api`, `trace-query-api` | `cargo test` in CI (Gate 1 of ADR-0005) plus the explicit over-window scenarios with a LyingStore | Every push | Crafty (DELIVER) writes the tests; Bea (DOCUMENT) records closure in the per-feature narrative |
| 2 | Same suite | Same, plus the explicit over-result scenarios | Same | Same |
| 3 | Same suite | The within-cap happy-path scenarios | Same | Same |
| 4 | Each crate's redaction tests in `#[cfg(test)] mod tests` | `cargo test` per crate | Same | Same |
| 5 | `cargo mutants` per-crate workflow on the three changed crates | The per-crate Gate 5 workflow already in CI | Same | Apex (DEVOPS) maintains the workflow; Crafty keeps the kill rate at 100 percent on changed files |
| 6 | `cargo public-api` diff against the prior tag | Gate 2 in the per-crate CI on `query-api`, `log-query-api`, `trace-query-api`, `pulse`, `lumen`, `ray` | Same | Apex |

## Hypothesis

We believe that TWO compile-time caps (`MAX_WINDOW_SECONDS` and
`MAX_RESULT_ROWS`) applied at the parse-and-validate seam of each
of the three handlers (the same seam the existing inverted-bounds
400 already lives at, plus a result-count check between the store
response and the success-response serialisation), returning the
existing `{status:"error", error:"<reason>"}` envelope, will close
the S13 self-DoS surface for all three read APIs without:

- changing any storage trait,
- adding any new event, metric, or dashboard,
- renegotiating the read contract with Prism (which already handles
  the same envelope shape for matcher errors), or
- requiring the `query-http-common` extraction the residuality
  analysis defers to M-5.

We will know this is true when:

- An operator (or the acceptance suite) observes 100 percent of
  over-window requests refusing with the named 400 and zero store
  calls on the rejected path.
- 100 percent of over-result requests refuse with the named 400
  and zero truncations / `X-Truncated` headers / silent empties.
- 100 percent of within-cap requests succeed with the existing
  envelopes (no false positives).
- 100 percent of the new cap 400 reasons pass the per-crate
  redaction tests.
- 100 percent of mutants on the changed files are killed by the
  per-crate mutation gate.
- 0 changes to the storage trait signatures.

We will know this is false if:

- The chosen window cap (FLAG 1) is too tight for legitimate
  dashboards in the field, generating false positives Prism users
  notice. Escalation path: re-pick the value in a successor slice,
  or move to env-driven configurability (slice 02, declared OUT).
- The chosen result cap (FLAG 2) is too tight for legitimate
  exports. Escalation: same.
- DESIGN concludes the right shape on result cap breach is
  TRUNCATE rather than REFUSE (FLAG 3). Re-frame the relevant
  scenarios; the DISCUSS-time LIKELY recommendation was REFUSE.

## Smell-test review

| Check | Verdict | Note |
|---|---|---|
| Measurable today? | Yes | Acceptance-test outcomes, `cargo mutants` kill rate, and `cargo public-api` diffs are all already collected by the platform's existing CI surface (ADR-0005). |
| Rate not total? | Yes | All KPIs are rates over scenarios ("100 percent of over-window requests refuse"), not gross counts. |
| Outcome not output? | Yes | The KPIs describe the operator's observable behaviour change (the API refuses with a named reason), not the shipped artefact ("we shipped caps"). |
| Has baseline? | Yes | 0 percent today, with handler-file-and-line evidence cited in the baseline column of KPIs 1, 2, 4. |
| Team can influence? | Yes | The team owns the handler code, the constants, and the redaction tests; nothing external. |
| Has guardrails? | Yes | KPI 3 (no false positives on within-cap requests), KPI 6 (no trait change), plus the existing 400-arm regression tests in each crate. |

## Handoff to DEVOPS

Per the residuality follow-up roadmap, DEVOPS (Apex) for this
feature is SLIM: there is no new crate (the work lands inside the
three existing read-API crates) and no new dependency expected.
The DEVOPS surface this feature touches:

- **Data collection**: NONE new. The refusal rides on the existing
  `{status:"error", error}` envelope; no new event, no new metric,
  no new dashboard.
- **Dashboards / monitoring**: NONE new at v0/v1. The platform has
  no live observability stack of its own yet.
- **Alerting thresholds**: NONE. The 400 IS the signal; the
  operator (or the client) sees the named cap and narrows.
- **Baseline measurement**: the handler-file-and-line evidence
  cited in the baseline columns suffices; no separate baseline
  collection needed.
- **Per-crate mutation gates**: `gate-5-mutants-query-api`,
  `gate-5-mutants-log-query-api`, `gate-5-mutants-trace-query-api`
  already exist; the slice-01 change must keep all three at 100
  percent kill on the changed files. No new gate, no new workflow.

## Connection to the residuality analysis

The KPIs above map directly onto the analysis's incidence-matrix
S13 row, with read-side amplification from S04 / S14:

- Every cell that reads `D no upper bound on window` under `QM`,
  `QL`, `QT` (S13 row) today becomes `S window cap refuses at the
  handler` once slice 01 lands.
- Every cell that reads `D fan-out cost` (S14 row at QM; the
  cardinality-bomb amplification at the read side) gains a
  secondary residue: the result cap bounds the worst case.

KPI 1 plus KPI 2 is the rate at which this matrix transition
completes across the three crates; KPI 3 + KPI 4 + KPI 5 + KPI 6
are the guardrails that the transition does not cost anything
elsewhere in the matrix (no trait change, no false positive on
within-cap requests, no redaction regression, no mutation-kill-rate
regression).
