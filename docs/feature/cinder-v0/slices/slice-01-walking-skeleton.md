# Slice 01 — `TieringStore` trait + in-memory adapter (US-CI-01)

## Goal

Ship the trait the v1 disk-backed adapter will implement, plus
one in-memory adapter that proves the contract carries place +
get_tier + migrate + list_by_tier with timestamp preservation.

## IN scope

- `TieringStore` trait
- `InMemoryTieringStore` adapter using
  `HashMap<(TenantId, ItemId), TierEntry>`
- `Tier`, `ItemId`, `TierEntry`, `MigrateError` types
- Per-tenant + per-item isolation
- KPI 1

## OUT scope

- Lifecycle policy (slice 02)
- Physical substrate (v1)
- Retention deletion (v1)

## Learning hypothesis

Disproves "a single `HashMap` keyed by `(tenant, item_id)` is
fast enough at v0 latencies even with 10 000 placed items". If
KPI 1 fails, the v1 columnar substrate is the only path
forward.

## Effort

≤1 day.
