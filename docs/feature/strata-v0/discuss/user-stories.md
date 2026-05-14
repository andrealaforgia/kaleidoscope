# Strata v0 — user stories

Two LeanUX user stories with mandatory Elevator Pitches per the
nWave DISCUSS template. Personas drawn from `acme-observability`.

The principal user is **Sasha, a platform engineer** who needs
the fourth signal pillar — profiles — to land behind the same
trait shape as Lumen / Pulse / Ray. The Phase 6 substrate
(pprof ingest, Arrow-columnar flame-graph storage, columnar
DAG layout, gimli/addr2line symbolisation) is high-cost; the
v0 cut is port-first, like every storage engine before it.

The secondary user is **Riley, an SRE** investigating a CPU
regression. Riley uploads a pprof produced by `go tool pprof
-proto` or by `pprof-rs` and asks "show me every profile for
service `checkout` between t1 and t2". v0 answers that. Flame
graph rendering and diff views live in Prism v1.

System constraints (apply to every story):

1. Library at v0. Strata ships as a Rust crate (`strata`)
   exposing the `ProfileStore` trait and one in-memory adapter.
   The v1 columnar adapter implements the trait alongside the
   Arrow / Parquet / DataFusion + RocksDB substrate plus a
   gimli + addr2line symbolisation pipeline.
2. AGPL-3.0-or-later.
3. **pprof-shaped types at the trait boundary**. `Profile`
   field set mirrors the public `profile.proto`
   ([github.com/google/pprof](https://github.com/google/pprof)):
   `sample_type` array, samples (location id + value), location
   index, function index, mapping index, string table.
   v0 round-trips every field byte-stable.
4. **Per-tenant isolation**. Keyed by `aegis::TenantId`.
5. **Two queries at v0**: by `(service.name, time range)` and
   by `(service.name, profile_type)`. Slice 02 composes them.
6. **No symbolisation at v0**. Profiles arrive symbolised by
   the producer (Go runtime, pprof-rs, perf record + perf
   script → pprof). v1 will accept raw stack frames and run
   symbolisation server-side.
7. **No flame graph rendering at v0**. Strata exposes the data;
   Prism renders the flame graph at v1.
8. **No cross-pillar exemplars at v0**. Linking Pulse points
   to Ray traces to Strata profiles is the cross-pillar work
   that comes at v1.
9. **No telemetry-on-telemetry**. `MetricsRecorder` seam
   carries forward verbatim.
10. **In-memory only at v0**. Restart loses profiles.

---

## US-ST-01 — Walking skeleton: ingest + query by (service, range)

### Elevator Pitch

- **Before**: Sasha has no first-party profile storage.
  Profiles forward to an external Pyroscope / Parca backend.
  The "we built it ourselves" claim for the profile pillar is
  empty.
- **After**: run `cargo test -p strata --test slice_01_walking_skeleton`
  → sees `test result: ok. N passed; 0 failed`. The acceptance
  test ingests a batch of pprof-shaped profiles, queries them
  back by `(service, time range)`, asserts every field round-
  trips byte-stable in ascending-time order.
- **Decision enabled**: Sasha can credibly claim Strata is
  the first-party profile engine even at v0. The v1 disk-
  backed adapter inherits the contract.

### Acceptance criteria

- AC-1.1 — `ProfileStore::ingest(tenant, batch)` accepts a
  `ProfileBatch` and returns `Ok(IngestReceipt { count })`.
- AC-1.2 — `ProfileStore::query(tenant, service, range)`
  returns every profile whose `time_unix_nano` falls within
  `[start, end)` and whose `resource_attributes["service.name"]`
  equals `service`.
- AC-1.3 — Profiles returned in ascending `time_unix_nano`
  order.
- AC-1.4 — Two tenants isolated: query on tenant A never
  returns tenant B's profiles.
- AC-1.5 — Roundtrip preserves every field on `Profile`:
  `time_unix_nano`, `duration_nanos`, `profile_type`,
  `sample_type`, `samples`, `locations`, `functions`,
  `mappings`, `string_table`, `resource_attributes`,
  `attributes`.
- AC-1.6 — `query` on unknown service returns
  `Ok(Vec::new())`, not an error.
- AC-1.7 — Empty range returns `Ok(Vec::new())`.

### KPI anchor

- KPI 1 (Ingest latency): p95 ≤ 5 ms per 10-profile batch on
  the in-memory adapter. (Profiles are bigger than spans /
  logs / metric points — kilobytes to megabytes of sample
  data each. The ceiling reflects realistic profile size,
  not pretend-they-are-cheap.)

---

## US-ST-02 — Structured query: filter by profile_type

### Elevator Pitch

- **Before**: Riley can pull every profile for service X in
  range, but cannot say "show me only the CPU profiles, not
  the heap profiles". v0 cannot answer.
- **After**: run `cargo test -p strata --test slice_02_structured_query`
  → sees `test result: ok. N passed; 0 failed`. The
  acceptance test ingests a mixed batch (CPU + heap +
  goroutine profiles for the same service) and asserts that
  `query_with(tenant, service, range, Predicate { profile_type })`
  returns exactly the matching profiles.
- **Decision enabled**: Riley narrows to the profile type
  that matters for the investigation.

### Acceptance criteria

- AC-2.1 — `Predicate::profile_type(name)` filters to
  profiles whose `profile_type == name` (e.g. `"cpu"`,
  `"heap"`, `"goroutine"`).
- AC-2.2 — Empty predicate ≡ slice-01 query.
- AC-2.3 — No matches returns `Ok(Vec::new())`.

### KPI anchor

- KPI 2 (Query latency under predicate): p95 ≤ 10 ms when
  scanning 1 000 ingested profiles on the in-memory adapter.
  (1000 not 10000 — profiles are an order of magnitude
  larger than spans, so the realistic scan corpus is
  smaller.)
