# Wave Decisions: lumen-query-api-v0 (DISCUSS)

British English throughout. No em dashes.

## Configuration (decided, not asked)

| # | Decision | Value | Implication |
|---|----------|-------|-------------|
| 1 | Feature type | Backend | HTTP read service; no TUI/web mockups; contract-first |
| 2 | Research depth | Lightweight | Primary "user" is an operator (and later prism) reading logs over HTTP; emotional arc kept brief |
| 3 | JTBD | No | No diverge artifacts; job grounded informally in the logs read-loop closure |
| 4 | Walking skeleton | Yes (read half of an existing write loop) | Lumen store, aegis tenancy, gateway write path all exist; this adds the missing read half |

## What this feature opens

Kaleidoscope stores logs durably in the `lumen` crate (`FileBackedLogStore`,
`crates/lumen/src/file_backed.rs`), and the gateway already WRITES logs via lumen.
But there is NO way to read them back: logs are written and unseen. This feature is
the HTTP read path for logs, the second observability pillar, the exact analogue of
what `query-range-api-v0` did for metrics. For metrics an operator can now query and
plot; for logs they can do nothing yet. This feature lets an operator (and, later,
prism) READ the stored logs, querying them by tenant and time window over HTTP.

## DIVERGE artifacts

None present at `docs/feature/lumen-query-api-v0/diverge/`. No prior JTBD run.
Risk noted: the job statement is grounded informally in the platform read-loop
narrative and the verified `LogStore` surface rather than a validated ODI job
analysis. Low risk: the store surface is concrete and verified, and the metrics read
path (`query-range-api-v0`, ADR-0042) is a directly analogous precedent, removing most
requirement ambiguity.

## Verified facts (stories are grounded in these, read 2026-05-22)

1. `crates/lumen/src/store.rs` defines `pub trait LogStore` with
   `query(&self, tenant: &TenantId, range: TimeRange) -> Result<Vec<LogRecord>, LogStoreError>`
   and `query_with(&self, tenant, range, predicate)`. Per-tenant isolation; ascending
   `observed_time_unix_nano` order; half-open `[start, end)` time range.
2. `FileBackedLogStore` implements `LogStore` (`crates/lumen/src/file_backed.rs:191`),
   the durable adapter; `query` is at `file_backed.rs:211`.
3. `LogRecord` (`crates/lumen/src/record.rs:44`) carries: `observed_time_unix_nano: u64`,
   `severity_number: SeverityNumber(i32)`, `severity_text: String`, `body: String`,
   `attributes: BTreeMap<String,String>`, `resource_attributes: BTreeMap<String,String>`,
   `trace_id: Option<[u8;16]>`, `span_id: Option<[u8;8]>`. Field set mirrors
   `opentelemetry-proto::logs::v1::LogRecord`.
4. `TimeRange` (`record.rs:97`) is half-open `[start_unix_nano, end_unix_nano)` in u64
   nanoseconds; `TimeRange::all()` is `[0, u64::MAX)`.
5. `LogStoreError::PersistenceFailed { reason }` is the only typed failure
   (`store.rs:44`).
6. There is NO HTTP query API for logs today: the metrics `query-api` crate does not
   reference lumen, and no log-query crate exists. Verified by Grep over the workspace.

## Key decisions taken in DISCUSS

1. **Logs are not metrics. No PromQL.** The metrics endpoint serves a Prometheus
   `matrix` (time series). Logs are records, not series; PromQL does not apply. This
   feature reads `LogRecord`s by tenant and time window. The selector grammar of the
   metrics endpoint has NO analogue here: there is no metric name and no query
   language in slice 01. Query inputs are tenant + `[start, end)` only.

2. **Read against the real durable store.** The endpoint reads from the real
   `FileBackedLogStore` via `LogStore::query(&tenant, range)`, not a fixture. This is
   the same posture the metrics read path took against the durable Pulse store.

3. **Tenant resolved fail-closed.** Lumen is per-tenant (`query(&TenantId, range)`).
   The endpoint resolves exactly one tenant per request and refuses to serve when none
   resolves, mirroring the metrics read path (ADR-0042 Decision 7) and the gateway
   write path (`KALEIDOSCOPE_DEFAULT_TENANT`, fail-closed). The mechanism is a DESIGN
   decision; DISCUSS pins the BEHAVIOUR (scoped, fail-closed), not the mechanism.

4. **Slice 01 is one thin walking skeleton.** An HTTP endpoint that, given a tenant and
   a window `[start, end]`, returns the `LogRecord`s falling in the window as JSON, read
   from the real durable lumen `LogStore`. That is the whole of slice 01.

## Flagged to DESIGN (NOT decided in DISCUSS)

These are open questions handed to the DESIGN wave (solution-architect). DISCUSS does
not choose between the options.

- **FLAG 1 (response contract)**: a Loki-shaped (Grafana) log response versus a plain
  JSON array of `LogRecord`s. Both are viable. The metrics endpoint had its response
  shape PINNED by Prism's existing client; the logs read path has NO pinned consumer
  contract yet (no prism log panel exists). DESIGN owns the choice. DISCUSS pins only
  that the in-window records are returned as JSON, faithfully carrying the `LogRecord`
  fields (verified fact 3).
- **FLAG 2 (new crate vs extend)**: whether this needs a NEW crate (e.g.
  `log-query-api` / `lumen-query-api`) or extends the existing `query-api` crate. The
  metrics `query-api` crate does not reference lumen today (verified fact 6); reuse vs
  a clean new boundary is a DESIGN trade-off. DESIGN owns the choice.
- **FLAG 3 (same-origin / static serving for prism)**: out of slice 01 entirely.
  Whether and how the endpoint same-origin-serves a future prism log UI is deferred;
  not decided here.

## Red cards (open questions for DESIGN / clarification)

- RED CARD 1 (response contract): Loki-shaped vs plain `LogRecord` JSON array. See
  FLAG 1. Owner: DESIGN.
- RED CARD 2 (placement): new crate vs extend `query-api`. See FLAG 2. Owner: DESIGN.
- RED CARD 3 (tenant supply): which mechanism resolves the tenant for a log query
  (configured single tenant vs `X-Scope-OrgID` header vs aegis Bearer token)?
  Recommended slice-01 default: configured single tenant, fail-closed (mirrors the
  metrics read path). Owner: DESIGN.
- RED CARD 4 (request shape for the window): how `start`/`end` arrive on the wire
  (epoch seconds like the metrics endpoint, RFC3339, or nanoseconds) and how they map
  to the u64-nanosecond `TimeRange`. Recommended: mirror the metrics endpoint's epoch
  seconds for operator muscle memory, converting exactly. Owner: DESIGN.

## Out of scope (deferred to later slices, DECLARED)

- Severity / level filtering (e.g. ERROR and above).
- Full-text search on the body.
- Matchers on attributes / resource attributes (regex or exact), even though
  `query_with(predicate)` exists in the store.
- Pagination / limits / ordering guarantees beyond the store's natural ascending
  `observed_time_unix_nano` order.
- Any prism UI; same-origin static serving for prism (FLAG 3).
- PromQL: logs are not metrics.

## Risk register

| Risk | Prob | Impact | Mitigation |
|------|------|--------|------------|
| Response contract churn once a prism log panel arrives | Med | Med | FLAG 1; defer the choice to DESIGN; faithfully carry LogRecord fields whatever the envelope |
| Tenant mechanism mismatch with platform | Med | High | RED CARD 3; mirror gateway/metrics fail-closed default; DESIGN decides |
| Scope creep into severity/body/attribute filtering | Med | High | Explicit OUT-of-scope list; slice 01 frozen at tenant + window |
| seconds/nanoseconds (or unit) error on the window | Med | Med | RED CARD 4; explicit conversion AC + half-open boundary example |
| Persistence failure mis-surfaced as empty | Low | High | Dedicated scenario: PersistenceFailed -> 5xx, never a fabricated empty success |
