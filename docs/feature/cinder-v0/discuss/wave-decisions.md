# Cinder v0 — DISCUSS wave decisions

## Key decisions

- **[D1] Port + one adapter at v0**, mirroring every prior
  v0 cut. The S3 + OpenDAL + Iceberg substrate lands at v1.

- **[D2] Cinder stores tier *metadata*, not payloads**. The
  storage engines (Lumen / Pulse / Ray / Strata) own the
  payloads. Cinder records `(tenant, item_id) → (tier,
  placed_at, migrated_at)`. This separation buys Cinder the
  freedom to evolve the tier policy without entangling
  itself in each engine's storage substrate.

- **[D3] Three tiers at v0**: `Hot`, `Warm`, `Cold`. The
  trait does not assume a specific physical substrate
  behind each tier; v1 wires hot to RocksDB / in-memory,
  warm to local Parquet, cold to S3-via-OpenDAL.

- **[D4] Generic `ItemId`** keyed by string. The trait
  takes a `&ItemId(String)` so any storage engine can use
  it without forcing a single id-type contract. Pulse can
  pass `format!("{}/{}", metric_name, time_bucket)`; Ray
  can pass `hex(trace_id)`; etc.

- **[D5] Tenant on every call**. Same shape as every prior
  storage engine.

- **[D6] Age-based lifecycle policy at slice 02**.
  `TierPolicy::age_based(hot_to_warm: Duration,
  warm_to_cold: Duration)` is the v0 policy shape. Other
  policies (size-based, query-rate-based, cost-based) land
  at v1.

- **[D7] Pure `evaluate_at(now, &policy)` API**, not a
  background-thread timer. The operator binary owns the
  timer at v1; Cinder's job is the pure evaluator. This
  keeps the crate testable in milliseconds rather than
  requiring `tokio::time::sleep`-style integration tests.

- **[D8] Migration is forward-only by default**. Items move
  `Hot → Warm → Cold` automatically; the manual `migrate`
  call honours any direction (so v1's "rehydrate from cold"
  flow works). v0 has no automatic rehydrate.

- **[D9] `MetricsRecorder` seam carries forward verbatim**.
  `record_place`, `record_migrate`, `record_evaluate`.

- **[D10] In-memory only at v0**. Restart loses tier
  metadata.

- **[D11] AGPL-3.0-or-later**.

- **[D12] Two carpaccio slices in one implementation
  commit** per established precedent.

## Slicing

- **Slice 01 — walking skeleton** (US-CI-01). Trait +
  adapter + place + get_tier + migrate + list_by_tier +
  KPI 1.
- **Slice 02 — age-based lifecycle** (US-CI-02).
  `TierPolicy` + `evaluate_at(now, &policy)` + KPI 2.

## Constraints established

- v1 disk-backed adapter must be drop-in compatible.
- The four storage engines remain free of Cinder dependency
  at v0; they will adopt the tier-lookup pattern at v1
  alongside their own durable adapters.
- Cinder depends on `aegis` (for `TenantId`) only.

## DESIGN handoff

DESIGN collapses into the implementation commit.
