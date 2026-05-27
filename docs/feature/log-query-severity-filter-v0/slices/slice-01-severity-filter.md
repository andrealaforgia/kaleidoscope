# Slice 01 — log-query-severity-filter

Thin slice on top of `crates/log-query-api`. ONE optional query-string
parameter on `GET /api/v1/logs` that filters returned `LogRecord`s by
minimum OTel severity. Default behaviour (parameter absent) is unchanged.

## Walking-skeleton scenario (the demoable outcome)

> Tenant `acme-prod` has six log records seeded into a real durable Lumen
> store, all inside `[1716200000s, 1716200060s)`: three INFO heartbeats,
> two WARN slow-upstream notices, one ERROR payment-timeout.
> The operator runs
> `curl 'http://logs.kaleidoscope.acme.internal/api/v1/logs?start=1716200000&end=1716200060&<min_severity_param>=WARN'`.
> The response is HTTP 200 with a bare JSON array of exactly three records
> (the two WARN and the one ERROR), in ascending `observed_time_unix_nano`
> order. The three INFO records are excluded by the platform before
> serialisation. The shape of each record is identical to today's
> response.

## Learning hypothesis

> One optional parameter on the existing query path plus one OTel-name ->
> `SeverityNumber` mapping is enough to satisfy the operator's "WARN or
> worse" job, without changing the lumen trait, without changing the
> response shape, and without changing the existing caps.

The hypothesis is FALSIFIED if any of the following happens during DELIVER:

- The lumen trait grows a new method.
- A new module is added to `crates/log-query-api/src/`.
- A new error envelope or status code is introduced.
- An existing acceptance scenario in `tests/slice_01_logs_read.rs` or
  `tests/slice_02_caps.rs` is edited or deleted.
- The `MAX_WINDOW_SECONDS` or `MAX_RESULT_ROWS` constants change.
- The bare-JSON-array success shape (ADR-0047 Decision 1) changes.

Any of these means the slice is bigger than thought and should be re-scoped
under a successor feature ID, NOT under this one.

## Scope (IN)

- One optional query-string parameter on `GET /api/v1/logs`. Name pinned by
  DESIGN (FLAG 1; recommended `min_severity`).
- Accepted values: the six OTel names `TRACE`, `DEBUG`, `INFO`, `WARN`,
  `ERROR`, `FATAL`. Case-sensitivity pinned by DESIGN (FLAG 2;
  recommendation: case-insensitive, no aliases).
- The filter is `>=` on `SeverityNumber` per
  `crates/lumen/src/record.rs:32-39`.
- The filter runs via the existing `LogStore::query_with(&tenant, range,
  &predicate)` seam with
  `Predicate::new().min_severity(SeverityNumber::WARN)` (or the
  corresponding constant for the supplied name).
- An unknown or malformed severity value returns HTTP 400 with the existing
  envelope `{"status":"error","error":"unknown severity"}`. The body does
  NOT echo the raw parameter value.
- Default (parameter absent) = behaves exactly as today.

## Scope (OUT — declared, deferred)

- `severity_text` filtering.
- Severity ranges (e.g. WARN+ERROR but NOT FATAL).
- Body regex / substring filtering.
- Record-attribute filters.
- Resource-attribute filters on the HTTP boundary (e.g. `service.name`;
  lumen supports it, HTTP exposure is a separate slice).
- Aliases (`WARNING` -> `WARN`, `err` -> `ERROR`, etc.).
- Env-driven default severity floor.
- The `query-http-common` extraction (ADR-0048 Decision 5).
- Any change to `MAX_WINDOW_SECONDS` / `MAX_RESULT_ROWS`.

## Mapping to user stories

| Story | Scenario(s) in user-stories.md | Walking-skeleton role |
|---|---|---|
| US-01 | "A min-severity floor returns only records at-or-above the floor" | THE walking skeleton |
| US-02 | "Parameter absent returns every record in the window (default unchanged)" | Backward-compat contract |
| US-03 | "The min-severity boundary is inclusive (== floor passes)" and "A record one notch below the floor is excluded" | Boundary mutation kill |
| US-04 | "The filter applies BEFORE the result cap" | Cap interaction (FLAG 3) |
| US-05 | "An unknown severity name is a redacted 400" | Error-envelope reuse + redaction |

## Acceptance file (DELIVER target)

`crates/log-query-api/tests/slice_01_severity_filter.rs` — NEW. Reuses
`mod common` helpers from the existing slice test files. Follows the
established one-at-a-time outer-loop convention from
`tests/slice_01_logs_read.rs` (walking skeleton enabled first, following
scenarios `#[ignore]`'d until enabled).

## Flags to DESIGN

1. Wire parameter name (`min_severity` recommended).
2. Case-sensitivity (case-insensitive on the six OTel names, no aliases,
   recommended).
3. Filter BEFORE the result cap (recommended).
4. ADR-0052 vs refinement of ADR-0047 (small ADR-0052 recommended).

See `discuss/wave-decisions.md` § "Flags to DESIGN" for the reasoning
behind each recommendation.

## Estimated effort

1 day end-to-end:

- Parse one parameter and map six names to constants (~30 lines).
- Branch one handler arm to call `query_with` when a predicate is present
  (~10 lines).
- Five acceptance scenarios in one new test file (~150 lines, mostly
  fixture seeding via existing helpers).
- One mutation-test pass on the modified files (existing
  `gate-5-mutants-log-query-api` workflow, no new CI job).
