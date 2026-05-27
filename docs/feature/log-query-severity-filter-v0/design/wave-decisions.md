# Wave Decisions: log-query-severity-filter-v0 — DESIGN wave

Author: `@nw-solution-architect` (Morgan), DESIGN wave, 2026-05-27.
Mode: propose.

This file pins the four DISCUSS-wave flags and records the
parse-and-wire micro-decisions the crafter needs to GREEN the
slice without further design ambiguity. ADR-0052 carries the
durable cross-reference for the contract growth.

## Inputs read (read-first checklist)

The DESIGN wave was grounded in the following files, read in full
before any artefact was written:

- [x] `docs/feature/log-query-severity-filter-v0/discuss/user-stories.md`
      — five user stories, six Gherkin scenarios, the four flags
      named with recommendations.
- [x] `docs/feature/log-query-severity-filter-v0/discuss/story-map.md`
      — backbone, walking skeleton, scope assessment (PASS, 5
      stories, 1 bounded context, 1 day).
- [x] `docs/feature/log-query-severity-filter-v0/discuss/outcome-kpis.md`
      — three KPIs (payload reduction, backward compatibility,
      redacted 400); hypothesis; measurement plan; handoff to
      DEVOPS (no instrumentation requested).
- [x] `docs/feature/log-query-severity-filter-v0/discuss/wave-decisions.md`
      — eight scope and process decisions taken in DISCUSS; four
      flags to DESIGN; five risks (all LOW); DoR PASSED.
- [x] `docs/feature/log-query-severity-filter-v0/slices/slice-01-severity-filter.md`
      — the thin-slice description with walking-skeleton scenario,
      learning hypothesis and falsifiers, IN / OUT scope, mapping
      to user stories.
- [x] `docs/product/architecture/adr-0047-lumen-log-query-api-contract-and-crate-layout.md`
      — Decision 1 (bare JSON array success; `{status:"error",
      error}` envelope; redaction posture); Decision 5 (the lumen
      `LogStore` trait is unchanged; `query_with(predicate)` exists
      but is NOT used in slice 01 of `lumen-query-api-v0`). This
      ADR is the precedent ADR-0052 GROWS by one optional
      parameter, NOT modified.
- [x] `docs/product/architecture/adr-0050-earned-trust-read-side-caps.md`
      — Decision 1 (window cap 86_400s; uniform across the three
      crates); Decision 2 (result cap 100_000; measured AFTER the
      store and BEFORE serialisation); Decision 4 (the result-cap
      "measures what the user observes ... not the upstream raw
      row count"). This ADR is the precedent ADR-0052 honours at
      the filter-BEFORE-cap interaction, NOT modified.
- [x] `docs/product/architecture/adr-0051-pulse-per-tenant-cardinality-watermark.md`
      — the latest ADR; ADR-0052 is the next free number (`ls
      docs/product/architecture/adr-0052*` returns no hits;
      `adr-0051` is the latest).
- [x] `crates/lumen/src/store.rs` — `LogStore::query_with(&tenant,
      range, predicate)` at line 89 (the predicate-carrying seam);
      `InMemoryLogStore::query_with` at line 159 (honours the
      predicate via `predicate.matches(r)` at line 175). NO trait
      change needed.
- [x] `crates/lumen/src/record.rs` — the OTel SeverityNumber
      ladder at lines 32-39 (`TRACE=1`, `DEBUG=5`, `INFO=9`,
      `WARN=13`, `ERROR=17`, `FATAL=21`); `LogRecord` at line 44;
      `TimeRange` at line 98 (the half-open `[start, end)`).
- [x] `crates/lumen/src/predicate.rs` — `Predicate::new()` at
      line 33; `Predicate::min_severity(SeverityNumber)` at line 46;
      the `>=` semantics at line 60-63 (`if record.severity_number
      < floor { return false; }`); `Predicate::is_empty()` at line
      70. The substrate carries the exact semantics the HTTP slice
      needs.
- [x] `crates/log-query-api/src/lib.rs` — the handler shape
      (`handle_logs` at line 118); `LogsParams` at line 107
      (`start: String`, `end: String`); `parse_time_range_seconds`
      at line 190; `parse_epoch_seconds` at line 202 (the redaction
      precedent for the new helper); `MAX_WINDOW_SECONDS = 86_400`
      at line 70; `MAX_RESULT_ROWS = 100_000` at line 77;
      `error_response` at line 231; `success_response` at line 224;
      the existing `state.store.query(&tenant, range)` call at line
      147 (the dispatch point the parse-and-wire targets).

## Flags resolved (the four DISCUSS flags)

| # | Flag | Recommendation | DESIGN decision | Rationale |
|---|---|---|---|---|
| 1 | Wire parameter name | `min_severity` | **PIN: `min_severity`** | Aligns with the `>=` floor semantics; avoids `level`'s ambiguous "exactly vs and-above" connotation; matches the lumen builder method name `Predicate::min_severity(SeverityNumber)` verbatim, removing cognitive translation between wire and substrate. |
| 2 | Case-sensitivity | Case-insensitive on six OTel names; no aliases | **PIN: case-insensitive, no aliases** | Case-insensitivity matches operator muscle memory across `syslog`, OTel SDKs, and ad-hoc curl usage with zero correctness cost. Alias rejection matches the honest-refusal posture: a typo (`WARNING`) is a typo, refused with the existing named 400; aliases would mask the platform's contract and foreclose a future `severity_text`-based filter that may legitimately want `"WARNING"` as a distinct user-defined label (the substrate distinguishes `severity_number` from `severity_text`; the HTTP boundary should not pre-coerce them). |
| 3 | Filter BEFORE or AFTER the result cap | BEFORE | **PIN: BEFORE** | An operator running `min_severity=ERROR` wants every matching ERROR record up to the cap, not a cap-400 caused by an INFO storm the request explicitly asked to filter out. Aligns with ADR-0050 Decision 4 ("the check measures what the user observes ... not the upstream raw row count"). The implementation lands this naturally if the filter rides inside the store via `query_with` and the cap measures the returned `Vec::len()`. |
| 4 | ADR-0052 vs refinement of ADR-0047 | Small new ADR-0052 | **PIN: write ADR-0052** | ADR-0047 is immutable (repo-wide ADR rule, set by ADR-0001 and honoured by every preceding ADR including ADR-0049, ADR-0050, and ADR-0051). The read-side log API contract is growing by one optional parameter (a new accepted parameter name, a new 400 reason class, a new accepted-value set); that is a contract growth worth a small ADR with cross-reference to 0047 and 0050. ADR-0052 number verified free by `ls docs/product/architecture/adr-0052*` (no hits; `adr-0051` is the latest). |

## Other decisions pinned (parse + wire micro-decisions)

These are the small mechanical choices the crafter needs and the
acceptance designer needs in order to write the slice without
further design ambiguity. They are NOT contract decisions; they
are HOW-it-wires inside the existing handler and helpers.

### D5. Wiring: extend `LogsParams`, branch the handler, one new 400 arm

The existing `LogsParams` struct (`crates/log-query-api/src/lib.rs:107`)
grows ONE additive field:

- `min_severity: Option<String>` — `serde` deserialises the
  missing parameter as `None`; a present empty value
  (`?min_severity=`) is `Some(String::new())` and is rejected by
  the parse helper as "unknown severity" (the empty string is
  not one of the six OTel names).

The handler grows the following AFTER `parse_time_range_seconds`
returns `Ok` and AFTER the window-cap check passes (so the parse
order is: tenancy -> window parse -> window cap -> severity
parse), and BEFORE the existing `state.store.query(...)` call:

```text
let min_sev: Option<SeverityNumber> = match params.min_severity.as_deref() {
    None => None,
    Some(raw) => match parse_min_severity(raw) {
        Ok(sev) => Some(sev),
        Err(reason) => return error_response(StatusCode::BAD_REQUEST, &reason),
    },
};
```

The store dispatch branches on the resolved option:

```text
let records = match min_sev {
    Some(floor) => state.store.query_with(
        &tenant,
        range,
        &Predicate::new().min_severity(floor),
    ),
    None => state.store.query(&tenant, range),
};
```

(The above is illustrative, NOT a code prescription; the crafter
owns the GREEN / REFACTOR shape. The decisions pinned here are:
the helper name, the dispatch shape branched on `Option`, and
the order of checks.)

The result-cap check on `records.len() > MAX_RESULT_ROWS` stays
EXACTLY where it is (`crates/log-query-api/src/lib.rs:153`);
only the source of the `Vec<LogRecord>` it measures changes when
the parameter is present (post-filter via `query_with`,
unchanged via `query`).

### D6. Parse helper signature and behaviour

Free function in `crates/log-query-api/src/lib.rs`, named in the
same shape as `parse_time_range_seconds` and
`parse_epoch_seconds`:

```text
fn parse_min_severity(raw: &str) -> Result<SeverityNumber, String>
```

Behaviour:

1. Trim leading and trailing ASCII whitespace.
2. Match case-insensitively against the six OTel names:
   - `"TRACE"` -> `SeverityNumber::TRACE`
   - `"DEBUG"` -> `SeverityNumber::DEBUG`
   - `"INFO"` -> `SeverityNumber::INFO`
   - `"WARN"` -> `SeverityNumber::WARN`
   - `"ERROR"` -> `SeverityNumber::ERROR`
   - `"FATAL"` -> `SeverityNumber::FATAL`
3. Any other value (typos, aliases, empty string,
   `"UNSPECIFIED"` — the `SeverityNumber::UNSPECIFIED = 0`
   constant is NOT an accepted wire value) returns
   `Err("unknown severity".to_string())`. The reason text
   matches the literal from the user-stories § "An unknown
   severity name is a redacted 400" scenario.

The function is `fn`, not `pub fn`: same visibility as the
existing `parse_time_range_seconds`. Inline unit tests cover the
six accepted names in mixed cases, the rejection of `WARNING`,
the rejection of `""`, and the rejection of `"UNSPECIFIED"`.

The match logic uses `eq_ignore_ascii_case` (case-folding for the
ASCII range only; the six OTel names are all ASCII so a UTF-8
case-fold is unnecessary; a future operator typing a non-ASCII
character is rejected as unknown, which is the honest answer).

### D7. Existing helpers reused unchanged

- `error_response(StatusCode::BAD_REQUEST, "unknown severity")`
  for the unknown-severity 400. The reason text is the literal
  string `"unknown severity"`; it does NOT echo the raw value;
  the existing redaction precedent
  (`the_bounds_error_never_echoes_the_raw_value` test) extends
  to the new arm.
- `success_response(records)` for the 200 arm; unchanged.
- `parse_time_range_seconds` and `parse_epoch_seconds` for the
  window; unchanged.
- The window-cap check at line 141 and the result-cap check at
  line 153; unchanged in shape, unchanged in location, unchanged
  in reason text.

### D8. The unknown-severity 400 arm NEVER touches the store

The order of checks is enforced by the handler's control flow:
the `parse_min_severity` call returns `Err` BEFORE the dispatch
match; on `Err`, the handler returns the 400 via
`error_response`. The store is NEVER called on the
unknown-severity path. The acceptance scenario
("An unknown severity name is a redacted 400") encodes this
with a no-store-call assertion (test double's `query` /
`query_with` counter is asserted zero).

### D9. Public API surface

The `LogsParams` struct is a private internal type (line 107,
no `pub`); the `min_severity` field addition does NOT appear in
the `cargo public-api` diff. The `LOGS_ROUTE` constant
(`/api/v1/logs`) is unchanged. The `MAX_WINDOW_SECONDS` and
`MAX_RESULT_ROWS` public constants are unchanged. The `router`
function signature is unchanged. The crate's public API is
byte-identical to the prior tag.

The `lumen::LogStore` trait signatures are byte-identical (Gate 2
`cargo public-api` on lumen confirms); no method is added,
removed, or re-signed.

## Reuse Analysis (mandatory table)

| Component | Verdict | Notes |
|---|---|---|
| `crates/lumen/src/store.rs` (`LogStore::query_with`) | REUSE unchanged | The predicate-carrying seam already exists at line 89; the slice uses it for the first time on the HTTP boundary, which is consistent with ADR-0047 Decision 5 (the trait method exists; its USE was deferred). No trait change. |
| `crates/lumen/src/predicate.rs` (`Predicate::new`, `Predicate::min_severity`) | REUSE unchanged | The builder pattern and the `>=` semantics are present at lines 33 and 46. The slice constructs `Predicate::new().min_severity(floor)` per request; no new builder method, no new field. |
| `crates/lumen/src/record.rs` (`SeverityNumber` constants) | REUSE unchanged | The six OTel constants at lines 32-39 are the mapping target for the parse helper. No new constant, no value change. |
| `crates/log-query-api/src/lib.rs` (`LogsParams`) | EXTEND with one additive field | One `min_severity: Option<String>` field added to the existing struct. No public-API impact (struct is private). |
| `crates/log-query-api/src/lib.rs` (`handle_logs`) | EXTEND with one parse step and a branched dispatch | One new `match` block to parse `min_severity`; one new `match` block to choose between `query_with` and `query`; the rest of the handler (tenancy, window parse, window cap, result cap, success / error response) is unchanged. |
| `crates/log-query-api/src/lib.rs` (parse helpers) | ADD ONE FREE FUNCTION `parse_min_severity` | Lives next to `parse_time_range_seconds` and `parse_epoch_seconds`; same shape (`fn`, returns `Result<T, String>`). |
| `crates/log-query-api/src/lib.rs` (`error_response`, `success_response`) | REUSE unchanged | Both helpers are called from the new arms unchanged. |
| `crates/log-query-api/src/lib.rs` (`MAX_WINDOW_SECONDS`, `MAX_RESULT_ROWS`) | REUSE unchanged | Both public constants are unchanged; the cap arms are unchanged in location, value, and reason text. |
| `crates/log-query-api/src/composition.rs` | UNCHANGED | The composition root, the Earned-Trust startup probe, and the tenant resolution are unchanged. |
| `crates/log-query-api/src/main.rs` | UNCHANGED | The thin binary is unchanged. |
| `crates/lumen/src/file_backed.rs` (`FileBackedLogStore::query_with`) | REUSE unchanged | The file-backed adapter already implements `query_with`; the slice uses it as designed. |
| `crates/log-query-api/tests/slice_01_logs_read.rs` | UNCHANGED (NOT EDITED) | DISCUSS Decision 8 pins this: the existing acceptance suite must stay green unchanged. |
| `crates/log-query-api/tests/slice_02_caps.rs` | UNCHANGED (NOT EDITED) | DISCUSS Decision 8 pins this; the `BulkLogStore` test-double pattern at line 86 is REUSED (not edited) by the new slice's filter-BEFORE-cap scenario. |
| NEW `crates/log-query-api/tests/slice_01_severity_filter.rs` | CREATE (DISTILL output, NOT this DESIGN's output) | The acceptance file is created by the DISTILL wave (`@nw-acceptance-designer`); this DESIGN wave records the scenarios it must encode. |
| `query-http-common` crate | NOT CREATED | ADR-0048 Decision 5 / M-5 deferral HONOURED. |

**Reuse verdict**: the slice is parse + wire inside the existing
`crates/log-query-api/src/lib.rs`. No new crate. No new external
dependency. No new module. No new file under `crates/lumen/src/`.
No new file under `crates/log-query-api/src/`. The only new file
in the workspace is the NEW acceptance suite
`crates/log-query-api/tests/slice_01_severity_filter.rs` (a
DISTILL-wave output, NOT a DESIGN-wave output).

## DEVOPS handoff annotation

- **NO new crate.** The change is parse + wire inside
  `crates/log-query-api`.
- **NO new external dependency.** The parse helper uses
  `std::primitive::str::eq_ignore_ascii_case` (core); the lumen
  `Predicate` and `SeverityNumber` constants are existing
  in-process types. No new third-party API consumed; no
  consumer-driven contract test recommendation.
- **NO new CI workflow.** The existing
  `gate-5-mutants-log-query-api` job covers the modified file
  (`crates/log-query-api/src/lib.rs`) via `--in-diff` at the
  100% kill-rate gate (ADR-0005 Gate 5; CLAUDE.md). The
  existing `gate-2-public-api` job covers the public-API
  byte-identity (lumen trait signatures and the
  `log-query-api` `pub` surface).
- **NO new graduation tag.** The slice's surface is internal to
  the existing `log-query-api` crate; the `router()` signature is
  unchanged; the `LogsParams` field addition is private and does
  NOT appear in the public-API diff.
- **External integrations: none new** (the parse helper is
  in-process string matching; the store call uses an in-process
  trait method against the durable `FileBackedLogStore`, which
  is a first-party library, not a network service).
- **Per-feature mutation 100%** scoped to the modified files
  (`crates/log-query-api/src/lib.rs`) via the existing
  workflow.

## Primary mutation targets

Each target is a fertile-ground for `cargo mutants` and is
covered by the slice-01 acceptance suite (DISTILL output):

| Target | Mutant class | Killing scenario |
|---|---|---|
| `>=` boundary on `Predicate::min_severity` (existing lumen semantics; the HTTP boundary inherits) | `>=` -> `>` (record at exactly the floor would be EXCLUDED) | US-03 first scenario: a `min_severity=WARN` request with a record at severity 13 returns the record. |
| `>=` boundary on `Predicate::min_severity` | `>=` -> `<` (filter inverts; would return below-floor records only) | US-01 walking skeleton: only WARN+ERROR records appear; no INFO records appear. |
| Six-name mapping table in `parse_min_severity` | Drop / rename any one of the six names | Per-name acceptance assertion: the slice asserts each of `TRACE`, `DEBUG`, `INFO`, `WARN`, `ERROR`, `FATAL` is accepted in at least one canonical case. |
| Case-insensitivity (`eq_ignore_ascii_case`) | `eq_ignore_ascii_case` -> `eq` (only `WARN` accepted, `warn` rejected) | The slice asserts at least two different cases (`WARN`, `warn`) map to the same `SeverityNumber`. |
| Redaction in the `"unknown severity"` reason text | Echo the raw parameter value into the reason | US-05 redaction assertion: the unknown-severity 400 body must NOT contain the literal `WARNING`. |
| Order of checks (severity parse BEFORE store call) | Move the parse AFTER the store call | US-05 no-store-call assertion: the test double's `query` and `query_with` counters are both zero on the unknown-severity 400 path. |
| Dispatch branch (`Some` -> `query_with`, `None` -> `query`) | Always call `query`, ignore the predicate | US-01 walking skeleton: the response would contain INFO records, failing the assertion. |
| Dispatch branch (`Some` -> `query_with`, `None` -> `query`) | Always call `query_with` with an empty predicate, regardless of parameter presence | US-02 default-unchanged: the response would be byte-equal but the `record_query` metric semantics would change; the acceptance suite asserts behavioural identity to the parameter-less path. |
| Filter BEFORE cap (the cap measures post-filter records) | Reverse the cap and filter ordering | US-04 cap-interaction: a 150k INFO + 50k ERROR fixture with `min_severity=ERROR` returns 200 with 50k records, NOT a cap-400. |

## Risks (carried from DISCUSS, addressed)

| # | Risk | Status |
|---|---|---|
| R-1 | DESIGN inverts FLAG 3 (cap BEFORE filter) | NOT INVERTED. Recommendation pinned (filter BEFORE cap). US-04 scenario carries the recommendation as encoded. |
| R-2 | DESIGN inverts FLAG 2 (strict case-sensitivity) | NOT INVERTED. Recommendation pinned (case-insensitive, no aliases). The case-insensitivity scenarios in user-stories.md remain valid. |
| R-3 | Future Loki-shaped severity filtering need | UNCHANGED. ADR-0047 Decision 1's posture explicitly accepts Loki-shaping as a later additive slice behind the same route. The OTel-shaped filter does not preclude it. |
| R-4 | KPI-1 5x payload multiplier too tight / too loose | CARRIED. DELIVER may calibrate the multiplier with the actual fixture; the fixture is small and deterministic; the multiplier is a KPI target, not a contract pin. |
| R-5 | DIVERGE wave was not run | CARRIED. The slice is a thin growth on a single API with one named user outcome; the JTBD is pinned by ADR-0047's operator persona. A retroactive DIVERGE is unnecessary at this scope. |

## Handoff to DISTILL

The DISTILL wave (`@nw-acceptance-designer`) inherits:

1. The four DESIGN artefacts in
   `docs/feature/log-query-severity-filter-v0/design/`:
   `wave-decisions.md` (this file) and
   `application-architecture.md`.
2. The new ADR
   `docs/product/architecture/adr-0052-log-query-severity-filter.md`.
3. The brief.md application-architecture section
   appended at `docs/product/architecture/brief.md` §
   "Application Architecture — log-query-severity-filter-v0".
4. The four flags pinned (`min_severity`, case-insensitive,
   filter BEFORE cap, ADR-0052) and the parse + wire
   micro-decisions (D5-D9).
5. The six Gherkin scenarios from `discuss/user-stories.md` to
   translate into `#[test]` functions in the new
   `crates/log-query-api/tests/slice_01_severity_filter.rs`,
   following the existing `tests/slice_01_logs_read.rs`
   conventions (`open_durable_store`, `tenant`, `seed`,
   `record`, `record_at_nanos`, `rich_record`, `logs_request`,
   `records_array`, `record_bodies`, `is_error_envelope`).
6. The per-name case-insensitive assertions and the no-store-call
   assertion on the unknown-severity 400 path.

## Handoff to DEVOPS (Apex)

The DEVOPS wave (`@nw-platform-architect`, Apex) inherits:

- NO new CI job, NO new graduation tag, NO new dependency, NO
  new env variable.
- The existing `gate-5-mutants-log-query-api` covers the
  modified file via `--in-diff` at the 100% kill-rate gate.
- The existing `gate-2-public-api` job confirms the public-API
  surface is byte-identical to the prior tag (the `LogsParams`
  field addition is private; the `lumen::LogStore` trait
  signatures are unchanged).
- No external integration, no consumer-driven contract test
  recommendation.

## Contradictions with DISCUSS

None. All four DISCUSS-wave recommendations are pinned as
recommended. The eight DISCUSS-wave scope decisions (no lumen
change, no new module, no `query-http-common` extraction, error
envelope reuse, redaction inherited, no new metric, existing
suites unchanged) are all preserved. The case-insensitivity
scenarios in user-stories.md remain valid. The filter-BEFORE-cap
scenario in user-stories.md remains valid as written.
