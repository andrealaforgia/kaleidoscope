# Story Map: log-query-severity-filter-v0

## User: Sara Mendez (on-call SRE, tenant `acme-prod`); Marcus Webb (platform engineer, automated alerting client)

## Goal: Narrow the log read to a single severity floor (e.g. WARN+) at the HTTP boundary, so that mid-incident triage and downstream automation receive only the records that matter, in the same shape as today's response.

## Backbone

| 1. Issue read request | 2. Server validates input | 3. Server reads + filters | 4. Server enforces cap | 5. Client receives narrowed JSON |
|---|---|---|---|---|
| US-01 Sara appends `<min_severity_param>=WARN` to the existing `/api/v1/logs?start=&end=` URL | US-02 default behaviour (parameter absent) preserved; US-05 unknown severity name -> redacted 400 with existing envelope | US-01 records below the floor are dropped; US-03 boundary inclusive (`==` floor passes) | US-04 cap interaction: filter applied BEFORE cap (LIKELY; flagged to DESIGN) | US-01 bare JSON array of just the matching records |
| Marcus's automation appends `<min_severity_param>=ERROR` to its hourly query | parameter parsing: six OTel names; case-sensitivity flagged to DESIGN | the existing `query_with(&tenant, range, predicate)` seam carries the filter | result-cap stays at `MAX_RESULT_ROWS = 100_000` (ADR-0050), measured post-filter | shape is identical to today; only the row set is narrower |

---

### Walking Skeleton

The thinnest end-to-end slice that demonstrates the value:

> Seed tenant `acme-prod` with a mix of INFO, WARN, and ERROR records inside
> `[1716200000s, 1716200060s)`. Issue
> `GET /api/v1/logs?start=1716200000&end=1716200060&<min_severity_param>=WARN`.
> Receive HTTP 200 with a bare JSON array containing ONLY the WARN and ERROR
> records, in ascending `observed_time_unix_nano` order. The INFO records are
> excluded.

This walks all five backbone columns at once: an operator issues the request
(col 1), the server validates the severity name (col 2), the lumen
`Predicate::min_severity(WARN)` drops INFO before the cap (col 3), the
post-filter result is under the cap (col 4), and the response is the
narrowed bare array (col 5). US-01 IS the walking skeleton.

### Release 1 (slice 01, THIN): walking skeleton + boundary + default + error path

All five stories ship together because the slice is small enough (~1 day) and
no story is independently demoable without the others (US-02 default is the
backward-compat contract that US-01 must not break; US-03 boundary is the
mutation-test partner of US-01; US-04 is a flag-confirmation scenario; US-05
is the only non-200 arm the new parameter introduces).

| Story | Backbone column | Outcome KPI link | Notes |
|---|---|---|---|
| US-01 walking skeleton (min_severity=WARN drops below-floor) | 1, 3, 5 | KPI-1 (payload-size reduction) | The demoable behaviour |
| US-02 default unchanged (no parameter -> today's behaviour) | 1, 5 | KPI-2 (backward compatibility, zero broken clients) | Backward-compat contract |
| US-03 boundary inclusive (`==` floor passes) | 3 | KPI-1 (correctness) | Mutation kill on `>=` boundary |
| US-04 cap interaction (filter BEFORE cap; flag confirmed) | 3, 4 | KPI-1 (cap-budget honesty) | Encodes flagged recommendation |
| US-05 unknown severity -> redacted 400 | 2 | KPI-3 (redaction posture preserved) | Error-envelope reuse |

## Scope Assessment: PASS — 5 stories, 1 bounded context (`log-query-api`), estimated 1 day

Carpaccio gate signals:

- 5 user stories (well under the 10-story threshold).
- 1 bounded context (`crates/log-query-api`); the `lumen` crate is touched at
  the consumer level only (calling existing `query_with` and reading existing
  `Predicate::min_severity`); no lumen surface change.
- Walking skeleton requires 1 integration point (HTTP route -> existing
  `LogStore::query_with` seam).
- Estimated effort: 1 day end-to-end (parse one parameter, branch one
  handler arm, add one mapping table, write five acceptance scenarios).
- Single user outcome: a narrowed read on `GET /api/v1/logs`.

All five signals well below the oversized threshold. NO split needed.

## Priority Rationale

The five stories ship in a single slice because they are mutually dependent
proofs of one behaviour, not independent deliverables:

1. **US-01 is the walking skeleton**: it carries the value (narrowed payload)
   and is the only story that demonstrates the feature in a demo. It MUST
   ship.
2. **US-02 is the backward-compatibility contract** that US-01 must not
   break. Without US-02 the slice could regress Marcus's existing automation.
   It MUST ship alongside US-01.
3. **US-03 is the mutation-test partner of US-01**: without the boundary
   scenario, a `>=` -> `>` mutant on the severity floor survives Gate 5
   (ADR-0005). It MUST ship alongside US-01.
4. **US-04 confirms FLAG 3 in code**: the scenario encodes the LIKELY
   recommendation (filter BEFORE cap). If DESIGN inverts the flag, US-04 is
   rewritten before DELIVER; either way the cap interaction is pinned in
   acceptance, not folklore. Ships in the slice.
5. **US-05 is the only new error arm**: a 400 with the existing envelope and
   the existing redaction posture. Without US-05 the slice ships an
   un-tested error path. MUST ship alongside US-01.

Risk-derisking order WITHIN the slice (drives the order of acceptance test
enablement in DELIVER, per the one-at-a-time outer loop already used in
`tests/slice_01_logs_read.rs`):

1. US-01 (skeleton, the riskiest assumption: that the existing
   `query_with` seam suffices).
2. US-02 (backward-compat, derisks "did we accidentally change the
   no-parameter response?").
3. US-03 (boundary, derisks the `>=` semantics).
4. US-05 (error envelope, derisks the redaction posture on the new 400 arm).
5. US-04 (cap interaction, derisks FLAG 3 once DESIGN confirms or inverts).

## Out-of-scope (DECLARED; carried into next slices)

- `severity_text` filtering (custom non-OTel labels).
- Severity RANGES (e.g. WARN+ERROR but NOT FATAL).
- Body regex / substring filtering.
- Record-attribute filtering.
- Resource-attribute filtering on the HTTP boundary (lumen already supports
  `service`; HTTP exposure is a separate slice).
- Aliases (`WARNING` -> `WARN`, etc.).
- Env-driven default severity floor.
- The `query-http-common` extraction (ADR-0048 Decision 5; ADR-0050 §5).
- Per-pillar cap tuning (ADR-0050 forward-looking scope).
