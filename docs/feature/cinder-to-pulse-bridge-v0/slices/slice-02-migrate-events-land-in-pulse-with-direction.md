# Slice 02 — Migrate events land in Pulse with direction attributes

**Story**: US-02
**Outcome KPI**: OK2
**Tag**: `@infrastructure`
**Estimated effort**: ~2 hours
**Depends on**: Slice 01 (the bridge struct + the lowercase tier helper)

## Goal

Wire Cinder's `record_migrate(tenant, from, to)` events into Pulse as
queryable `cinder.migrate.count` metric points under per-tenant
partitions, with the migration direction carried as two point
attributes (`from`, `to`). Ensure failed migrates leave no trace.

## What ships in this slice

| Artifact | Change |
|----------|--------|
| `crates/self-observe/src/cinder_bridge.rs` | MODIFY. Replace the Slice 01 stub of `record_migrate` with a real implementation that emits `cinder.migrate.count` with the two direction attributes. |
| `crates/self-observe/tests/cinder_to_pulse.rs` | EXTEND. Slice 02 test block: 3 tests (happy path with direction attrs, failed-migrate quiescence, two-tenant isolation under opposite-direction migrations). |

No new dependencies. No `Cargo.toml` change.

## Acceptance tests for this slice

```rust
#[test]
fn cinder_migrate_produces_a_pulse_point_with_from_and_to_attributes() {
    // place acme/item Hot, migrate -> Warm, query cinder.migrate.count
    // expect 1 point, value 1.0, attrs { from: "hot", to: "warm" }
}

#[test]
fn cinder_migrate_failure_with_unknown_item_emits_no_pulse_point() {
    // migrate on never-placed item -> Err(UnknownItem)
    // expect cinder.migrate.count query returns empty Vec
}

#[test]
fn two_tenants_cinder_migrate_events_land_in_isolated_pulse_buckets() {
    // acme migrate Hot->Warm, globex migrate Hot->Cold
    // expect each tenant's query returns its own 1 point with its own attrs
}
```

## DoR satisfied for this slice

See `discuss/dor-validation.md` US-02 row.

## Out of scope for this slice

- `record_evaluate` (Slice 03).
- Validating that the test from Slice 01 still passes — that is implicit
  and verified by CI re-running the whole test file on every commit.

## Risks specific to this slice

| Risk | Mitigation |
|------|------------|
| Attribute key naming ("from"/"to" vs "src"/"dst" vs "src_tier"/"dst_tier") drifts from convention used elsewhere | Cinder's own trait names them `from: Tier, to: Tier`, so the bridge mirrors. Decision recorded in `wave-decisions.md` D1 and locked by the acceptance test's string-literal assert. |
| Cinder's failure path silently calls `record_migrate` anyway in some future v1 change | Acceptance test `cinder_migrate_failure_with_unknown_item_emits_no_pulse_point` would fail loudly. This is the test's whole purpose. |

## Definition of Done for this slice

- All Slice 01 and Slice 02 tests green.
- `cargo clippy --workspace --all-targets` clean.
