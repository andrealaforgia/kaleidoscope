# Wave Decisions: log-body-text-search-v0

Decisions and deferrals captured during the DISCUSS wave. The DESIGN wave
inherits this file as input to ADR drafting (FLAG 6 below).

## Wave context

- **Feature**: `log-body-text-search-v0`
- **Type**: backend, thin slice on `crates/log-query-api`. Carpaccio
  parallel to `log-query-severity-filter-v0`; only the origin of the
  filter changes (severity number to substring on body string).
- **JTBD**: not run (per task brief; thin slice on an existing read API
  with one named user outcome).
- **DIVERGE artefacts**: not present (no
  `docs/feature/log-body-text-search-v0/diverge/` directory). The feature
  was scoped directly into DISCUSS by the brief, which is honest for a
  one-day slice on top of an already-validated API. Recorded as a noted
  absence, NOT a blocker; the slice's user job is named in the brief and
  traceable to ADR-0047's operator persona.
- **Research depth**: lightweight (per task brief).
- **Walking skeleton**: NO new skeleton. The endpoint, the durable store,
  the tenant seam, the caps, the `min_severity` parameter, and the
  `query-http-common` shared crate are ALL already in production (slice
  01 of log-query-api, slice 02 caps, log-query-severity-filter-v0, M-5).
  This slice is brownfield additive.
- **DISCUSS author**: `nw-product-owner` (Luna).
- **DISCUSS date**: 2026-05-27.

## Read-first checklist (artefacts grounded in source code)

The DISCUSS wave was grounded in the following files, all read in full
before any artefact was written:

- [x] `crates/log-query-api/src/lib.rs` — confirmed the post-M-5 handler
      shape: `parse_time_range`, `resolve_tenant_or_refuse`,
      `error_response`, `MAX_WINDOW_SECONDS`, `MAX_RESULT_ROWS`, and the
      four `REASON_*` constants are ALL consumed via
      `query_http_common::` (lines 64, 120, 127, 137-142, 154-160, 180-185,
      188-193). Confirmed the `LogsParams` struct shape (lines 104-109)
      where the new `body_contains: Option<String>` field will land beside
      `min_severity: Option<String>`. Confirmed the existing dispatch arm
      (lines 165-172) where the predicate-bearing branch already exists
      and just needs a composed predicate.
- [x] `crates/lumen/src/store.rs` — confirmed the `LogStore::query_with`
      seam (line 89) and the `InMemoryLogStore::query_with` (lines
      159-180), which already honours an arbitrary `Predicate` via
      `predicate.matches(r)` (line 175). The tenant bucket lookup happens
      BEFORE any predicate evaluation (lines 166-172), which is the
      foundation of the cross-tenant isolation invariant pinned in US-06.
- [x] `crates/lumen/src/lib.rs` — confirmed `Predicate` is on the public
      surface via `pub use predicate::Predicate;` (line 57). Confirmed
      `LogRecord` is on the public surface (line 58). The slice's surface
      additions, if FLAG 3 lands as recommended, will be visible to
      `cargo public-api`.
- [x] `crates/lumen/src/predicate.rs` — CONFIRMED `Predicate` does NOT
      today carry a `body_contains` field (lines 25-28: only `service:
      Option<String>` and `min_severity: Option<SeverityNumber>`).
      Confirmed the `matches` method is conjunctive AND (lines 53-66:
      every set filter must pass; an unset filter is skipped). Confirmed
      `is_empty()` (lines 70-72) returns true when neither `service` nor
      `min_severity` is set; if FLAG 3 lands as recommended, the
      `is_empty()` arm must be extended to include `body_contains`.
      Confirmed `Predicate::matches` reads `record.body` only via no
      existing arm; the slice's match arm would be a straightforward
      `if let Some(target) = self.body_contains.as_deref() { if
      !record.body.contains(target) { return false; } }`.
- [x] `crates/query-http-common/src/lib.rs` — confirmed the public
      surface the slice will consume: `MAX_WINDOW_SECONDS` (line 69),
      `MAX_RESULT_ROWS` (line 75), `REASON_INVALID_TIME_RANGE` (line 84),
      `REASON_WINDOW_TOO_LARGE` (line 89), `REASON_TOO_MANY_ROWS` (line
      94), `REASON_MISSING_TENANT` (line 100), `parse_time_range` (line
      174), `resolve_tenant_or_refuse` (line 235), `error_response`
      (line 264). No new helper needed in `query-http-common` for the
      slice; `body_contains` is a single-pillar concern (the `body` field
      is OTLP-shaped and lives on `lumen::LogRecord` only, not on
      `pulse::MetricPoint` or `ray::Span`).
- [x] `crates/lumen/src/record.rs` — confirmed `LogRecord.body` is a
      plain `String` (line 54), serde-derived (line 44). The substring
      filter operates on this field via `String::contains`, no shape
      conversion needed.
- [x] `docs/feature/log-query-severity-filter-v0/discuss/user-stories.md`
      — adopted the Elevator Pitch pattern (Before / After / Decision
      enabled), the System Constraints / OUT-of-scope structure, the
      named-mutation-targets posture, and the flag-recommendation table
      shape.
- [x] `docs/feature/log-query-severity-filter-v0/discuss/wave-decisions.md`
      — adopted the read-first checklist structure, the
      decisions-taken-in-DISCUSS posture (not design choices), the
      flags-to-DESIGN table shape, the risks-surfaced-not-managed
      posture, and the DoR-evidence table format.
- [x] `docs/feature/log-query-severity-filter-v0/discuss/story-map.md`
      — adopted the Backbone / Walking Skeleton / Release / Scope
      Assessment / Priority Rationale / Out-of-scope structure.
- [x] `docs/residuality-followups-roadmap.md` — confirmed M-5
      (`query-http-common` extraction) was already deferred as a separate
      future feature and is NOT in the residuality roadmap. This slice
      validates the M-5 work post-extraction by consuming the shared
      crate as the first new arm.

## Decisions taken in DISCUSS (NOT design choices)

These are scope and process decisions; the design wave does NOT re-open
these.

1. **Scope: SLICE 01 THIN.** A single optional query-string parameter
   `body_contains=<string>` on `GET /api/v1/logs`. Substring matching
   ONLY (no regex). Case-sensitive ONLY. `LogRecord.body` field ONLY
   (no `severity_text`, no attributes, no resource attributes). One
   substring per request. Empty value is a redacted 400. Composes
   conjunctively with `min_severity`. All deferrals are named in
   `user-stories.md` § "OUT of scope" and `story-map.md`
   § "Out-of-scope".

2. **No new ADR authored by DISCUSS.** ADR drafting belongs to DESIGN
   (Morgan). DISCUSS recommends ADR-0055 in FLAG 6 below; DESIGN decides
   whether to author it (recommended YES if FLAG 3 lands as a lumen
   surface extension; recommended NO if FLAG 3 lands as handler-side
   filtering only).

3. **`query-http-common` is the SOLE provider** of the cap constants
   (`MAX_RESULT_ROWS`, `MAX_WINDOW_SECONDS`), the reason constants
   (four `REASON_*`), the error envelope helper (`error_response`), the
   tenant seam (`resolve_tenant_or_refuse`), and the bounds parser
   (`parse_time_range`). The slice MUST NOT re-declare or re-implement
   any of them inside `log-query-api`. KPI-3 in `outcome-kpis.md`
   enforces this with CI static-grep assertions and a line-count budget
   on the diff.

4. **No lumen trait change.** The existing `query_with(&tenant, range,
   &Predicate)` seam carries the slice. `lumen::LogStore` trait
   signatures stay byte-identical (Gate 2 `cargo public-api`). If FLAG
   3 lands as recommended (extend `Predicate`), the change is
   IMPLEMENTATION-LEVEL (new field on the struct, new builder method,
   new arm in `matches`); the `LogStore` trait itself is unchanged.

5. **No new module.** All new code in `crates/log-query-api/` lives in
   `src/lib.rs` alongside the existing handler. Any new code in
   `crates/lumen/` lives in `src/predicate.rs` alongside the existing
   builder methods.

6. **Error envelope reuse.** No new envelope shape, no new status code.
   The empty-value 400 reuses
   `{"status":"error","error":"invalid body_contains"}` via
   `query_http_common::error_response`. The reason text is a static
   literal; the (empty) raw value is NEVER interpolated.

7. **Redaction posture inherited.** The new 400 arm honours ADR-0047
   Decision 1 (no forwarded credential, no raw header value), the
   ADR-0050 Decision 7 symmetric extension (no raw parameter value
   echoed), and the ADR-0052 Decision 1 reaffirmation (the redaction
   posture applies to every new query-string parameter).

8. **No new metric, no new tracing event, no new dashboard.** Consistent
   with ADR-0050 Decision 8 and ADR-0052; the contract IS the signal.

9. **Existing acceptance suites must stay green unchanged.**
   `tests/slice_01_logs_read.rs`, `tests/slice_02_caps.rs`, and
   `tests/slice_01_severity_filter.rs` are NOT edited by this slice.
   The new acceptance suite is a new file
   (`tests/slice_01_body_contains.rs`) that adds to, never replaces,
   the existing coverage. KPI-2 in `outcome-kpis.md` enforces this.

10. **Cap interaction: filter BEFORE cap (post-filter records measured
    against the cap).** Consistent with ADR-0050 Decision 4 and the
    posture pinned by log-query-severity-filter-v0 (its FLAG 3
    recommendation, which DESIGN was expected to confirm). The slice's
    acceptance scenarios assume this posture; if DESIGN inverts it on
    this slice, the empty-substring-into-cap-budget scenario is
    rewritten before DELIVER (one scenario's expectations, not the
    whole suite).

## Requirements Summary

- **Primary jobs / user needs**: Sara Mendez (on-call SRE) holds an error
  string mid-incident and needs records carrying it from `/api/v1/logs`
  without `jq`-piping the whole window. Marcus Webb (platform engineer)
  polls a known failure signature every 60s for an incident classifier
  and needs payload + latency budget. Priya Raman (support engineer)
  triages a customer ticket that quotes an exact error message and
  needs to confirm the string appeared in the customer's tenant logs.
- **Walking skeleton scope**: NO new skeleton; brownfield additive on
  `/api/v1/logs`. The thinnest end-to-end demonstration is US-01:
  `body_contains=kafka%20timeout` against a fixture of six records
  narrows to the two matching records.
- **Feature type**: backend, thin slice on `crates/log-query-api`. One
  incidental additive change on `crates/lumen` if FLAG 3 lands as
  recommended.

## Constraints Established

- `query-http-common` is the SOLE provider of caps, reasons, envelope
  helper, tenant seam, and bounds parser. Zero new duplications in
  `log-query-api` (KPI-3 CI-enforced).
- New lines in `crates/log-query-api/src/lib.rs` under 30 (KPI-3
  CI-enforced).
- Substring matching, NOT regex.
- Case-sensitive matching, byte-wise.
- `LogRecord.body` field ONLY.
- Empty `body_contains` value is a 400 with the literal envelope; the
  raw value is NEVER reflected.
- Default (parameter absent) is byte-equal to the slice-prior response
  (KPI-2 CI-enforced).
- Conjunctive AND composition with `min_severity` via the existing
  `Predicate::matches` arm-by-arm conjunction.
- `lumen::LogStore` trait signatures stay byte-identical to the prior
  tag (Gate 2 `cargo public-api`).
- The result cap measures post-filter records (cap AFTER filter, as in
  log-query-severity-filter-v0).
- Cross-tenant isolation invariant holds for the new arm: tenant B
  never sees tenant A's matches.

## Upstream Changes

- **None.** No DISCOVER assumptions changed; no DIVERGE artefacts to
  back-propagate against. The slice composes additively on top of
  ADR-0047 / ADR-0050 / ADR-0052 / ADR-0054 without altering any of them.

## Flags to DESIGN (do NOT decide in DISCUSS; recommendations recorded)

These six decisions belong to the DESIGN wave (`nw-solution-architect`,
Morgan). DISCUSS records recommendations; DESIGN pins.

| # | Flag | Recommendation | Reason |
|---|---|---|---|
| 1 | **Substring vs regex** | SUBSTRING for slice 01 | A substring filter is the simplest predicate over a `String` field, the most predictable in cost, and the lowest-surprise default. Regex carries a ReDoS budget the slice is not ready to size and an expression-grammar contract the slice is not ready to publish. Regex is a clean separate future feature (`log-body-regex-search-vN`) whose design wave will weigh the ReDoS posture and the expression grammar (PCRE vs RE2 vs `regex` crate's syntax). |
| 2 | **Case-sensitivity** | CASE-SENSITIVE (grep-style) | A case-sensitive default has fewer surprises on false-positive matches: a customer's error string `INFO connection refused` should not match the platform's own `severity_text: "INFO"` boilerplate; case-folding amplifies that risk. `grep` is operator muscle memory and is case-sensitive by default. A case-insensitive option (a separate `body_contains_ci=<string>` parameter or a `case_sensitive=false` flag) is a future slice; the slice's KPI-4 explicitly pins the case-sensitive rule via acceptance test so operators learn it from where they will look. |
| 3 | **Predicate seam in lumen** | EXTEND `lumen::Predicate` with a `body_contains` builder and one new arm in `Predicate::matches` | Grep-verified: `lumen::Predicate` does NOT today carry a `body_contains` field (`crates/lumen/src/predicate.rs:25-28` declares only `service: Option<String>` and `min_severity: Option<SeverityNumber>`). The slice needs the predicate to grow ONE field, ONE builder, ONE arm in `matches`, and ONE clause in `is_empty()`. The alternative (apply the substring filter handler-side on the `Vec<LogRecord>` returned by `query_with`) keeps `lumen::Predicate` byte-identical but splits the predicate semantics across two crates and prevents the v1 columnar substrate from pushing the substring scan into the storage adapter where it belongs. The recommendation honours the existing seam ("the predicate IS the filter") and the v1 evolution path. If DESIGN judges the lumen surface change too costly to ship in this slice, shape (2) is a fallback; the user-visible behaviour is identical for both shapes. |
| 4 | **Empty string handling** | 400 with the LITERAL reason `invalid body_contains` | Symmetric with the empty-string rejection in log-query-severity-filter-v0's `parse_min_severity` (rejects `Some("")` as unknown rather than treating it as a missing-value shortcut). An empty `body_contains` substring is meaningless on `String::contains` (it matches every record, which is observably indistinguishable from no filter) and is therefore ambiguous between operator intent ("I meant the empty match") and operator error ("I dropped the substring"). The slice refuses the ambiguity out loud. The reason text is a static literal; the (empty) raw value is NEVER interpolated. |
| 5 | **Anti-echo on the empty-string 400** | The 400 body is the LITERAL envelope `{"status":"error","error":"invalid body_contains"}`; the raw value is NEVER interpolated | Honours ADR-0047 Decision 1, ADR-0050 Decision 7, and ADR-0052 Decision 1. For the empty-string arm specifically there is no non-empty raw value to echo; the recommendation is the redaction posture FOR ANY FUTURE ARM that might add a raw-value-bearing reason (e.g. "body_contains exceeds maximum length", "body_contains contains invalid UTF-8"). The acceptance scenario in US-01 / US-04 asserts the body is byte-equal to the literal envelope. |
| 6 | **ADR-0055 (small)** | YES if FLAG 3 lands as a lumen surface extension; NO if FLAG 3 lands as handler-side filtering only | ADR-0047, ADR-0050, ADR-0052, and ADR-0054 are immutable (repo-wide rule). Extending `lumen::Predicate` with a `body_contains` builder is a public-surface API change on the lumen crate, governed by Gate 2 `cargo public-api`, and worth its own ADR for searchability. Likely title: "ADR-0055 — lumen Predicate body_contains substring filter". The ADR would record the substring-vs-regex pin (FLAG 1), the case-sensitivity pin (FLAG 2), the empty-string-rejection pin (FLAG 4), and the lumen surface diff (one field + one builder + one match arm + one is_empty clause). If DESIGN pins FLAG 3 as handler-side filtering only, no public-surface change occurs in lumen, and a `wave-decisions.md` reference suffices. |

Each flag has a recommendation, a reason, and a name; DESIGN pins by
reading this section, weighing the recommendation, and recording the
choice in its own `design/wave-decisions.md` or in the ADR-0055 it
authors per FLAG 6.

## Risks (surfaced, not managed)

- **R-1 (LOW)**: DESIGN inverts FLAG 3 (handler-side filtering instead of
  lumen surface extension). Impact: the lumen surface stays byte-identical
  (zero `cargo public-api` diff), but the slice's predicate semantics
  split across two crates (the lumen `query_with` returns the
  range-and-tenant-and-severity-narrowed records; the handler then walks
  the vector and drops non-matching records). The acceptance scenarios
  stay the same; the mutation-test surface changes (the `contains`
  mutation lives in `log-query-api` instead of `lumen`). Mitigation:
  recorded explicitly; DELIVER can switch shapes without scenario rewrite.

- **R-2 (LOW)**: DESIGN inverts FLAG 2 (case-INsensitive matching).
  Impact: KPI-4's acceptance test is rewritten (`KAFKA` returns the
  matching record instead of `[]`); US-05 is rewritten to document the
  case-folding rule instead. Mitigation: trivial scenario rewrite; no
  acceptance test deletion. The recommendation reasoning is preserved
  in the flag table for the design wave to weigh.

- **R-3 (LOW)**: DESIGN judges the slice oversized because it touches two
  crates (`log-query-api` and `lumen`) instead of one. Impact: the slice
  is split into a lumen-surface slice (extend `Predicate`) and a
  log-query-api-wire slice (consume the new builder). Mitigation:
  recorded as a possible re-slicing decision; the acceptance scenarios
  do not change shape between the two slicings (the user-visible
  behaviour is the same).

- **R-4 (LOW)**: A future consumer (a prism log panel, a Grafana
  datasource) wants Loki-style `|=` syntax on the body. Mitigation:
  ADR-0047's posture explicitly accepts Loki-shaping as a later additive
  slice behind the same route; the substring-shaped filter does not
  preclude it.

- **R-5 (LOW)**: The walking-skeleton acceptance fixture's "every
  returned record's body contains the substring" KPI-1 assertion is too
  permissive if the fixture contains overlapping substrings (e.g. a
  record with body `xkafka timeoutx` legitimately contains
  `kafka timeout`). Mitigation: the fixture is small and deterministic
  (six records, exactly two of which carry the substring); DESIGN may
  calibrate the fixture without changing the slice's intent.

- **R-6 (LOW)**: DIVERGE wave was not run. Mitigation: the feature is a
  thin slice on a single API with one named user outcome; the JTBD is
  pinned by ADR-0047's operator persona and the brief. A retroactive
  DIVERGE is unnecessary at this scope but should be considered if
  successor slices grow regex, ranges, or multi-field text search.

- **R-7 (LOW)**: A consumer expects `body_contains` to match against
  `severity_text` as well (since `severity_text` looks like body-shaped
  text in the JSON response). Mitigation: the slice's
  `OUT of scope` section in `user-stories.md` explicitly names
  `severity_text` as out; the acceptance scenarios pin the rule.

## Definition of Ready (9-item gate)

See `dor-validation.md` for the full evidence table.

**DoR Status: PASSED.** Ready for DESIGN handoff.

## Handoff to DESIGN

The DESIGN wave (`nw-solution-architect`, Morgan) inherits:

1. The five artefacts in `docs/feature/log-body-text-search-v0/discuss/`:
   `user-stories.md`, `story-map.md`, `outcome-kpis.md`,
   `dor-validation.md`, `wave-decisions.md`.
2. The six flags above, each with a recommendation, a reason, and a name.
3. The read-first checklist above (every grounding source named with a
   file path and a line number where applicable).
4. The KPI-3 validation posture: this slice is the first
   `query-http-common` consumer born AFTER M-5, and the slice's CI
   assertions (static-grep + line-count) are the honest measure that
   the shared crate paid for itself.

DESIGN authors (or refines) ADR-0055 per FLAG 6 and produces the DESIGN
brief to the crafter.
