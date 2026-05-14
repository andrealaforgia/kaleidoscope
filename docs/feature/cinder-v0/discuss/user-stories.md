# Cinder v0 — user stories

Two LeanUX user stories with mandatory Elevator Pitches per the
nWave DISCUSS template. Personas drawn from `acme-observability`.

The principal user is **Sasha, a platform engineer** who has
just shipped four in-memory storage engines (Lumen, Pulse,
Ray, Strata). Each one loses data on restart. The honest gap
in the storage-plane story is durability and retention.
Cinder v0 is the port-first cut of the tiering layer that
governs which physical tier each ingested item lives in —
hot, warm, or cold. Sasha's job at v0 is to ship the trait;
the S3-backed adapter via Apache OpenDAL + Iceberg manifests
lands at v1.

The secondary user is **Riley, an SRE** running a disaster-
recovery drill. Riley kills a hot-tier process and replays
the last hour from cold storage. v0 simulates this on the
in-memory adapter; v1 wires it to real S3.

System constraints (apply to every story):

1. Library at v0. Cinder ships as a Rust crate (`cinder`)
   exposing the `TieringStore` trait + one in-memory adapter.
   The v1 adapter implements the trait alongside Apache
   OpenDAL (the object port) and Iceberg-Rust (manifest
   format).
2. AGPL-3.0-or-later.
3. **Tier-agnostic at the trait boundary**. Cinder does not
   store records itself; it stores *tier metadata* — for each
   `(tenant, item_id)` it records the current tier (`Hot`,
   `Warm`, `Cold`) and the timestamp at which the item was
   placed. The storage engines (Lumen / Pulse / Ray / Strata)
   read this metadata and decide where to look for the
   actual payload. v0 keeps the metadata in-memory; v1 keeps
   it in an Iceberg manifest plus object-store metadata.
4. **Per-tenant isolation**. Cinder keys every item by
   `aegis::TenantId × ItemId`.
5. **Three tiers at v0**: `Hot`, `Warm`, `Cold`. The hot
   tier is what Lumen / Pulse / Ray / Strata currently hold
   in-memory; the warm tier is the local-Parquet tier; the
   cold tier is the object-store tier. v0 has no opinion on
   the physical substrate behind each tier.
6. **Lifecycle policy** at slice 02. `TierPolicy` decides
   when an item is eligible for migration based on its age.
   v0 ships an age-based policy with configurable thresholds.
   `Hot → Warm` after the hot threshold; `Warm → Cold` after
   the warm threshold; items never move backwards
   automatically (a manual `migrate(item, Hot)` request is
   still honoured, which v1 will use for "rehydrate" flows).
7. **No telemetry-on-telemetry**. `MetricsRecorder` seam
   carries forward verbatim from the four storage engines.
8. **In-memory only at v0**. Restart loses the tier
   metadata. v1 persists it.
9. **No retention deletion at v0**. Cinder records the tier;
   the storage engines decide whether to evict the payload.
   Cold-tier-then-delete (TTL) is v1 work.

---

## US-CI-01 — Walking skeleton: place + get_tier + migrate

### Elevator Pitch

- **Before**: Sasha has four in-memory storage engines and
  no notion of where the data is. The story "we ship
  first-party storage with tiering" is empty.
- **After**: run `cargo test -p cinder --test slice_01_walking_skeleton`
  → sees `test result: ok. N passed; 0 failed`. The
  acceptance test places an item in the hot tier, reads its
  current tier back, manually migrates it to warm, then to
  cold, asserting the tier transitions are observable and
  the placement timestamp is preserved.
- **Decision enabled**: Sasha can credibly claim Cinder
  governs the tiering even at v0. The v1 S3 + Iceberg
  adapter inherits the trait.

### Acceptance criteria

- AC-1.1 — `TieringStore::place(tenant, item, tier,
  placed_at)` records the tier and timestamp.
- AC-1.2 — `TieringStore::get_tier(tenant, item)` returns
  the current `Tier` or `None` if the item is unknown.
- AC-1.3 — `TieringStore::migrate(tenant, item, to_tier,
  migrated_at)` updates the tier and the timestamp of last
  migration.
- AC-1.4 — Two tenants' tier metadata is isolated.
- AC-1.5 — `list_by_tier(tenant, tier)` returns every item
  in that tier for the tenant.
- AC-1.6 — Unknown items return `None` from `get_tier`,
  not an error.
- AC-1.7 — `migrate` on an unknown item is a typed error
  (`MigrateError::UnknownItem`), not a silent placement.

### KPI anchor

- KPI 1 (Tier-metadata lookup): `get_tier` p95 ≤ 50 µs over
  10 000 placed items on the in-memory adapter. Cinder sits
  on every read path (storage engines consult it before
  going to disk); the lookup must be cheap.

---

## US-CI-02 — Age-based lifecycle policy

### Elevator Pitch

- **Before**: Sasha can place and migrate items manually,
  but there is no automatic policy. Production needs
  age-driven migration (hot → warm at ~1 h, warm → cold at
  ~24 h).
- **After**: run `cargo test -p cinder --test slice_02_lifecycle`
  → sees `test result: ok. N passed; 0 failed`. The
  acceptance test installs a `TierPolicy` with hot=1 h and
  warm=24 h thresholds, advances simulated time, calls
  `evaluate_at(now)`, and asserts every eligible item moved
  to its next tier.
- **Decision enabled**: Sasha sets thresholds in the
  operator config; Cinder enforces them. v1 wires the
  evaluator to a periodic timer in the operator binary.

### Acceptance criteria

- AC-2.1 — `TierPolicy::age_based(hot_to_warm, warm_to_cold)`
  builds a policy with two thresholds (`Duration`).
- AC-2.2 — `TieringStore::evaluate_at(now, &TierPolicy)`
  returns the count of items migrated.
- AC-2.3 — Items in `Hot` with `age >= hot_to_warm` move
  to `Warm`.
- AC-2.4 — Items in `Warm` with `age >= warm_to_cold` move
  to `Cold`.
- AC-2.5 — Items in `Cold` do not move automatically.
- AC-2.6 — `evaluate_at` is idempotent: a second call with
  the same `now` returns zero migrations.
- AC-2.7 — `evaluate_at` is per-tenant: each tenant's items
  evaluate independently against the same policy.

### KPI anchor

- KPI 2 (Lifecycle evaluation): `evaluate_at` p95 ≤ 5 ms
  over 10 000 placed items split across tenants. The
  evaluator runs periodically; it must not dominate the
  operator binary's CPU.
