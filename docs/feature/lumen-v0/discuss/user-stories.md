# Lumen v0 — user stories

Two LeanUX user stories with mandatory Elevator Pitches per the
nWave DISCUSS template. Personas drawn from `acme-observability`.

The principal user is **Sasha, a platform engineer** who needs
the first first-party storage engine to land behind a stable
trait, so the rest of the storage plane (Pulse, Ray, Strata) can
follow the same shape. Sasha's job at v0 is the log signal:
ingest OTLP log batches from Aperture, persist them somewhere
queryable, and answer simple time-range queries from Prism.

The secondary user is **Riley, an SRE** debugging a production
incident. Riley needs to grep yesterday's logs for `correlation_id
= 7f3a…` within ten seconds of asking the question. The current
plumbing (Aperture forwarding to an external backend) works, but
Riley wants Kaleidoscope to be self-contained for the log pillar.

System constraints (apply to every story):

1. Library at v0. Lumen ships as a Rust crate (`lumen`) exposing
   the log-store trait and one in-memory adapter. The on-disk
   Parquet + RocksDB adapters live behind the same trait at v1.
2. AGPL-3.0-or-later. Same licensing posture as every platform
   component.
3. **OTLP-shaped ingest at v0.** Lumen consumes
   `opentelemetry-proto::logs::v1::ResourceLogs` (or its Rust
   equivalent) and round-trips it to query callers without
   structural loss. The on-disk schema is a v1 concern.
4. **Per-tenant isolation.** Lumen keys every ingested record by
   `aegis::TenantId`. No cross-tenant query at v0; the trait
   takes a tenant on every call.
5. **Time-range query at v0; rich predicates at v1.** Lumen
   answers "logs for tenant X between t1 and t2" exactly. Filters
   on service, severity, body, and attributes land in slice 02
   below; full-text via Tantivy is v1.
6. **No telemetry-on-telemetry.** Lumen itself emits OTLP
   telemetry to the operator's Aperture; ingest counters and
   query latency via metric instruments. v0 ships the trait
   for this seam plus a no-op + capturing recorder, matching the
   Sluice pattern.
7. **In-memory only at v0.** No durability; a process restart
   loses ingested records. This is acceptable because Lumen v0 is
   a *port* with one adapter; durable adapters land at v1 once
   the Arrow + Parquet + DataFusion substrate is wired in.
8. **Aperture v1 retrofit is out-of-scope for Lumen v0.** Lumen
   v0 ships the crate plus its own acceptance suite. Aperture's
   exporter chain learns about Lumen at v1.

---

## US-LU-01 — Walking skeleton: ingest + query by time range

### Elevator Pitch

- **Before**: Sasha has no first-party log storage. Logs are
  forwarded by Aperture to an external Loki / Mimir / Datadog
  backend. The "we built it ourselves" claim for the log pillar
  is empty.
- **After**: run `cargo test -p lumen --test slice_01_walking_skeleton`
  → sees `test result: ok. N passed; 0 failed`. The acceptance
  test ingests an OTLP log batch, queries it back by tenant and
  time range, asserts the records round-trip byte-stable in
  insertion order.
- **Decision enabled**: Sasha can credibly claim Lumen is the
  first-party log engine even at v0 because the trait shape is
  pinned by the acceptance suite and one adapter implements it
  end-to-end. The disk-backed adapter at v1 will inherit the
  same trait.

### Acceptance criteria

- AC-1.1 — `LogStore::ingest(tenant, batch)` accepts a
  `Vec<LogRecord>` and returns `Ok(IngestReceipt { count })` on
  success.
- AC-1.2 — `LogStore::query(tenant, TimeRange { start, end })`
  returns every record whose `observed_time_unix_nano` falls
  within `[start, end)`.
- AC-1.3 — Records are returned in observed-time ascending order
  within a tenant.
- AC-1.4 — Two tenants' records are isolated: query on tenant A
  never returns tenant B's records.
- AC-1.5 — The roundtrip preserves every field on `LogRecord`
  byte-for-byte: `observed_time_unix_nano`, `severity_number`,
  `severity_text`, `body`, `attributes`, `trace_id`, `span_id`.
- AC-1.6 — Empty queries (range with no matches) return
  `Ok(Vec::new())`, not an error.

### KPI anchor

- KPI 1 (Ingest latency): p95 ≤ 1 ms per 100-record batch on the
  in-memory adapter. Lumen sits behind Aperture's exporter on
  the hot path; ingest cannot be a bottleneck.

---

## US-LU-02 — Structured query: service + severity filters

### Elevator Pitch

- **Before**: Riley can ask Lumen "logs for tenant X between t1
  and t2" but cannot narrow further. A typical production
  question is "logs for service `checkout` at severity ≥ ERROR
  in the last 5 minutes"; v0 cannot answer it.
- **After**: run `cargo test -p lumen --test slice_02_structured_query`
  → sees `test result: ok. N passed; 0 failed`. The acceptance
  test ingests a mixed batch (multiple services, multiple
  severities) and asserts that
  `query_with(tenant, range, Predicate { service, min_severity })`
  returns exactly the matching records, in observed-time order.
- **Decision enabled**: Riley can grep logs by service and
  severity from Prism's log panel (Prism v1 wires the predicate
  through the OTLP query API). Lumen exposes the contract; the
  UI lands separately.

### Acceptance criteria

- AC-2.1 — `Predicate::service(name: &str)` filters to records
  whose resource attribute `service.name == name`.
- AC-2.2 — `Predicate::min_severity(sev: SeverityNumber)` filters
  to records whose `severity_number >= sev`.
- AC-2.3 — Predicates compose: `service` + `min_severity` is the
  intersection.
- AC-2.4 — An empty predicate is equivalent to the slice-01
  range-only query.
- AC-2.5 — Predicates that match nothing return `Ok(Vec::new())`,
  not an error.

### KPI anchor

- KPI 2 (Query latency under predicate): p95 ≤ 10 ms when
  scanning 10 000 ingested records on the in-memory adapter.
  v1's columnar substrate will tighten this dramatically, but
  the v0 trait must already be observably bounded.
