# Strata v0 — DISCUSS wave decisions

## Key decisions

- **[D1] Port + one adapter at v0**, mirroring Sluice, Lumen,
  Pulse, Ray. The columnar substrate (Arrow + Parquet +
  DataFusion + RocksDB) plus symbolisation (gimli +
  addr2line) lands at v1.

- **[D2] pprof-shaped types at the trait boundary**. `Profile`
  field set mirrors the public `profile.proto`: sample types,
  samples (location id + value), location / function / mapping
  indices, string table. v0 round-trips every field
  byte-stable. v1 will align with the OpenTelemetry Profiles
  signal as it stabilises; the gap is documented in
  `outcome-kpis.md`.

- **[D3] Tenant + service on every call**. Same shape as
  every prior storage engine.

- **[D4] Two query shapes at v0**: by `(service.name, time
  range)` (the slice 01 query) and by `(service.name, profile
  _type)` (slice 02 — composes with time range). The
  per-service bucket model matches how Ray indexes spans by
  service; Strata reuses the pattern.

- **[D5] Single index at v0**. `HashMap<(TenantId,
  ServiceName), Vec<Profile>>` sorted by `time_unix_nano`.
  Profiles are big enough that a dual index would more than
  double memory cost; we keep the simpler shape and accept
  the linear scan when the predicate narrows by
  `profile_type`. v1's columnar substrate replaces this
  entirely.

- **[D6] No symbolisation at v0**. Producers ship symbolised
  pprof. v1 accepts raw frames + symbolises server-side.

- **[D7] No flame graph at v0**. Strata is the storage
  engine; rendering is Prism v1.

- **[D8] In-memory only at v0**.

- **[D9] No cross-pillar exemplars at v0**. v1 cross-pillar
  work.

- **[D10] `MetricsRecorder` seam carries forward verbatim**
  from Lumen + Pulse + Ray + Sluice.

- **[D11] AGPL-3.0-or-later**.

- **[D12] Two carpaccio slices in one implementation commit**
  per established precedent.

## Slicing

- **Slice 01 — walking skeleton** (US-ST-01). Trait + adapter
  + ingest + query by `(service, range)` + KPI 1.
- **Slice 02 — structured query** (US-ST-02). `Predicate` +
  `profile_type` filter + `query_with` + KPI 2.

## Constraints established

- v1 disk-backed adapter must be drop-in compatible.
- The pprof format is the v0 shape; the OpenTelemetry
  Profiles signal is still in development upstream. Strata
  tracks pprof at v0 and aligns with OTel at v1 once that
  signal stabilises.
- Strata depends on `aegis` (for `TenantId`) only.

## DESIGN handoff

DESIGN collapses into the implementation commit.
