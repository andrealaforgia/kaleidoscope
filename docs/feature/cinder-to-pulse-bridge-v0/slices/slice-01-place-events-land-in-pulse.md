# Slice 01 — Place events land in Pulse

**Story**: US-01
**Outcome KPI**: OK1
**Tag**: `@infrastructure` (library-level; user surface is a post-v0 follow-up feature)
**Estimated effort**: ~3 hours

## Goal

Wire Cinder's `record_place(tenant, tier)` events into Pulse as queryable
`cinder.place.count` metric points under per-tenant partitions, with the
entry tier carried as a point attribute.

## What ships in this slice

| Artifact | Change |
|----------|--------|
| `crates/self-observe/src/cinder_bridge.rs` | NEW. `CinderToPulseRecorder` struct with `record_place` implemented; `record_migrate` / `record_evaluate` ship as `todo!()` (or as empty `impl` stubs, DESIGN decides) and are NOT exercised by this slice's tests. |
| `crates/self-observe/src/lib.rs` | `mod cinder_bridge;` + `pub use cinder_bridge::CinderToPulseRecorder;`. |
| `crates/self-observe/Cargo.toml` | New dep `cinder = { path = "../cinder", version = "0.1.0" }`. New `[[test]] name = "cinder_to_pulse", path = "tests/cinder_to_pulse.rs"`. |
| `crates/self-observe/tests/cinder_to_pulse.rs` | NEW. Slice 01 test block: 4 tests (happy path, three-tier serialisation, two-tenant isolation, no-event-no-point). |

## Acceptance tests for this slice

(Names taken from the Lumen bridge test naming convention.)

```rust
#[test]
fn cinder_place_produces_a_pulse_metric_point_under_same_tenant() {
    // happy path: place once for acme/Hot -> 1 point, value 1.0, tier=hot
}

#[test]
fn cinder_place_serialises_each_tier_as_lowercase_string() {
    // three items in Hot/Warm/Cold -> 3 points, tier attrs = {"hot","warm","cold"}
}

#[test]
fn two_tenants_cinder_place_events_land_in_isolated_pulse_buckets() {
    // acme x1, globex x2 -> acme query returns 1, globex query returns 2
}

#[test]
fn no_cinder_event_means_no_pulse_metric_point() {
    // bridge constructed, never used -> empty Vec
}

#[test]
fn the_bridge_is_send_and_sync() {
    // compile-time check; lives in this slice but covers all slices
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<CinderToPulseRecorder>();
}
```

## DoR satisfied for this slice

See `discuss/dor-validation.md` US-01 row.

## Out of scope for this slice

- `record_migrate` and `record_evaluate` implementations (Slices 02/03).
- Any CLI surface or runbook documentation (post-v0 follow-up feature).
- OTLP-JSON cross-process variant (separate future feature).

## Risks specific to this slice

| Risk | Mitigation |
|------|------------|
| Tier serialisation choice ("hot" vs "Hot" vs numeric) becomes load-bearing for Slices 02/03 | Slice 01 acceptance test locks the convention by string-equality assert. Slices 02/03 reuse the same helper. |
| The `record_migrate`/`record_evaluate` stubs accidentally satisfy the trait silently and let Slice 02/03 ship without implementations | DESIGN wave should pick stubs that fail loudly (`todo!()` or `unimplemented!()`) rather than empty impls. Acceptance tests in Slices 02/03 would catch the gap regardless. |

## Definition of Done for this slice

- All five tests above green under `cargo test --package self-observe --test cinder_to_pulse`.
- `cargo clippy --workspace --all-targets` clean (no new warnings).
- `crates/self-observe/src/lib.rs` re-export visible in `cargo doc`.
