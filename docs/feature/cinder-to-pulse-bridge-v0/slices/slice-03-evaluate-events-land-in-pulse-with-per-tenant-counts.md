# Slice 03 — Evaluate events land in Pulse with per-tenant migrated counts

**Story**: US-03
**Outcome KPI**: OK3
**Tag**: `@infrastructure`
**Estimated effort**: ~3 hours
**Depends on**: Slices 01 + 02 (the dual-emission test cross-asserts both `cinder.migrate.count` AND `cinder.evaluate.migrated.count` from one `evaluate_at` call)

## Goal

Wire Cinder's `record_evaluate(tenant, migrated)` events into Pulse as
queryable `cinder.evaluate.migrated.count` metric points under per-
tenant partitions, with the migrated count encoded as the point value
(not as an attribute). Preserve the dual-emission contract: a single
`evaluate_at` call that migrates N items for a tenant produces N points
on `cinder.migrate.count` AND 1 point on `cinder.evaluate.migrated.count`.

## What ships in this slice

| Artifact | Change |
|----------|--------|
| `crates/self-observe/src/cinder_bridge.rs` | MODIFY. Replace the Slice 01 stub of `record_evaluate` with a real implementation that emits `cinder.evaluate.migrated.count` with value = `migrated as f64`. |
| `crates/self-observe/tests/cinder_to_pulse.rs` | EXTEND. Slice 03 test block: 3 tests (one-tenant N-migration dual emission, zero-eligible quiescence, two-tenant per-tenant split). |

No new dependencies. No `Cargo.toml` change.

## Acceptance tests for this slice

```rust
#[test]
fn cinder_evaluate_emits_per_item_migrate_points_and_one_evaluate_point() {
    // place 5 items for acme in Hot at t0
    // evaluate_at(t0 + 25h, policy threshold 24h, Hot -> Warm)
    // expect cinder.evaluate_at returns 5
    // expect cinder.migrate.count for acme = 5 points (each from=hot, to=warm)
    // expect cinder.evaluate.migrated.count for acme = 1 point, value 5.0
}

#[test]
fn cinder_evaluate_with_no_eligible_items_emits_no_evaluate_point() {
    // place 3 items for acme in Hot at t0
    // evaluate_at(t0 + 1h, policy threshold 24h) -- nothing eligible
    // expect cinder.evaluate_at returns 0
    // expect cinder.evaluate.migrated.count for acme = empty
    // expect cinder.migrate.count for acme = empty
}

#[test]
fn cinder_evaluate_across_two_tenants_emits_per_tenant_counts() {
    // place 5 items for acme in Hot at t0, 2 for globex in Hot at t0
    // evaluate_at(t0 + 25h, policy)
    // expect cinder.evaluate.migrated.count: acme=1 point/value 5.0, globex=1/2.0
    // expect cinder.migrate.count: acme=5 points, globex=2 points
}
```

## DoR satisfied for this slice

See `discuss/dor-validation.md` US-03 row.

## Out of scope for this slice

- Property-style assertion that across all possible workloads
  `sum(cinder.evaluate.migrated.count.values) == count(cinder.migrate.count)`
  for tenants that have only `evaluate_at`-driven migrations. This is
  implied by Cinder's own contract; v0 ships only the worked examples
  above. A property test could be added in DELIVER if the crafter chooses.

## Risks specific to this slice

| Risk | Mitigation |
|------|------------|
| Future Cinder change starts calling `record_evaluate` even for tenants with 0 migrations (a contract drift in `store.rs`) | Acceptance test `cinder_evaluate_with_no_eligible_items_emits_no_evaluate_point` would fail. Recorded in `wave-decisions.md` D3. |
| `migrated as f64` precision loss at very large counts | Operationally impossible (counts >2^53 per evaluate would imply 9 quadrillion items in flight). Documented in `outcome-kpis.md` under `migrated_count` risk row. |
| The dual-emission contract creates the suspicion of a double-count bug to a fresh reader | Slice's first test name and the journey-visual.md call this out explicitly: per-item migrate AND per-tenant evaluate are both emitted intentionally. |

## Definition of Done for this slice

- All Slice 01 + Slice 02 + Slice 03 tests green (full file).
- `cargo clippy --workspace --all-targets` clean.
- Slice closes the feature: the three event types of `cinder::MetricsRecorder` are all bridged. The `self_observe::CinderToPulseRecorder` is feature-complete at v0.
