# Slice 01: logs walking read

## Elevator Pitch
An operator GETs the logs endpoint for a tenant over a time window `[start, end]` and
sees the in-window `LogRecord`s returned as JSON, read out of the real durable lumen
`FileBackedLogStore`. The logs read loop (ingest -> store -> query -> see) closes for
the first time, the analogue of what query-range-api-v0 did for metrics.

## Learning hypothesis
We believe an HTTP endpoint calling `LogStore::query(&tenant, range)` over the durable
lumen store will let an operator read real in-window logs, scoped fail-closed and with
honest empty/400/5xx arms. We will know this is true when an end-to-end test (ingest
in-window and out-of-window records, GET the endpoint via tower oneshot, assert only the
in-window records come back, with every field intact) passes against the real
`FileBackedLogStore`. Size: <= 1 day for the walking-skeleton happy path (US-01 + US-03
happy path); the empty and failure arms (US-02, US-04) follow within the slice.

## Stories in this slice
- US-01 Read the in-window logs for a tenant over HTTP (P1, Must)
- US-03 Scope every log query to one tenant, fail-closed (P1, Must)
- US-02 Return a calm empty result when nothing is in the window (P1, Must)
- US-04 Reject a malformed window and surface a store failure honestly (P2, Should)

## Walking-skeleton scenario
Ingest some `LogRecord`s in-window and out-of-window for tenant "acme-prod", GET the
logs endpoint for "acme-prod" over `[start, end]`, and only the in-window records come
back, in ascending `observed_time` order, with every field intact. Exercised in the
acceptance suite via the tower `oneshot` pattern against the real `FileBackedLogStore`.

## End-to-end demonstrable behaviour
A GET for the logs endpoint with a tenant and a window `[start, end]` returns the
in-window `LogRecord`s as JSON, scoped to one fail-closed tenant, with a calm empty arm
(HTTP 200), a 400 for a malformed window, and a 5xx for a store failure, the four arms
all distinct.

## Carpaccio taste tests
- Vertical (end to end): YES. Request -> tenant resolve -> window validate -> store
  query -> JSON response. A real, observable HTTP read, not a layer.
- Demonstrable in one session: YES. Ingest, GET, see the in-window records.
- Delivers user value alone: YES. The operator can read logs that were previously
  written and unseen.
- Thin (no fat): YES. No severity filter, no body search, no attribute matchers, no
  pagination. Tenant + window only.
- Independently shippable: YES. Does not depend on any deferred slice.

## Store surface (verified, do not redesign)
- `LogStore::query(&self, tenant: &TenantId, range: TimeRange) -> Result<Vec<LogRecord>, LogStoreError>`
  (`crates/lumen/src/store.rs`); durable adapter `FileBackedLogStore`
  (`crates/lumen/src/file_backed.rs:211`).
- `TimeRange` half-open `[start_unix_nano, end_unix_nano)`, u64 nanoseconds
  (`crates/lumen/src/record.rs:97`).
- `LogRecord` fields: `observed_time_unix_nano`, `severity_number`, `severity_text`,
  `body`, `attributes`, `resource_attributes`, optional `trace_id` / `span_id`
  (`crates/lumen/src/record.rs:44`).
- `LogStoreError::PersistenceFailed { reason }` is the only typed failure
  (`crates/lumen/src/store.rs:44`).
- Test-build reference: `crates/lumen/tests/v1_slice_02_snapshot.rs` shows how
  `LogRecord`s are built, seeded, and queried.

## Integration points (3)
1. HTTP client (the contract consumer; exercised via tower `oneshot` in the acceptance
   suite).
2. lumen `LogStore::query(&TenantId, TimeRange)` against the durable
   `FileBackedLogStore`.
3. aegis `TenantId` (tenant scoping vocabulary).

## Flagged to DESIGN (do NOT decide in this slice)
- FLAG 1 (response contract): Loki-shaped (Grafana) response versus a plain JSON array
  of `LogRecord`s. Flagged, not chosen.
- FLAG 2 (placement): a NEW crate (`log-query-api` / `lumen-query-api`) versus extending
  the existing `query-api` crate. Flagged, not chosen.
- FLAG 3 (same-origin / static serving for a future prism log UI): out of slice 01.
- RED CARD 3 (tenant supply): configured single tenant (recommended slice-01 default,
  fail-closed) versus `X-Scope-OrgID` header versus aegis Bearer token.
- RED CARD 4 (window on the wire): epoch seconds (recommended, mirrors the metrics
  endpoint) versus RFC3339 versus nanoseconds; convert exactly to the u64-ns `TimeRange`.

## Out of scope (deferred, declared)
Severity / level filtering; full-text body search; attribute / resource matchers (even
though `query_with(predicate)` exists); pagination / limits / ordering beyond the
store's natural ascending `observed_time` order; any prism UI; same-origin static
serving for prism; PromQL (logs are not metrics).

## KPIs targeted
North Star (KPI 1: operator reads real in-window logs), KPI 2 (every LogRecord field
round-trips, 100%), KPI 3 (p95 <= 500 ms on ubuntu-latest), KPI 4 (tenant fail-closed,
0 cross-tenant leaks).
