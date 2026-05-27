<!-- markdownlint-disable MD024 -->

# User Stories: honest-read-caps-v0

British English. No em dashes. No emoji.

This feature is M-2 from the residuality analysis: per-request window
cap and result-size cap on the three read APIs (`query-api`,
`log-query-api`, `trace-query-api`). The current handlers
(`crates/query-api/src/lib.rs:146`, `crates/log-query-api/src/lib.rs:104`,
`crates/trace-query-api/src/lib.rs:115`) parse `start` / `end` and
reject non-numeric or inverted bounds, but impose NO upper bound on
`end - start`, and serialise WHATEVER the store returns regardless of
size. A year-long window, or a one-million-row response, traverses
the store and (best case) saturates the listener for the duration of
the read; worst case (cardinality bomb at the read side, per S04 /
S14) drives the process to OOM.

This feature closes that gap for ALL THREE read APIs in ONE slice
because the cap pattern is the same shape even though each crate
keeps its own `TimeRange` type. The After line of each story names a
real HTTP endpoint as the entry point; the observable output is the
400 body
`{status:"error", error:"<names the breached cap>"}` returned BEFORE
the store is touched (window cap) or BEFORE serialisation (result
cap).

## System Constraints

- The caps ride OUTSIDE the storage traits. `pulse::MetricStore`,
  `lumen::LogStore`, `ray::TraceStore` signatures and their other
  callers are UNCHANGED. The cap checks live in the handler, exactly
  where `parse_time_range` already produces 400s today.
- The error envelope is the EXISTING shape:
  `{status:"error", error:"<reason>"}`. No new envelope, no new
  status code (always 400), no `X-Truncated` header, no new event
  name. The cap is its own signal.
- The error text NEVER echoes the raw `start`, the raw `end`, the
  raw query (in `query-api`), the raw pattern, the raw `service` (in
  `trace-query-api`), or any forwarded Authorization / SECRET /
  Bearer value. Redaction is symmetric with the existing
  `the_bounds_error_never_echoes_the_raw_value` tests in each crate.
- Slice 01 caps are COMPILE-TIME CONSTANTS per crate. Env-driven
  configurability (e.g. `KALEIDOSCOPE_QUERY_MAX_WINDOW_SECONDS`,
  `_MAX_RESULTS`) is explicitly DEFERRED. A future slice or
  successor feature lifts them to env-driven.
- The result-size cap is enforced AFTER the store query and BEFORE
  serialisation, inside the handler. NO store-trait method is added
  for "limit". The duplication across the three crates is the
  deliberate cost the deferred `query-http-common` extraction
  (M-5 / ADR-0048 Decision 5) is the eventual home for.
- The walking skeleton uses ONE window cap value and ONE result cap
  value across all three crates. Per-pillar tuning is a DESIGN
  refinement, not a DISCUSS one.
- FLAGGED to DESIGN, NOT decided here:
  (1) the exact window cap value (e.g. 6h / 24h / 7d);
  (2) the exact result-size cap value (e.g. 10k / 100k / 1M);
  (3) REFUSE vs TRUNCATE on result cap breach (LIKELY recommendation:
      REFUSE with 400);
  (4) whether a new ADR (likely ADR-0050) records the cap policy as
      a cross-cutting refinement or whether each of ADR-0042 / 0047 /
      0048 is amended individually (LIKELY recommendation: one new
      ADR-0050).
- OUT of scope for slice 01 (deferred and declared): env-driven cap
  configurability; telemetry / metrics / new event on refusals;
  changes to the tower `oneshot` test pattern; any renegotiation
  with Prism (Prism receives an honest 400 with the same envelope
  shape its `isPromError` already handles for matcher errors);
  `query-http-common` shared extraction; caps on parameters other
  than window-span and result-size (no cap on matcher count, no cap
  on `service` length, no cap on regex complexity beyond the ReDoS
  residue already secured by ADR-0046).

---

## US-01: A year-long window to /api/v1/query_range is refused with a named 400 before pulse is touched

### Elevator Pitch

- Before: Maya Kowalski operates the platform for tenant "acme-prod"
  and a misconfigured Grafana dashboard hits
  `GET /api/v1/query_range?query=cpu_seconds_total&start=0&end=31536000`
  (a one-year span starting at the epoch). The current handler at
  `crates/query-api/src/lib.rs:146` parses the bounds cleanly, calls
  `store.query(&tenant, &name, range)`, and the pulse store walks
  every series-key under that metric name across a year of points.
  Best case: the listener saturates for the duration. Worst case
  (S04 / S14 amplification): the process OOMs.
- After: the same request to the same endpoint returns 400 with
  `{status:"error", error:"window exceeds <N> seconds"}` (or the
  equivalent named reason from DESIGN), BEFORE
  `state.store.query(...)` is called. Observable at the real
  endpoint via the tower `oneshot` acceptance pattern already in
  use; the test asserts both the 400 body AND that the lying store's
  `query` was never called. Prism's `isPromError` handles the
  envelope already (ADR-0042 lines 220-229).
- Decision enabled: Maya re-narrows the dashboard's range to within
  the cap; the platform serves the narrower request normally; the
  self-DoS surface S13 is closed for metrics.

### Problem

Maya Kowalski runs Kaleidoscope's `query-api` binary as the read
plane for tenant "acme-prod"'s metrics. A new dashboard in their
Grafana setup, hand-edited from another team's template, sends a
query with `start=0` (the Unix epoch) and `end` set to "now", which
is roughly `1_716_900_000` (mid-2024) in epoch seconds. The current
handler's `parse_time_range` (`crates/query-api/src/lib.rs:201`)
accepts non-negative numeric bounds with `end > start` and converts
them to nanoseconds, no upper bound. The store then walks roughly
54 years of series-points for the matching metric, allocating
matched `(metric, point)` pairs in memory before `matrix::to_matrix`
collapses them. The residuality analysis (S13 row of the incidence
matrix; "the **S13 columns QM / QL / QT** all degrade" under "Notable
cells") flags this exact path as the read-side self-DoS surface.
Maya cannot tell from a dropped request whether the cause was a
network blip, a store outage, or a query she should have written
differently; she needs the platform to refuse out loud and name what
she breached.

### Who

- Maya Kowalski - platform operator for "acme-prod" - runs the
  `query-api` binary, reads its logs, and answers when Grafana
  alerts go red. Needs the platform to refuse a wrong-sized query
  by name, not by timeout or process death.
- Hands-off Hannah - a Grafana dashboard author at "acme-prod" who
  rarely sees the platform binary itself; she sees Prism (or
  Grafana) showing "error: window exceeds N seconds" and knows to
  narrow her range.
- A future misconfigured client (or an attacker probing) - the
  feature must refuse predictably even when the request was not made
  in good faith; the 400 envelope is exactly what the residuality
  analysis A-D6 "honest three-way outcomes on read" guarantees.

### Solution

Add a compile-time constant `MAX_WINDOW_SECONDS` to `query-api` (the
exact value is FLAGGED to DESIGN; see `wave-decisions.md` FLAG 1).
After `parse_time_range` succeeds and BEFORE the call to
`state.store.query(...)`, compute `end_secs - start_secs` and, if
greater than the cap, return 400 with
`{status:"error", error:"window exceeds <N> seconds"}` via the
existing `error_response` helper at `crates/query-api/src/lib.rs:249`.
The reason names the cap but never echoes the raw `start`, the raw
`end`, the raw query, or any forwarded header value (redaction
symmetric with the existing
`the_bounds_error_never_echoes_the_raw_value` test at line 303).

The cap rides in the handler, mirroring the place the existing
inverted-bounds 400 lives. No change to `pulse::MetricStore`. No
change to `parse_time_range` beyond a follow-up call to the cap
check.

### Domain Examples

#### 1: Happy path - a 1-hour window passes the cap and is served normally

Maya's well-behaved dashboard sends
`GET /api/v1/query_range?query=cpu_seconds_total{job="api"}&start=1716896400&end=1716900000`
(a one-hour window ending at "now"). The handler parses the bounds,
computes `end - start = 3600`, which is below the configured cap,
and proceeds into `state.store.query(...)`. The store returns the
matching rows, `matrix::to_matrix` translates them, the response is
200 with the `success` envelope. The cap is invisible on well-formed
queries.

#### 2: Refuse - a 1-year window is refused BEFORE the store is touched

Hands-off Hannah's hand-edited dashboard sends
`GET /api/v1/query_range?query=cpu_seconds_total&start=0&end=31536000`
to the same endpoint. The handler parses the bounds, computes
`end - start = 31_536_000` (one year), which exceeds the configured
cap. The handler returns 400 with
`{status:"error", error:"window exceeds <N> seconds"}` via the
existing `error_response` helper. The store's `query` method is
NEVER called (asserted by an acceptance test that wires a lying
store whose `query` always returns `PersistenceFailed`; if `query`
had been called, the response would be the 500 from
`crates/query-api/src/lib.rs:189`, not the 400 cap arm).

#### 3: Boundary - the boundary tick is included, the next tick is refused

The window cap is half-open in the same sense the existing
inverted-bounds check is: `end - start <= MAX_WINDOW_SECONDS` is
allowed; `end - start > MAX_WINDOW_SECONDS` is refused. A request
with `end - start = MAX_WINDOW_SECONDS` exactly is served (it is the
boundary tick of "still within the cap"); a request with
`end - start = MAX_WINDOW_SECONDS + 1` is refused with the same
named 400. This boundary kills a `<=` -> `<` mutant on the cap check
the way the existing `equal_bounds_are_accepted_as_an_empty_half_open_range`
test kills the inversion-check mutant
(`crates/query-api/src/lib.rs:267`).

### UAT Scenarios (BDD)

#### Scenario: A query_range request inside the window cap is served normally

```gherkin
Given the query-api binary configured for tenant "acme-prod" with a window cap of N seconds (DESIGN-chosen)
And a real FileBackedMetricStore seeded with one matching series for `cpu_seconds_total{job="api"}`
When Maya sends GET /api/v1/query_range?query=cpu_seconds_total{job=\"api\"}&start=1716896400&end=1716900000
Then the response status is 200
And the response body has shape {status:"success", data:{resultType:"matrix", result: [...]}}
And the store was queried exactly once
```

#### Scenario: A query_range request beyond the window cap is refused before the store is touched

```gherkin
Given the query-api binary configured for tenant "acme-prod" with a window cap of N seconds
And a LyingMetricStore whose `query` method always returns `PersistenceFailed`
When the request is GET /api/v1/query_range?query=cpu_seconds_total&start=0&end=31536000
Then the response status is 400
And the response body has shape {status:"error", error:"window exceeds N seconds"}
And the LyingMetricStore's `query` method was NEVER called
```

#### Scenario: The boundary case at exactly the cap is served, one second over is refused

```gherkin
Given the query-api binary configured for tenant "acme-prod" with a window cap of N seconds
When Maya sends a query_range with `end - start = N`
Then the response status is 200 (the boundary tick is within the cap)
And when Maya sends a query_range with `end - start = N + 1`
Then the response status is 400 with `{status:"error", error:"window exceeds N seconds"}`
```

#### Scenario: The cap 400 body never echoes the raw window values or a forwarded header

```gherkin
Given the query-api binary configured for tenant "acme-prod" with a window cap of N seconds
When Maya sends a query_range with `start=0&end=31536000` and a forwarded `Authorization: Bearer SECRET-VALUE` header
Then the response status is 400
And the response body does NOT contain the literal string "31536000"
And the response body does NOT contain the literal string "SECRET-VALUE"
And the response body does NOT contain the literal string "Bearer"
And the response body does NOT contain the raw query text
```

#### Scenario: The pulse MetricStore trait signature is unchanged

```gherkin
Given the workspace as of slice 01 of this feature
When the public-api diff is computed against the prior tag for the `pulse` crate
Then `MetricStore`'s trait signature is byte-identical to the prior tag
And no method is added, removed, or re-signed on the store trait
```

### Acceptance Criteria

- [ ] A query_range request with `end - start <= MAX_WINDOW_SECONDS` is served normally (Scenario 1).
- [ ] A query_range request with `end - start > MAX_WINDOW_SECONDS` is refused with 400 and the named envelope, before the store is touched (Scenario 2).
- [ ] The boundary at exactly the cap is included; one second over is refused (Scenario 3).
- [ ] The cap 400 body never contains the raw `start`, `end`, query text, or any forwarded header value (Scenario 4).
- [ ] `pulse::MetricStore` trait signature is unchanged (Scenario 5).

### Outcome KPIs

- **Who**: an operator (or a Prism dashboard) of the `query-api` binary at tenant "acme-prod".
- **Does what**: receives a named 400 instead of a slow or OOM-killing response for any `query_range` request with `end - start > MAX_WINDOW_SECONDS`.
- **By how much**: 100 percent of over-window requests in the acceptance suite (Scenario 2, Scenario 3 second part) return the named 400 with no store call; 100 percent of within-cap requests (Scenarios 1, 3 first part) succeed.
- **Measured by**: the slice-01 acceptance suite outcomes on `query-api`, plus 100 percent mutation kill on the changed files (ADR-0005 Gate 5).
- **Baseline**: 0 percent today. The handler at `crates/query-api/src/lib.rs:146` calls `state.store.query(...)` for any well-formed window, no upper bound.

### Technical Notes (DESIGN-flagged, NOT decided here)

- The exact value of `MAX_WINDOW_SECONDS` is FLAGGED to DESIGN
  (`wave-decisions.md` FLAG 1). DISCUSS recommends ONE value across
  the three crates for slice 01.
- The cap lives in the `query-api` handler, between
  `parse_time_range` and `state.store.query(...)` (i.e. between
  lines 165 and 180 of `crates/query-api/src/lib.rs` as it stands).
- The redaction precedent is the existing
  `the_bounds_error_never_echoes_the_raw_value` test at line 303.
- Dependencies: none beyond the existing
  `crates/query-api/src/lib.rs` shape and the existing
  `error_response` helper.

---

## US-02: A year-long window to /api/v1/logs is refused with a named 400 before lumen is touched

### Elevator Pitch

- Before: Maya's incident-response colleague Idris Mbeki sends a
  `GET /api/v1/logs?start=0&end=31536000` to the `log-query-api`
  binary while trying to find an old issue. The current handler at
  `crates/log-query-api/src/lib.rs:104` parses the bounds cleanly
  (the comment at line 116 even names the place a malformed window
  is refused before the store), calls
  `state.store.query(&tenant, range)`, and the lumen store reads
  every `LogRecord` in a year of WAL across all sources for the
  tenant. The response either takes minutes or never arrives.
- After: the same request to the same endpoint returns 400 with
  `{status:"error", error:"window exceeds <N> seconds"}` via the
  existing `error_response` helper at line 183. Observable at the
  real endpoint via the tower `oneshot` acceptance pattern already
  in use in `log-query-api`.
- Decision enabled: Idris re-narrows the window to within the cap;
  the platform serves the narrower request normally; the S13
  surface is closed for logs.

### Problem

Idris Mbeki, on-call for "acme-prod", queries
`GET /api/v1/logs?start=...&end=...` while looking for the trail of
a regression that may have started weeks ago. He picks a wide range
to be safe. The current handler accepts any non-negative numeric
window with `end >= start`. The lumen store walks the WAL for the
tenant, deserialises every `LogRecord`, filters by the window, and
returns the matching set. With a year-wide window on a chatty
tenant, this is hundreds of megabytes to several gigabytes of JSON.
Idris has no way to know from a stalled curl whether the platform
is degraded, the volume is too wide, or something else; he needs
the platform to refuse with a named reason at the wire.

### Who

- Idris Mbeki - on-call SRE for "acme-prod" - sends ad hoc `logs`
  queries from the command line; needs a named 400 when he asked
  for too much, not a hung connection.
- Maya Kowalski - same as US-01, but reading the `log-query-api`
  binary's logs.
- A misconfigured operator script - automated jobs that sweep over
  long ranges; the named 400 is the script's hook for "narrow and
  retry".

### Solution

Add a compile-time constant `MAX_WINDOW_SECONDS` to `log-query-api`
with the SAME value as `query-api` for slice 01 (DESIGN may differ
them later). After `parse_time_range` succeeds at
`crates/log-query-api/src/lib.rs:118` and BEFORE the call to
`state.store.query(...)` at line 123, compute the window span and,
if it exceeds the cap, return 400 with the named envelope via
`error_response` at line 183. Redaction symmetry: the body never
echoes `start`, `end`, the raw forwarded Authorization, or
credentials.

### Domain Examples

#### 1: Happy path - a 1-hour window of logs is served as a bare JSON array

Idris sends
`GET /api/v1/logs?start=1716896400&end=1716900000` to the
`log-query-api`. The handler parses the bounds, the span is below
the cap, the store query returns the in-window `LogRecord`s in
ascending `observed_time_unix_nano` order, the body is the bare JSON
array as ADR-0047 Decision 1 pins.

#### 2: Refuse - a 1-year window of logs is refused with the named 400

Idris's wide query
`GET /api/v1/logs?start=0&end=31536000` is refused: the handler
computes `end - start = 31_536_000` (one year), notices it exceeds
the cap, returns 400 with the existing envelope BEFORE the lumen
store is touched. A `LyingLogStore` (mirroring the one already
present at `crates/log-query-api/src/composition.rs:97`) is wired
into the test; the test asserts that the lying store's `query` was
NOT called.

#### 3: Boundary - exactly at the cap is served, one second over is refused

A logs query with `end - start = MAX_WINDOW_SECONDS` is served
normally; one second wider is refused. Mirrors the boundary kill in
US-01.

### UAT Scenarios (BDD)

#### Scenario: A logs request inside the window cap is served as a bare JSON array

```gherkin
Given the log-query-api binary configured for tenant "acme-prod" with a window cap of N seconds
And a real FileBackedLogStore seeded with two in-window LogRecords
When Idris sends GET /api/v1/logs?start=1716896400&end=1716900000
Then the response status is 200
And the response body is a bare JSON array of two LogRecords in ascending observed-time order
And the store was queried exactly once
```

#### Scenario: A logs request beyond the window cap is refused before lumen is touched

```gherkin
Given the log-query-api binary configured for tenant "acme-prod" with a window cap of N seconds
And a LyingLogStore whose `query` method always returns `PersistenceFailed`
When the request is GET /api/v1/logs?start=0&end=31536000
Then the response status is 400
And the response body has shape {status:"error", error:"window exceeds N seconds"}
And the LyingLogStore's `query` method was NEVER called
```

#### Scenario: The boundary at exactly the cap is served, one second over is refused

```gherkin
Given the log-query-api binary configured for tenant "acme-prod" with a window cap of N seconds
When Idris sends a logs request with `end - start = N`
Then the response status is 200
And when Idris sends a logs request with `end - start = N + 1`
Then the response status is 400 with `{status:"error", error:"window exceeds N seconds"}`
```

#### Scenario: The cap 400 body never echoes the raw window values or a forwarded header

```gherkin
Given the log-query-api binary configured for tenant "acme-prod" with a window cap of N seconds
When Idris sends GET /api/v1/logs?start=0&end=31536000 with `Authorization: Bearer SECRET-VALUE`
Then the response status is 400
And the response body does NOT contain "31536000"
And the response body does NOT contain "SECRET-VALUE"
And the response body does NOT contain "Bearer"
```

### Acceptance Criteria

- [ ] A logs request within the cap is served as a bare JSON array (Scenario 1).
- [ ] A logs request beyond the cap is refused with 400 before the store is touched (Scenario 2).
- [ ] The cap boundary is at exactly `MAX_WINDOW_SECONDS` inclusive (Scenario 3).
- [ ] The cap 400 body never echoes raw window values or a forwarded header value (Scenario 4).

### Outcome KPIs

- **Who**: an on-call SRE (or automated script) querying `log-query-api` at tenant "acme-prod".
- **Does what**: receives a named 400 instead of a stalled or OOM-killing response for any logs request with `end - start > MAX_WINDOW_SECONDS`.
- **By how much**: 100 percent of over-window logs requests in the acceptance suite return the named 400 with no store call; 100 percent of within-cap requests succeed.
- **Measured by**: the slice-01 acceptance suite outcomes on `log-query-api`, plus 100 percent mutation kill on changed files (ADR-0005 Gate 5).
- **Baseline**: 0 percent today. The handler at `crates/log-query-api/src/lib.rs:104` calls `state.store.query(...)` for any well-formed window.

### Technical Notes (DESIGN-flagged, NOT decided here)

- Same cap value as US-01 for slice 01 (FLAG 1).
- The cap lives between `parse_time_range` and `state.store.query(...)`
  in the handler, mirroring US-01.
- Redaction precedent: the existing
  `the_bounds_error_never_echoes_the_raw_value` test at
  `crates/log-query-api/src/lib.rs:244`.
- Dependencies: none beyond the existing handler shape and the
  existing `error_response` helper.

---

## US-03: A year-long window to /api/v1/traces is refused with a named 400 even though service is already validated

### Elevator Pitch

- Before: Idris also queries
  `GET /api/v1/traces?service=checkout&start=0&end=31536000` while
  hunting the same regression in traces. The current handler at
  `crates/trace-query-api/src/lib.rs:115` validates the `service`
  parameter at line 133 (a sibling 400 the residuality analysis
  named as the closest precedent), then parses the bounds at line
  140, then calls `state.store.query(&tenant, &service, range)` at
  line 145. Even with `service=checkout` narrowing the fan-out, a
  year-wide window over a chatty service is megabytes-to-gigabytes
  of spans.
- After: the same request to the same endpoint returns 400 with
  `{status:"error", error:"window exceeds <N> seconds"}`. The cap
  check is the NEXT gate after the service-required 400 already in
  place; the `service` validation runs first, then the window
  validation, then the cap. Observable at the real endpoint via the
  tower `oneshot` acceptance pattern.
- Decision enabled: the operator narrows the window OR adds a
  narrower service; the platform serves the narrower request
  normally; the S13 surface is closed for traces.

### Problem

The trace pillar's read API already has the strictest validation of
the three: `service` is required (ADR-0048 Decision 1) and validated
at `crates/trace-query-api/src/lib.rs:133` BEFORE the store is
touched, with stricter redaction (the error body must contain
neither "SECRET" nor "Bearer", and never the raw `service`). What it
lacks is the window cap. A year-long window with a narrow `service`
is still a problem: even per-service, a chatty `checkout` over a year
yields a span volume that overruns the read path. The residuality
analysis (S13 column QT in the incidence matrix) flags this. The
fix is the same shape as US-01 / US-02 but slots in at the spot the
sibling pattern (the required-service 400) has already prepared.

### Who

- Idris Mbeki - same persona; trace-side of the same incident.
- A future Prism trace panel - once Prism gains a `/traces` view
  (ADR-0048 mentions it as future), the panel needs the same honest
  400 the metrics panel already handles for matcher errors.
- A misconfigured operator script - same as US-02.

### Solution

Add a compile-time constant `MAX_WINDOW_SECONDS` to
`trace-query-api` with the SAME value as `query-api` and
`log-query-api` for slice 01. After `parse_time_range` succeeds at
`crates/trace-query-api/src/lib.rs:140` and BEFORE the call to
`state.store.query(...)` at line 145, compute the window span and,
if it exceeds the cap, return 400 with the named envelope via
`error_response` at line 223. The cap-check 400 must honour the
STRICTER redaction posture of `trace-query-api`: no "SECRET", no
"Bearer", no raw `service`, no raw window values.

### Domain Examples

#### 1: Happy path - a 1-hour window with service=checkout is served as a bare JSON array

Idris sends
`GET /api/v1/traces?service=checkout&start=1716896400&end=1716900000`.
The handler validates `service`, parses the bounds, the span is
below the cap, the store returns the in-window spans for
`(tenant, "checkout")` in ascending `start_time_unix_nano` order.
The body is the bare JSON array per ADR-0048 Decision 2.

#### 2: Refuse - a 1-year window with service=checkout is refused with the named 400

Idris's wide query
`GET /api/v1/traces?service=checkout&start=0&end=31536000` is
refused: the handler validates `service` (passes), parses the bounds
(passes), notices the span exceeds the cap, returns 400 with the
envelope. A `LyingTraceStore` (mirroring the one already at
`crates/trace-query-api/src/composition.rs:106`) is wired; the test
asserts the lying store's `query` was NOT called.

#### 3: Boundary - exactly at the cap is served; missing service still trumps an over-window window

Two boundary cases:

- A traces request with `end - start = MAX_WINDOW_SECONDS` exactly
  and a valid `service` is served. One second wider is refused
  with the cap 400.
- A traces request with `end - start > MAX_WINDOW_SECONDS` AND no
  `service` parameter still returns the EXISTING service-required
  400 (not the new cap 400), because `service` is validated
  BEFORE the window cap in the existing handler order
  (`crates/trace-query-api/src/lib.rs:131-143`). The order is
  preserved on purpose; the user fixes the most-required-field
  failure first.

### UAT Scenarios (BDD)

#### Scenario: A traces request with service and a window inside the cap is served as a bare JSON array

```gherkin
Given the trace-query-api binary configured for tenant "acme-prod" with a window cap of N seconds
And a real FileBackedTraceStore seeded with two in-window Spans for service "checkout"
When Idris sends GET /api/v1/traces?service=checkout&start=1716896400&end=1716900000
Then the response status is 200
And the response body is a bare JSON array of two Spans in ascending start-time order
And the store was queried exactly once
```

#### Scenario: A traces request with service and a window beyond the cap is refused before ray is touched

```gherkin
Given the trace-query-api binary configured for tenant "acme-prod" with a window cap of N seconds
And a LyingTraceStore whose `query` method always returns `PersistenceFailed`
When the request is GET /api/v1/traces?service=checkout&start=0&end=31536000
Then the response status is 400
And the response body has shape {status:"error", error:"window exceeds N seconds"}
And the LyingTraceStore's `query` method was NEVER called
```

#### Scenario: The missing-service 400 still fires first, even on an over-window window

```gherkin
Given the trace-query-api binary configured for tenant "acme-prod" with a window cap of N seconds
When the request is GET /api/v1/traces?start=0&end=31536000 (no `service`)
Then the response status is 400
And the response body has shape {status:"error", error:"invalid request: service is required"}
And NOT the cap 400 reason
```

#### Scenario: The cap 400 body never echoes raw window values, the raw service, or a forwarded header

```gherkin
Given the trace-query-api binary configured for tenant "acme-prod" with a window cap of N seconds
When Idris sends GET /api/v1/traces?service=checkout-with-secret-shape&start=0&end=31536000 with `Authorization: Bearer SECRET-VALUE`
Then the response status is 400
And the response body does NOT contain "31536000"
And the response body does NOT contain "checkout-with-secret-shape"
And the response body does NOT contain "SECRET-VALUE"
And the response body does NOT contain "Bearer"
And the response body does NOT contain "SECRET"
```

### Acceptance Criteria

- [ ] A traces request with valid `service` and within-cap window is served as a bare JSON array (Scenario 1).
- [ ] A traces request with valid `service` and over-cap window is refused with 400 before the store is touched (Scenario 2).
- [ ] The existing missing-service 400 fires BEFORE the new cap 400 (Scenario 3); handler order is preserved.
- [ ] The cap 400 body never echoes raw window values, raw `service`, "SECRET", "Bearer", or a forwarded header value (Scenario 4). Stricter than `query-api` and `log-query-api`.

### Outcome KPIs

- **Who**: an on-call SRE (or Prism trace panel) querying `trace-query-api` at tenant "acme-prod".
- **Does what**: receives a named 400 instead of a stalled or OOM-killing response for any traces request with `end - start > MAX_WINDOW_SECONDS`.
- **By how much**: 100 percent of over-window traces requests in the acceptance suite return the named 400; the existing missing-service 400 still fires for the missing-service case.
- **Measured by**: the slice-01 acceptance suite on `trace-query-api`; 100 percent mutation kill on changed files.
- **Baseline**: 0 percent today. The handler at `crates/trace-query-api/src/lib.rs:115` calls `state.store.query(...)` for any well-formed `(service, window)` pair.

### Technical Notes (DESIGN-flagged, NOT decided here)

- Same cap value as US-01 / US-02 for slice 01 (FLAG 1).
- The cap lives between `parse_time_range` and `state.store.query(...)`,
  AFTER the existing missing/empty-service 400 at line 133.
- Redaction precedent: the existing
  `the_service_error_never_echoes_the_raw_service_value_or_a_credential`
  test at `crates/trace-query-api/src/lib.rs:334`. The cap 400
  inherits the stricter posture: body must contain neither "SECRET"
  nor "Bearer", and never the raw `service`.
- Dependencies: none beyond the existing handler shape and the
  existing `error_response` helper.

---

## US-04: A response that would exceed the result-size cap is refused with a named 400, not silently truncated

### Elevator Pitch

- Before: a within-window query whose store result is enormous (the
  same chatty `cpu_seconds_total` over a window inside the cap, or
  the same chatty `checkout` service whose spans saturate the
  window) is serialised in full. `matrix::to_matrix(rows)`,
  `Json(records)`, `Json(spans)` allocate the whole array in memory.
  The S04 / S14 amplification at the read side (cardinality bomb,
  per-series fan-out, narrow-window-wide-fan-out) is exactly the
  failure mode the residuality analysis named "D fan-out cost
  (0045 Consequences)".
- After: the handler, AFTER the store query but BEFORE
  serialisation, checks the result size against a compile-time
  `MAX_RESULT_ROWS` constant. If the matrix-entry count (for
  `query-api`), the `LogRecord` count (for `log-query-api`), or the
  `Span` count (for `trace-query-api`) exceeds the cap, the handler
  returns 400 with `{status:"error", error:"result exceeds M rows"}`
  (or a named-by-pillar reason from DESIGN). NOT a truncated 200,
  NOT an `X-Truncated: true` header, NOT a silent empty. The
  client knows the query was wrong-sized.
- Decision enabled: the client narrows the matchers (`query-api`),
  the window (all three), or the `service` (`trace-query-api`); the
  next query serves; the read path's memory-pressure surface is
  bounded.

### Problem

The window cap (US-01, US-02, US-03) limits TIME breadth. It does
NOT limit fan-out within a tolerated window. A chatty tenant with a
high-cardinality metric (or a chatty service emitting many spans
per second) can saturate the response inside a one-hour window if
the matchers do not narrow enough. The residuality analysis
explicitly calls this out: "the **S04 / S14 column P** is the
cardinality story" and "the **S13 columns QM / QL / QT** all
degrade" overlap. The fix is a result-size cap that complements the
window cap rather than replacing it. The cap fires AFTER the store
has answered (so the platform knows the actual result size) and
BEFORE serialisation (so the cost of the cap is just an integer
comparison, not a JSON-encoding cost).

### Who

- Maya Kowalski and Idris Mbeki - both personas - need their
  oversized queries to fail loudly with a named reason rather than
  succeed slowly or fail silently.
- Hands-off Hannah - the Grafana dashboard author - needs the
  envelope Prism already handles, so the dashboard shows "error:
  result exceeds N rows" rather than a half-rendered chart.

### Solution

Add a compile-time constant `MAX_RESULT_ROWS` (or the per-pillar
named equivalent) to each of the three crates. SAME value across
the three for slice 01 (DESIGN may differ them later; FLAG 2). In
each handler, AFTER the store query and BEFORE the serialisation:

- `query-api`: count `result.len()` after `matrix::to_matrix(rows)`
  at `crates/query-api/src/lib.rs:184` and, if it exceeds the cap,
  return 400 with the named envelope via `error_response`. The
  count is on matrix entries (one per distinct series), which is
  what the user actually sees.
- `log-query-api`: count `records.len()` after
  `state.store.query(&tenant, range)` succeeds at
  `crates/log-query-api/src/lib.rs:123` and, if it exceeds the cap,
  return 400. The count is on `LogRecord`s, which is exactly the
  array length the user sees.
- `trace-query-api`: count `spans.len()` after
  `state.store.query(&tenant, &service, range)` succeeds at
  `crates/trace-query-api/src/lib.rs:145` and, if it exceeds the
  cap, return 400. The count is on `Span`s.

REFUSE with 400 is the LIKELY recommendation (FLAG 3); DESIGN owns
the alternative TRUNCATE-with-200 if it picks that instead.

### Domain Examples

#### 1: Happy path - a result of M-1 rows is served

A query whose result is exactly `MAX_RESULT_ROWS - 1` matrix entries
(or `LogRecord`s, or `Span`s) is served normally. The cap is
invisible on well-sized results.

#### 2: Refuse - a result of M+1 rows is refused with the named 400

A query whose result is `MAX_RESULT_ROWS + 1` is refused: the store
answered (so the handler knows the size), serialisation has NOT
started, the handler returns 400 with the named envelope. The
client narrows and retries.

#### 3: Boundary - exactly M rows is served, M+1 is refused

The cap is half-open like the window cap: `<= MAX_RESULT_ROWS` is
served; `> MAX_RESULT_ROWS` is refused. Kills a `<=` -> `<` mutant
on the cap-check the way the window cap kills its boundary mutant.

### UAT Scenarios (BDD)

#### Scenario: A within-cap result is served normally on each of the three endpoints

```gherkin
Given the three read APIs configured for tenant "acme-prod" with a result cap of M rows
And each store is seeded with M-1 rows / records / spans inside an in-cap window
When Maya sends a query_range, a logs request, and a traces request all within both caps
Then each response is 200 with the appropriate envelope (matrix / bare array)
And each response contains exactly M-1 rows / records / spans
```

#### Scenario: An over-cap result is refused with the named 400 on each endpoint

```gherkin
Given the three read APIs configured for tenant "acme-prod" with a result cap of M rows
And each store is seeded with M+1 rows / records / spans inside an in-cap window
When Maya sends a query_range, a logs request, and a traces request all within the window cap but with this oversize result
Then each response is 400
And each response body has shape {status:"error", error:"result exceeds M rows"}
And no truncated 200 is ever observed
And no `X-Truncated` response header is ever set
```

#### Scenario: The boundary at exactly the cap is served, one row over is refused

```gherkin
Given a read API configured with a result cap of M rows
When the store result has exactly M rows
Then the response is 200 with all M rows in the body
And when the store result has exactly M+1 rows
Then the response is 400 with `{status:"error", error:"result exceeds M rows"}`
```

#### Scenario: The result-cap check fires AFTER the store query and BEFORE serialisation

```gherkin
Given a read API configured with a result cap of M rows
And a store that records every `query` call and panics if asked to serialise more than the cap
When the request would yield M+1 rows
Then the store was queried exactly once (the cap fires AFTER the store, not before)
And no serialisation of more than M rows was attempted
And the response is the 400 cap envelope
```

#### Scenario: The window cap and the result cap interact without contradiction

```gherkin
Given a read API configured with a window cap of N seconds and a result cap of M rows
When a request has `end - start > N` AND the (hypothetical) result would have > M rows
Then the window cap 400 fires first (it runs BEFORE the store)
And the response is `{status:"error", error:"window exceeds N seconds"}`
And the result cap is NOT consulted
```

### Acceptance Criteria

- [ ] On each of the three endpoints, a within-cap result is served normally (Scenario 1).
- [ ] On each of the three endpoints, an over-cap result is refused with the named 400, never silently truncated, never `X-Truncated` (Scenario 2).
- [ ] The result-cap boundary is at exactly `MAX_RESULT_ROWS` inclusive (Scenario 3).
- [ ] The result-cap check fires AFTER the store and BEFORE serialisation; the store is queried exactly once on the over-cap case (Scenario 4).
- [ ] When both caps would fire, the window cap fires first (Scenario 5); handler order is window-cap -> store -> result-cap.

### Outcome KPIs

- **Who**: an operator / SRE / dashboard at tenant "acme-prod" querying any of the three read APIs.
- **Does what**: receives a named 400 instead of a slow or memory-pressured response when the store result would exceed `MAX_RESULT_ROWS`.
- **By how much**: 100 percent of over-result requests in the acceptance suite (Scenario 2 for each of the three crates) return the named 400 with no truncation; 100 percent of within-cap requests succeed.
- **Measured by**: the slice-01 acceptance suite outcomes across all three crates; 100 percent mutation kill on changed files (ADR-0005 Gate 5).
- **Baseline**: 0 percent today. The handlers serialise WHATEVER the store returns regardless of size.

### Technical Notes (DESIGN-flagged, NOT decided here)

- The exact value of `MAX_RESULT_ROWS` is FLAGGED to DESIGN
  (`wave-decisions.md` FLAG 2). DISCUSS recommends ONE value across
  the three crates for slice 01.
- REFUSE vs TRUNCATE is FLAGGED to DESIGN (FLAG 3). DISCUSS's
  LIKELY recommendation is REFUSE with 400.
- The cap fires AFTER the store query (not before, because the
  handler does not know the size until the store answers) and
  BEFORE serialisation (so the JSON-encoding cost is not paid for
  the rejected result).
- The exact line for `query-api` is `crates/query-api/src/lib.rs:184`
  (just before `success_response(result)`); for `log-query-api`,
  `crates/log-query-api/src/lib.rs:124` (just before
  `success_response(records)`); for `trace-query-api`,
  `crates/trace-query-api/src/lib.rs:146` (just before
  `success_response(spans)`).
- Dependencies: depends on US-01 / US-02 / US-03 establishing the
  cap pattern, the envelope, and the per-crate constant module.

---

## US-05: A cap 400 body never echoes the requested window, the raw query, the raw pattern, the raw service, or a forwarded Authorization header

### Elevator Pitch

- Before: each of the three read APIs has redaction tests today on
  the bounds-error 400
  (`crates/query-api/src/lib.rs:303`,
  `crates/log-query-api/src/lib.rs:244`,
  `crates/trace-query-api/src/lib.rs:291`) and on the service-error
  400 (`crates/trace-query-api/src/lib.rs:334`). The NEW window-cap
  400 and the NEW result-cap 400 introduce TWO new error reasons
  per crate. Without explicit redaction tests, a future tweak could
  start echoing the raw window values or a forwarded header.
- After: explicit redaction tests are added for the two NEW cap
  reasons in each of the three crates. The window-cap 400 body
  never contains the literal `start`, the literal `end`, the raw
  query (in `query-api`), the raw pattern (`query-api` regex), the
  raw `service` (`trace-query-api`), or "Bearer" / "SECRET" /
  Authorization values. The result-cap 400 body never contains the
  same set of secrets (the result cap does not have a "raw window"
  to leak, but the same forwarded-header redaction applies). The
  redaction posture stays SYMMETRIC with the existing posture in
  each crate.
- Decision enabled: a future ADR or a future patch that changes the
  cap error reason cannot regress the redaction without failing the
  redaction test; the residuality analysis's A-U3 "Header echo in
  error bodies" undesired attractor stays blocked at the read side.

### Problem

Two of the three undesired attractors the residuality analysis names
are correctness invariants the new cap 400s could violate: A-U3
"Header echo in error bodies" (an error message leaks an
Authorization value, a tenant catalogue value, or the raw query
string) and A-U4 "Fabricated empty" (a calm 200 instead of a refusal).
A-U4 is excluded by US-04 (REFUSE, not TRUNCATE, not silent empty).
A-U3 is excluded only if redaction is asserted by tests for the new
error reasons specifically. Without that, an over-window 400 could
accidentally say "your window 0-31536000 is too wide" and echo the
forwarded `Authorization: Bearer ...` header on top.

### Who

- The same operator / SRE personas - their queries may carry
  tenant tokens, sensitive query text, or service names that
  themselves leak information.
- A future security reviewer - reads the redaction tests and
  expects the same posture across all error reasons in each crate.
- Kaleidoscope itself - as the entity claiming the redaction
  posture in three ADRs (ADR-0042 Decision 9, ADR-0047 Decision 1,
  ADR-0048 Decision 1); the cap 400 must honour it.

### Solution

Each crate gains explicit redaction tests for the two new cap
reasons, mirroring the shape of the existing
`the_bounds_error_never_echoes_the_raw_value` and
`the_service_error_never_echoes_the_raw_service_value_or_a_credential`
tests. The redaction asserts: the response body does NOT contain
the raw `start`, the raw `end`, the raw query (in `query-api`), the
raw pattern (`query-api`), the raw `service` (`trace-query-api`),
"Bearer", "SECRET", or any forwarded `Authorization` value the test
sends.

`trace-query-api` retains its stricter posture: the body must also
not contain "SECRET" or "Bearer" as substrings under any
circumstance.

### Domain Examples

#### 1: Happy path - the window-cap 400 body on query-api contains no raw window values

A test sends
`GET /api/v1/query_range?query=cpu_seconds_total&start=0&end=31536000`
with `Authorization: Bearer SECRET-XYZ` and asserts the 400 body
contains none of: "31536000", "0", the raw query text "cpu_seconds_total",
"Bearer", "SECRET-XYZ".

#### 2: Stricter on traces - the cap 400 body on trace-query-api also excludes "SECRET" and "Bearer"

The traces cap 400 body, with the same forwarded header and a
`service=checkout-with-secret-shape` parameter, contains none of:
the raw window values, the raw `service` value, "SECRET", "Bearer".

#### 3: Result cap redaction - same posture, no raw forwarded header

A request that would breach the result cap, with the same forwarded
header, has the same redaction asserted in its 400 body.

### UAT Scenarios (BDD)

#### Scenario: The window-cap 400 body on query-api never echoes the raw window, the raw query, or a forwarded header

```gherkin
Given the query-api binary configured for tenant "acme-prod" with a window cap of N seconds
When the request is GET /api/v1/query_range?query=cpu_seconds_total&start=0&end=31536000 with `Authorization: Bearer SECRET-XYZ`
Then the response status is 400
And the response body does NOT contain "31536000"
And the response body does NOT contain "cpu_seconds_total"
And the response body does NOT contain "SECRET-XYZ"
And the response body does NOT contain "Bearer"
```

#### Scenario: The window-cap 400 body on log-query-api never echoes the raw window or a forwarded header

```gherkin
Given the log-query-api binary configured for tenant "acme-prod" with a window cap of N seconds
When the request is GET /api/v1/logs?start=0&end=31536000 with `Authorization: Bearer SECRET-XYZ`
Then the response status is 400
And the response body does NOT contain "31536000"
And the response body does NOT contain "SECRET-XYZ"
And the response body does NOT contain "Bearer"
```

#### Scenario: The window-cap 400 body on trace-query-api also excludes "SECRET" and "Bearer" and never echoes the raw service

```gherkin
Given the trace-query-api binary configured for tenant "acme-prod" with a window cap of N seconds
When the request is GET /api/v1/traces?service=checkout-with-secret-shape&start=0&end=31536000 with `Authorization: Bearer SECRET-XYZ`
Then the response status is 400
And the response body does NOT contain "31536000"
And the response body does NOT contain "checkout-with-secret-shape"
And the response body does NOT contain "SECRET"
And the response body does NOT contain "Bearer"
And the response body does NOT contain "SECRET-XYZ"
```

#### Scenario: The result-cap 400 body on every crate excludes the same set of secrets

```gherkin
Given any of the three read APIs configured with a result cap of M rows
When a request would breach the result cap and carries `Authorization: Bearer SECRET-XYZ`
Then the response status is 400
And the response body does NOT contain "SECRET-XYZ"
And the response body does NOT contain "Bearer"
And on trace-query-api also: does NOT contain "SECRET" and does NOT contain the raw `service`
```

### Acceptance Criteria

- [ ] The window-cap 400 body on `query-api` excludes raw window values, the raw query, the raw pattern, "Bearer", and forwarded `Authorization` values (Scenario 1).
- [ ] The window-cap 400 body on `log-query-api` excludes raw window values, "Bearer", and forwarded `Authorization` values (Scenario 2).
- [ ] The window-cap 400 body on `trace-query-api` excludes raw window values, the raw `service`, "SECRET", "Bearer", and forwarded `Authorization` values (Scenario 3); stricter than the other two.
- [ ] The result-cap 400 body on each of the three crates honours the same redaction posture as the window-cap (Scenario 4).

### Outcome KPIs

- **Who**: a security reviewer (or an automated redaction-regression test) reading the cap 400 bodies of each of the three read APIs.
- **Does what**: confirms no raw window value, no raw query, no raw pattern, no raw `service`, no "SECRET", no "Bearer", and no forwarded `Authorization` value appears in any cap 400 body.
- **By how much**: 100 percent of the new cap 400 reasons are covered by an explicit redaction test in their crate; A-U3 undesired attractor stays blocked at the read side.
- **Measured by**: the redaction-test outcomes in the slice-01 acceptance suite.
- **Baseline**: today the existing bounds-error and service-error 400s have explicit redaction tests (precedent at `crates/query-api/src/lib.rs:303`, `crates/log-query-api/src/lib.rs:244`, `crates/trace-query-api/src/lib.rs:291`, `crates/trace-query-api/src/lib.rs:334`); the new cap reasons have NONE because the new code does not exist yet.

### Technical Notes (DESIGN-flagged, NOT decided here)

- The exact wording of the cap 400 reasons is a DESIGN call. The
  story constrains the redaction posture, not the exact text. The
  ONLY constraint is the named-cap pattern
  (`{status:"error", error:"<names the cap>"}`).
- The redaction tests live in the `#[cfg(test)] mod tests` block of
  each crate's `lib.rs`, mirroring the existing tests.
- `trace-query-api` retains its stricter posture (no "SECRET", no
  "Bearer" anywhere in any error body); the new cap tests inherit
  this.
- Dependencies: depends on US-01, US-02, US-03, US-04 producing the
  cap 400 reasons whose redaction is asserted here.

---

## Story sizing summary

| Story | Scenarios | Effort | Right-sized? |
|---|---|---|---|
| US-01 (query-api window cap) | 5 | 0.25 days | Yes |
| US-02 (log-query-api window cap) | 4 | 0.25 days | Yes |
| US-03 (trace-query-api window cap) | 4 | 0.25 days | Yes |
| US-04 (result-size cap, all three) | 5 | 0.5 days | Yes |
| US-05 (redaction on cap reasons, all three) | 4 | 0.25 days | Yes |

Total slice 01: roughly 1 day across the three crates, matching the
residuality analysis's "~30 LOC per crate" estimate. All five
stories live inside slice 01 (the walking skeleton). Slice 02 (if
required) lifts the caps from compile-time constants to env-driven
configurability per crate, per the deferred "v1-roadmap" frame in
the residuality analysis. None of these stories renegotiates Prism,
adds telemetry, or touches a storage trait.
