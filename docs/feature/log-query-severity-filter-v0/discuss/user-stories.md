<!-- markdownlint-disable MD024 -->

# User Stories: log-query-severity-filter-v0

Slice 01, thin. The log read endpoint `GET /api/v1/logs?start=&end=` grows ONE
optional request parameter that filters returned `LogRecord`s by minimum OTel
severity. Default behaviour (parameter absent) is unchanged: every record in the
window is returned. The parameter NAME is a DESIGN-wave flag (the brief
recommends `min_severity`); the stories below use `<min_severity_param>` as a
placeholder so DESIGN can pin the wire spelling without re-opening DISCUSS.

This is a thin slice on top of `log-query-api`, NOT a cross-crate refactor.
The `query-http-common` extraction (ADR-0048 Decision 5; ADR-0050 §5) stays
DEFERRED. No lumen trait change. No new module. No new ADR text in DISCUSS:
DESIGN will decide whether the API contract growth is ADR-0052 or a refinement
of ADR-0047 (FLAG 4).

## System Constraints (cross-cutting)

- The existing `MAX_WINDOW_SECONDS` (86_400) and `MAX_RESULT_ROWS` (100_000)
  caps from ADR-0050 are PRESERVED unchanged. The new filter MUST NOT remove,
  reorder, or weaken either cap.
- The error envelope on rejected input is the existing
  `{"status":"error","error":"<reason>"}` shape (ADR-0047 Decision 1).
  No new envelope. No new status code.
- The error text MUST NOT echo the raw parameter value (ADR-0047 redaction
  posture; symmetric with ADR-0050 Decision 7).
- The bare-JSON-array success shape (ADR-0047 Decision 1) is preserved. The
  filter changes WHICH records appear, not the shape of the response.
- The `lumen::LogStore` trait signatures stay byte-identical (Gate 2
  `cargo public-api`). The handler uses the EXISTING `query_with(&tenant,
  range, predicate)` seam already on the trait and the EXISTING
  `Predicate::min_severity(sev)` builder on `lumen::Predicate`. No new trait
  method, no new module.
- The OTel SeverityNumber ladder is the source of truth for ordering:
  TRACE=1, DEBUG=5, INFO=9, WARN=13, ERROR=17, FATAL=21
  (`crates/lumen/src/record.rs:32-39`). The filter is `>=` on the numeric
  ladder.

## OUT of scope (DECLARED and DEFERRED)

The following are EXPLICITLY out for slice 01 and named so DESIGN does not
re-discover them as gaps:

- Filtering on `severity_text` (custom non-OTel labels such as `"WARNING"`,
  `"critical"`, `"err"`). The filter is on `severity_number` only.
- Severity RANGES (e.g. WARN+ERROR but NOT FATAL). The filter is a single
  floor, not a set or interval.
- Body regex / substring filtering.
- Record-attribute filtering (e.g. `http.status_code`).
- Resource-attribute filtering (e.g. `service.name`). The lumen `Predicate`
  already supports `service(name)`, but the HTTP slice does not expose it
  yet; that is a SEPARATE slice.
- Aliases for severity names (e.g. `WARNING` -> `WARN`, `err` -> `ERROR`).
  Recommended in FLAG 2 but not adopted at slice 01.
- Env-driven default severity floor (e.g. `KALEIDOSCOPE_LOG_QUERY_MIN_SEVERITY`).
  Defaults stay compile-time absent (no filter).

## US-01 Walking skeleton: a min-severity floor drops records below the floor

### Elevator Pitch

- **Before**: Sara, on-call SRE for tenant `acme-prod`, GETs
  `/api/v1/logs?start=1716200000&end=1716200060` mid-incident and the response
  carries every record in the window, including thousands of INFO and DEBUG
  records that drown the WARN and ERROR records she actually needs. She
  downloads everything and filters client-side, which costs seconds she does
  not have and bandwidth she has paid for.
- **After**: Sara GETs
  `GET /api/v1/logs?start=1716200000&end=1716200060&<min_severity_param>=WARN`
  and the HTTP 200 body is a bare JSON array carrying ONLY the WARN and ERROR
  records from the window. The INFO and DEBUG records that were in the window
  are excluded by the platform before serialisation. The shape is identical
  to today's response; only the row set is narrower.
- **Decision enabled**: Sara decides which records to inspect next
  (paging through ERROR and WARN entries by `observed_time`, picking the
  earliest WARN that aligns with the incident start), without spending
  attention on INFO/DEBUG noise.

### Problem

Sara Mendez is the on-call SRE for tenant `acme-prod`. Mid-incident she queries
`GET /api/v1/logs?start=1716200000&end=1716200060`. The window holds 800
records: 600 INFO heartbeats, 150 WARN slow-upstream notices, 40 ERROR
checkout-timeout entries, and 10 DEBUG diagnostics. She has to read the WARN
and ERROR rows to triage; the INFO/DEBUG rows are noise that costs her seconds
of attention and her terminal screen of real estate. Today she pipes the
response through `jq 'map(select(.severity_number >= 13))'` after downloading
all 800 records. The server-side filter would deliver the same 190 records
without the 610 wasted.

### Who

- **Sara Mendez** | SRE on `acme-prod`, mid-incident, terminal + curl + jq |
  Triage urgency: needs to see WARN+ERROR within seconds of querying.
- **Marcus Webb** | platform engineer building an automated alerting pipe that
  pulls `WARN+` records every 30 seconds for an upstream incident classifier |
  Throughput motive: payload size dominates the request budget.

### Solution

`GET /api/v1/logs` accepts an optional query-string parameter (DESIGN flag for
the exact spelling; recommended `min_severity`) whose value is one of the six
OTel severity level names: `TRACE`, `DEBUG`, `INFO`, `WARN`, `ERROR`, `FATAL`.
The handler maps the name to its `SeverityNumber` via the existing ladder
(`crates/lumen/src/record.rs:32-39`) and constructs a
`lumen::Predicate::new().min_severity(SeverityNumber::WARN)` (or the
corresponding constant for the supplied name), then calls
`state.store.query_with(&tenant, range, &predicate)` instead of the existing
`state.store.query(&tenant, range)`. Records whose `severity_number` is below
the floor are excluded before the result-cap check and before serialisation.
Default (parameter absent) constructs no predicate (or an empty one) and
behaves exactly as today.

### Domain Examples

#### 1: Happy path — Sara filters to WARN+ during a payment-timeout incident

Tenant `acme-prod` has six records in `[1716200000s, 1716200060s)`:

| `observed_time_unix_nano` | `severity_number` | `severity_text` | `body` |
|---|---|---|---|
| `1_716_200_005_000_000_000` | 9 (INFO) | INFO | `checkout: heartbeat` |
| `1_716_200_010_000_000_000` | 9 (INFO) | INFO | `checkout: heartbeat` |
| `1_716_200_015_000_000_000` | 13 (WARN) | WARN | `checkout: slow upstream` |
| `1_716_200_020_000_000_000` | 17 (ERROR) | ERROR | `checkout: payment timeout` |
| `1_716_200_025_000_000_000` | 9 (INFO) | INFO | `checkout: heartbeat` |
| `1_716_200_030_000_000_000` | 13 (WARN) | WARN | `checkout: slow upstream` |

Sara runs
`curl 'http://logs.kaleidoscope.acme.internal/api/v1/logs?start=1716200000&end=1716200060&<min_severity_param>=WARN'`.
Response is HTTP 200 with a bare JSON array of three records: the two WARN and
the one ERROR, in ascending `observed_time_unix_nano` order. The three INFO
records are excluded. The shape of each record is identical to today's
response (same field set, same field names, same serialisation).

#### 2: Default unchanged — Marcus's old script keeps working

Marcus has an automation script that calls
`curl '.../api/v1/logs?start=1716200000&end=1716200060'` every 60 seconds.
The script is NOT updated when slice 01 ships. The response is identical to
the response it received the day before slice 01: every in-window record,
in ascending `observed_time_unix_nano` order, INFO and DEBUG included. The
backward-compatibility promise (the parameter is optional, absence is
no-filter) is honoured.

#### 3: Boundary inclusive — `min_severity=WARN` includes records at exactly WARN

Tenant `acme-prod` has three records in
`[1716200000s, 1716200060s)`:

| `observed_time_unix_nano` | `severity_number` | `severity_text` |
|---|---|---|
| `1_716_200_005_000_000_000` | 13 (WARN) | WARN |
| `1_716_200_010_000_000_000` | 9 (INFO) | INFO |
| `1_716_200_015_000_000_000` | 17 (ERROR) | ERROR |

Sara queries with `<min_severity_param>=WARN`. The response is a JSON array of
TWO records: the WARN (severity_number 13 == floor 13 is INCLUDED) and the
ERROR (17 > 13 is INCLUDED). The INFO (9 < 13 is EXCLUDED) does not appear.
The comparison is `>=`, NOT `>`, on the numeric ladder.

#### 4: Error path — an unknown severity name returns a redacted 400

Sara fat-fingers
`curl '.../api/v1/logs?start=1716200000&end=1716200060&<min_severity_param>=WARNING'`
(she meant `WARN`). The handler does NOT recognise `WARNING` as one of the
six OTel names. The response is HTTP 400 with the existing envelope
`{"status":"error","error":"unknown severity"}`. The body does NOT contain
the literal string `WARNING` (redaction posture; ADR-0047 Decision 1
symmetric with ADR-0050 Decision 7). The store is NEVER touched on this
path.

## UAT Scenarios

### Scenario: A min-severity floor returns only records at-or-above the floor

```gherkin
Given tenant "acme-prod" has six records in the window [1716200000s, 1716200060s):
  | observed_time_secs | severity_number | severity_text | body                       |
  | 1716200005         | 9               | INFO          | checkout: heartbeat        |
  | 1716200010         | 9               | INFO          | checkout: heartbeat        |
  | 1716200015         | 13              | WARN          | checkout: slow upstream    |
  | 1716200020         | 17              | ERROR         | checkout: payment timeout  |
  | 1716200025         | 9               | INFO          | checkout: heartbeat        |
  | 1716200030         | 13              | WARN          | checkout: slow upstream    |
When Sara GETs /api/v1/logs?start=1716200000&end=1716200060&<min_severity_param>=WARN
Then the status is 200
And the body is a bare JSON array of exactly three records in ascending observed_time order
And no record has severity_number below 13 (WARN)
And no INFO heartbeat record appears in the response
```

### Scenario: Parameter absent returns every record in the window (default unchanged)

```gherkin
Given tenant "acme-prod" has the same six records as the previous scenario
When Sara GETs /api/v1/logs?start=1716200000&end=1716200060 with NO severity parameter
Then the status is 200
And the body is a bare JSON array of all six in-window records
And the response is byte-equal to the slice-prior response for the same inputs
```

### Scenario: The min-severity boundary is inclusive (== floor passes)

```gherkin
Given tenant "acme-prod" has one record with severity_number exactly 13 (WARN)
And the record's observed_time is inside the window
When Sara GETs /api/v1/logs over the window with <min_severity_param>=WARN
Then the status is 200
And the WARN record appears in the response (== floor is included)
```

### Scenario: A record one notch below the floor is excluded

```gherkin
Given tenant "acme-prod" has one INFO record (severity_number 9) inside the window
And no other in-window records
When Sara GETs /api/v1/logs over the window with <min_severity_param>=WARN
Then the status is 200
And the response is the calm empty bare array []
And no INFO record appears in the response
```

### Scenario: An unknown severity name is a redacted 400

```gherkin
Given the handler resolves a valid tenant "acme-prod"
And the window parses cleanly
When Sara GETs /api/v1/logs over the window with <min_severity_param>=WARNING
Then the status is 400
And the body is the existing error envelope {"status":"error","error":"unknown severity"}
And the body does NOT contain the literal substring "WARNING"
And the store is NEVER queried on this path
```

### Scenario: The filter applies BEFORE the result cap

```gherkin
Given tenant "acme-prod" has 150_000 INFO records and 50_000 ERROR records inside an in-cap window
When Sara GETs /api/v1/logs over the window with <min_severity_param>=ERROR
Then the response carries the 50_000 ERROR records (well under MAX_RESULT_ROWS = 100_000)
And the response is HTTP 200, NOT a result-cap 400
And the INFO records do not consume cap budget
```

> **Note**: This scenario encodes the LIKELY behaviour (FLAG 3 recommendation:
> filter BEFORE cap). DESIGN may decide to invert this; if so, this scenario
> is rewritten to expect HTTP 400 with the result-cap envelope. The decision
> is flagged, not pinned.

## Acceptance Criteria

- [ ] An optional query-string parameter (DESIGN-pinned name; placeholder
      `<min_severity_param>`) is accepted on `GET /api/v1/logs`.
- [ ] The accepted values are the six OTel severity level names:
      `TRACE`, `DEBUG`, `INFO`, `WARN`, `ERROR`, `FATAL`.
- [ ] When the parameter is present, only records whose `severity_number` is
      greater than or equal to the requested floor appear in the response.
- [ ] When the parameter is absent, the response is identical to the
      slice-prior response (every in-window record, no filter).
- [ ] An unknown or malformed severity value returns HTTP 400 with the
      existing `{"status":"error","error":"unknown severity"}` envelope.
- [ ] The error body does NOT echo the raw severity parameter value.
- [ ] The window cap and result cap from ADR-0050 are preserved unchanged.
- [ ] The bare JSON array response shape from ADR-0047 Decision 1 is
      preserved unchanged.
- [ ] The `lumen::LogStore` trait signatures are byte-identical to the prior
      tag (Gate 2 `cargo public-api`).
- [ ] The half-open `[start, end)` window from ADR-0047 §3 is preserved
      unchanged.

## Outcome KPIs

See `outcome-kpis.md` for the full table. Story-level summary:

- **Who**: SRE operators and automation clients of the log read API on tenants
  with high INFO/DEBUG volume.
- **Does what**: Issue narrowed read requests (`<min_severity_param>=WARN` or
  stricter) instead of pulling every in-window record and filtering
  client-side.
- **By how much**: Median response payload on a narrowed read drops by at
  least 5x against the same window without the parameter, on a representative
  fixture (target: a 60s window of typical INFO-heavy production traffic).
- **Measured by**: Synthetic seeded-fixture acceptance assertion (the slice
  ships with a fixture; baseline is the pre-filter byte count, target is the
  post-filter byte count for the same fixture with `<min_severity_param>=WARN`).
- **Baseline**: 100% of in-window records returned today (no filter exists).

## Technical Notes

- **Existing seam**: `lumen::LogStore` already exposes `query_with(&tenant,
  range, &Predicate)` (`crates/lumen/src/store.rs:89`) and
  `lumen::Predicate` already exposes `min_severity(SeverityNumber)`
  (`crates/lumen/src/predicate.rs:46`). The slice uses these. No new trait
  method, no new lumen module.
- **Severity constants**: Use the existing `SeverityNumber::TRACE` ...
  `SeverityNumber::FATAL` associated constants
  (`crates/lumen/src/record.rs:32-39`). Do not hard-code numeric ladder
  values in `log-query-api`.
- **Parsing location**: The string-to-`SeverityNumber` mapping lives in
  `crates/log-query-api/src/lib.rs` (the handler) alongside the existing
  `parse_time_range_seconds`. It is a separate free function, NOT a method on
  `lumen::Predicate` (the lumen crate stays string-mapping-free).
- **Order of checks** (subject to FLAG 3 confirmation):
  1. Resolve tenant (fail-closed 401 — UNCHANGED).
  2. Parse window (400 on malformed — UNCHANGED).
  3. Window-cap check (400 if `end_secs - start_secs > MAX_WINDOW_SECONDS` —
     UNCHANGED; ADR-0050).
  4. Parse severity name if present (NEW; 400 with `"unknown severity"` on
     malformed). Store is NOT touched on this path.
  5. Call `query_with(&tenant, range, &predicate)` (NEW seam choice; replaces
     today's `query` when a predicate is constructed; the existing
     `query` call is retained on the default-no-filter path OR replaced by
     `query_with(empty_predicate)` — DESIGN micro-decision, not flagged
     because both behave identically on the no-predicate path).
  6. Result-cap check (400 if `records.len() > MAX_RESULT_ROWS` — UNCHANGED;
     ADR-0050; measured POST-filter per FLAG 3 recommendation).
  7. `success_response(records)`.
- **Mutation targets** (handed to DESIGN as fertile ground for Gate 5):
  - The `>=` boundary on the severity floor (a `>=` -> `>` mutant must be
    killed by the boundary-inclusive scenario).
  - The six-name mapping table (a mutant that drops or renames any of the six
    names must be killed by a per-name acceptance assertion).
  - The redaction on the unknown-severity 400 (a mutant that echoes the raw
    parameter value must be killed by the redaction assertion).
  - The order-of-checks: a mutant that calls `query` BEFORE parsing the
    severity name must be killed by an assertion that the store is NOT
    touched on the unknown-severity 400.

## Dependencies

- **Resolved**: ADR-0047 (log-query-api contract), ADR-0050 (read-side caps),
  `lumen::LogStore::query_with`, `lumen::Predicate::min_severity`,
  `lumen::SeverityNumber` constants. All present in the repository on the
  prior tag.
- **Tracked (not blockers)**: DESIGN flags 1-4 below.

## Flags to DESIGN (do NOT decide in DISCUSS)

1. **Parameter name on the wire**. Recommended `min_severity`; alternatives
   `level`, `severity_min`. DESIGN pins one; DISCUSS uses the placeholder
   `<min_severity_param>` so the choice does not require re-opening these
   stories.
2. **Case-sensitivity of the severity name**. Strict (`WARN` only) or
   case-insensitive (`warn`, `Warn`, `WARN` all accepted). Recommended
   case-insensitive on the six OTel names; NO aliases (no `WARNING` for
   `WARN`, no `err` for `ERROR`) at slice 01. DESIGN pins; DISCUSS records
   the recommendation.
3. **Filter BEFORE or AFTER the result cap from ADR-0050**. Recommended
   BEFORE: the cap counts post-filter records, so a high-volume INFO storm
   does not eat the cap budget; a strict `<min_severity_param>=ERROR`
   delivers all matching records up to the cap. DESIGN confirms or inverts;
   the "filter applies BEFORE the result cap" UAT scenario above encodes
   the recommendation and is rewritten by DESIGN if inverted.
4. **ADR-0052 vs refinement of ADR-0047**. Likely a small ADR-0052 (the
   read-side log API contract grows a new optional parameter; ADR-0047
   is immutable). DESIGN decides whether to author ADR-0052 or to leave
   the addition in code with a wave-decisions reference back to ADR-0047.
