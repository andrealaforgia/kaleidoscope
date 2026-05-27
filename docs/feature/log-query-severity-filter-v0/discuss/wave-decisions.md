# Wave Decisions: log-query-severity-filter-v0

Decisions and deferrals captured during the DISCUSS wave. The DESIGN wave
inherits this file as input to ADR drafting (FLAG 4 below).

## Wave context

- **Feature**: `log-query-severity-filter-v0`
- **Type**: backend, thin slice on `crates/log-query-api`.
- **JTBD**: not run (per task brief; the feature is a thin slice on an
  existing read API with a single named user outcome).
- **DIVERGE artefacts**: not present (no
  `docs/feature/log-query-severity-filter-v0/diverge/` directory). The
  feature was scoped directly into DISCUSS by the brief, which is honest
  for a one-day slice on top of an already-validated API. Recorded here as
  a noted absence, NOT a blocker; the slice's user job is named in the
  brief and traceable to ADR-0047's operator persona.
- **Research depth**: lightweight (per task brief).
- **DISCUSS author**: `nw-product-owner` (Luna).
- **DISCUSS date**: 2026-05-27.

## Read-first checklist (artefacts grounded in source code)

The DISCUSS wave was grounded in the following files, all read in full
before any artefact was written:

- [x] `crates/lumen/src/lib.rs` — confirmed `Predicate` and
      `SeverityNumber` are part of lumen's public surface (re-exported via
      `pub use`).
- [x] `crates/lumen/src/store.rs` — confirmed the `LogStore::query_with`
      seam (line 89) is the existing predicate-carrying query method;
      `InMemoryLogStore::query_with` honours the predicate via
      `predicate.matches(r)` (lines 159-180). No new trait method needed.
- [x] `crates/lumen/src/record.rs` — confirmed the OTel SeverityNumber
      ladder (lines 32-39): `TRACE=1`, `DEBUG=5`, `INFO=9`, `WARN=13`,
      `ERROR=17`, `FATAL=21`. The `severity_number` field on `LogRecord`
      is the source of truth for the filter; the `severity_text` field is
      out of scope.
- [x] `crates/lumen/src/predicate.rs` — confirmed
      `Predicate::min_severity(SeverityNumber)` already exists (line 46)
      with the correct `>=` semantics (line 61: `if record.severity_number
      < floor { return false; }`). NO lumen change needed.
- [x] `crates/log-query-api/src/lib.rs` — confirmed the handler shape, the
      `parse_time_range_seconds` parse pattern, the `MAX_WINDOW_SECONDS =
      86_400` constant (line 70), the `MAX_RESULT_ROWS = 100_000` constant
      (line 77), the `success_response` and `error_response` helpers
      (lines 224, 231), and the existing `state.store.query(&tenant,
      range)` call (line 147) that will branch on the presence of the new
      parameter to use `query_with` instead.
- [x] `crates/log-query-api/tests/slice_01_logs_read.rs` — confirmed the
      test-fixture conventions: `open_durable_store`, `tenant`, `seed`,
      `record`, `record_at_nanos`, `rich_record`, `logs_request`,
      `records_array`, `record_bodies`, `is_error_envelope`. The new
      acceptance file will reuse these helpers; the test posture is
      `tokio::test` + `oneshot` against the router.
- [x] `crates/log-query-api/tests/slice_02_caps.rs` — confirmed the
      `BulkLogStore` test-double pattern (line 86), used to exercise the
      result-cap arm without seeding 100_000+ records. The "filter BEFORE
      cap" scenario in user-stories.md will reuse this pattern with a
      severity-mix-driven count.
- [x] `docs/product/architecture/adr-0047-lumen-log-query-api-contract-and-crate-layout.md` —
      confirmed: bare JSON array success shape (Decision 1), the
      `{status:"error", error:"..."}` envelope (Decision 1), the redaction
      posture (Decision 1: the reason never echoes a forwarded header or
      credential value), the `query_with(predicate)` trait method exists
      but is NOT used in slice 01 (Decision 5). Slice 01 of THIS feature
      uses `query_with` for the first time on the HTTP boundary; that is
      consistent with ADR-0047 Decision 5 (which deferred its USE, not its
      EXISTENCE) and is the API-contract growth FLAG 4 records.
- [x] `docs/product/architecture/adr-0050-earned-trust-read-side-caps.md` —
      confirmed the cap interaction model: window-cap fires BEFORE the
      store; result-cap fires AFTER the store and BEFORE serialisation
      (Decision 4). FLAG 3 is about WHERE the new severity filter sits
      relative to the result cap. The likely answer (filter BEFORE cap)
      is consistent with ADR-0050 Decision 4's posture that the cap
      measures "what the user observes" (post-filter records, not the
      upstream raw row count).

## Decisions taken in DISCUSS (NOT design choices)

These are scope and process decisions; the design wave does NOT re-open
these.

1. **Scope: SLICE 01 THIN.** A single optional query-string parameter on
   `GET /api/v1/logs`. Six OTel severity names. `>=` on the numeric
   ladder. No `severity_text`, no ranges, no body regex, no attribute
   filters, no resource-attribute exposure on the HTTP boundary, no
   aliases, no env-driven default. All deferrals are named in
   `user-stories.md` § "OUT of scope" and `story-map.md` § "Out-of-scope".

2. **No lumen trait change.** The existing `query_with(&tenant, range,
   &Predicate)` seam and the existing `Predicate::min_severity` builder
   carry the slice. `lumen::LogStore` trait signatures stay
   byte-identical (Gate 2 `cargo public-api`).

3. **No new module.** All new code lives in
   `crates/log-query-api/src/lib.rs` alongside the existing handler.

4. **No `query-http-common` extraction.** ADR-0048 Decision 5 / ADR-0050
   § 5 deferral is HONOURED. The new severity-name parse function is
   `log-query-api`-local. A successor slice may extract it once a third
   pillar grows the same shape.

5. **Error envelope reuse.** No new envelope shape, no new status code.
   The unknown-severity 400 reuses `{"status":"error","error":"unknown
   severity"}` (subject to FLAG 1 affecting the spelling of the wire
   parameter name but NOT the envelope).

6. **Redaction posture inherited.** The new 400 arm honours ADR-0047
   Decision 1 (no forwarded credential, no raw header value) and the
   ADR-0050 Decision 7 symmetric extension (no raw parameter value
   echoed).

7. **No new metric, no new tracing event, no new dashboard.** Consistent
   with ADR-0050 Decision 8; the contract IS the signal.

8. **Existing acceptance suites must stay green unchanged.** Both
   `tests/slice_01_logs_read.rs` and `tests/slice_02_caps.rs` are NOT
   edited by this slice. The new acceptance suite is a new file
   (`tests/slice_01_severity_filter.rs`) that adds to, never replaces,
   the existing coverage.

## Flags to DESIGN (do NOT decide in DISCUSS)

These four decisions belong to the DESIGN wave (`nw-solution-architect`,
Morgan). DISCUSS records recommendations; DESIGN pins.

| # | Flag | Recommendation | Notes |
|---|---|---|---|
| 1 | **Wire parameter name** | `min_severity` | Alternatives `level`, `severity_min`. The handler-side mapping is identical; only the URL spelling differs. `min_severity` is the most explicit and matches the lumen `Predicate::min_severity` builder name. |
| 2 | **Case-sensitivity** | Case-insensitive on the six OTel names; NO aliases | `WARN`, `warn`, `Warn` all map to `SeverityNumber::WARN`. `WARNING`, `err`, `critical` are 400. The case-insensitivity matches operator muscle memory; the alias rejection matches "honest cap" posture (a typo is a typo, the platform refuses out loud). |
| 3 | **Filter BEFORE or AFTER the result cap** | BEFORE | The cap counts post-filter records, so a high-volume INFO storm does not eat the cap budget; a strict `min_severity=ERROR` delivers all matching records up to the cap. Consistent with ADR-0050 Decision 4 ("the check measures what the user observes ... bare-array records ... not the upstream raw row count"). The "filter applies BEFORE the result cap" UAT scenario in `user-stories.md` encodes this; DESIGN inverts the scenario if it inverts the flag. |
| 4 | **ADR-0052 vs refinement of ADR-0047** | Small ADR-0052 | ADR-0047 is immutable (repo-wide rule: ADRs are superseded, never edited). The read-side log API contract is growing a new optional parameter; that is a contract change worth its own ADR, with a back-reference to ADR-0047. The likely title: "ADR-0052 — log-query-api `min_severity` filter parameter". DESIGN may decide a per-wave `wave-decisions.md` reference suffices, but the recommendation is a fresh ADR for searchability. |

Each flag has a recommendation, a reason, and a name; DESIGN pins by
reading this section, weighing the recommendation, and recording the
choice in its own `design/wave-decisions.md` or in the ADR it authors.

## Risks (surfaced, not managed)

- **R-1 (LOW)**: DESIGN inverts FLAG 3 (cap BEFORE filter). The "filter
  applies BEFORE the result cap" UAT scenario in `user-stories.md` is
  rewritten; impact is one scenario's expectations. Mitigation: the
  scenario is encoded as one of five, not woven through all of them.
- **R-2 (LOW)**: DESIGN decides FLAG 2 the other way (strict
  case-sensitivity). Impact: the case-insensitivity scenarios in
  `user-stories.md` (e.g. accepting `warn`) become 400 scenarios.
  Mitigation: trivial scenario rewrite; no acceptance test deletion.
- **R-3 (LOW)**: A future consumer (a prism log panel, a Grafana
  datasource) wants Loki-shaped severity filtering. Mitigation:
  ADR-0047's posture explicitly accepts Loki-shaping as a later additive
  slice behind the same route; the OTel-shaped filter does not preclude
  it.
- **R-4 (LOW)**: The walking-skeleton acceptance fixture's "5x payload
  reduction" KPI-1 target is too tight or too loose. Mitigation: the
  fixture is small and deterministic (60s window, 80/20 INFO/WARN+ERROR
  mix); DESIGN may calibrate the multiplier without changing the
  feature's intent.
- **R-5 (LOW)**: DIVERGE wave was not run. Mitigation: the feature is a
  thin slice on a single API with one named user outcome; the JTBD is
  pinned by ADR-0047's operator persona and the brief. A retroactive
  DIVERGE is unnecessary at this scope but should be considered if
  successor slices grow ranges, aliases, or text-field filtering.

## Definition of Ready (9-item gate)

| # | DoR item | Status | Evidence |
|---|---|---|---|
| 1 | Problem statement clear, domain language | PASS | `user-stories.md` US-01 § Problem names Sara Mendez, the 800-record breakdown, the `jq` workaround. Domain language throughout (`SeverityNumber`, `min_severity`, `query_with`). |
| 2 | User/persona with specific characteristics | PASS | `user-stories.md` US-01 § Who names Sara Mendez (SRE, mid-incident, terminal + curl + jq, triage urgency) and Marcus Webb (platform engineer, automation, throughput motive). |
| 3 | 3+ domain examples with real data | PASS | `user-stories.md` US-01 § Domain Examples carries four examples (happy path, default unchanged, boundary inclusive, error path), each with real tenant id (`acme-prod`), real timestamps, real severity ladder values, and concrete URL fragments. |
| 4 | UAT in Given/When/Then (3-7 scenarios) | PASS | Six Gherkin scenarios in `user-stories.md` § UAT Scenarios (within the 3-7 range), each with concrete data tables and observable outcomes. |
| 5 | AC derived from UAT | PASS | `user-stories.md` § Acceptance Criteria carries 10 checkboxes, each traceable to one or more scenarios. |
| 6 | Right-sized (1-3 days, 3-7 scenarios) | PASS | 5 stories, 6 scenarios, 1 day estimated. Carpaccio gate explicitly passed in `story-map.md` § Scope Assessment. |
| 7 | Technical notes identify constraints | PASS | `user-stories.md` § Technical Notes names the existing seams (`query_with`, `Predicate::min_severity`, `SeverityNumber` constants), the parsing location, the order-of-checks, and the mutation targets. |
| 8 | Dependencies resolved or tracked | PASS | All dependencies are present in the prior tag: ADR-0047, ADR-0050, lumen's `query_with` and `Predicate::min_severity`. Flags to DESIGN are tracked, not blockers. |
| 9 | Outcome KPIs defined with measurable targets | PASS | `outcome-kpis.md` carries three KPIs, each with Who/Does what/By how much/Baseline/Measured by/Type. KPI-1 has a 5x quantitative target on a named fixture; KPI-2 and KPI-3 are 100% gates on existing suites and new redaction assertions. |

**DoR Status: PASSED.** Ready for DESIGN handoff.

## Handoff to DESIGN

The DESIGN wave (`nw-solution-architect`, Morgan) inherits:

1. The four artefacts in `docs/feature/log-query-severity-filter-v0/discuss/`:
   `user-stories.md`, `story-map.md`, `outcome-kpis.md`, `wave-decisions.md`.
2. The slice file `docs/feature/log-query-severity-filter-v0/slices/slice-01-severity-filter.md`
   carrying the walking-skeleton scenario in DESIGN-ready form.
3. The four flags above, each with a recommendation, a reason, and a name.
4. The read-first checklist above (every grounding source named with a
   file path and a line number where applicable).

DESIGN authors (or refines) ADR-0052 per FLAG 4 and produces the DESIGN
brief to the crafter.
