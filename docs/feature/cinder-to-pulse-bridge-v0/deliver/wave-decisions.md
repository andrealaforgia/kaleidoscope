# Wave Decisions — `cinder-to-pulse-bridge-v0` / DELIVER

**Author**: `nw-software-crafter` (Crafty)
**Date**: 2026-05-18
**Mode**: execute (subagent — autonomous)

## Inputs read

- `crates/self-observe/src/cinder_bridge.rs` — RED scaffold (DISTILL output)
- `crates/self-observe/tests/cinder_to_pulse.rs` — 11 acceptance tests (8 RED + 3 GREEN structural)
- `crates/self-observe/src/lumen_bridge.rs` — worked precedent for bridge shape
- `crates/cinder/src/metrics.rs` — `MetricsRecorder` trait signature (the driving port we adapt)
- `crates/cinder/src/tier.rs` — `Tier` enum (Hot/Warm/Cold)
- `crates/pulse/src/{metric,store}.rs` — `MetricBatch::with_metrics`, `Metric`, `MetricPoint`, `MetricStore::ingest`
- `docs/product/architecture/adr-0038-...` — §1 public surface, §2 per-event emission contract
- DISCUSS D4 (one tier-serialisation source of truth), D5 (best-effort emission)
- `.github/workflows/ci.yml` — `cargo mutants` invocation precedent (`--in-diff` per package)

## Starting state (handover from DISTILL)

```
cargo test -p self-observe --test cinder_to_pulse
test result: FAILED. 3 passed; 8 failed
```

The 3 already-GREEN tests were the cross-cutting quiescence + Send+Sync probe:

- `no_cinder_event_means_no_pulse_metric_point`
- `cinder_migrate_failure_with_unknown_item_emits_no_pulse_point`
- `cinder_evaluate_with_no_eligible_items_emits_no_evaluate_point`
- `the_bridge_is_send_and_sync` (compile-time)

These were structurally green because Cinder calls `record_*` only on success — the bridge sees nothing on failed migrate / zero-eligible evaluate, so the panic body never runs. Confirmed: they remained green throughout DELIVER.

## TDD cycles

### Slice 01 — `record_place` (US-01, OK1)

- **RED (verified)**: 3 acceptance tests panicked at `cinder_bridge.rs:77` with "not yet implemented — RED scaffold (DISTILL); DELIVER replaces this body." Confirmed RED for business-logic reasons (the body isn't there), not import/syntax error.
- **GREEN**: implemented `record_place(tenant, tier)` to emit one single-point `MetricBatch` with `name="cinder.place.count"`, `kind=Sum`, `value=1.0`, `attributes={"tier": lowercase(tier)}`, best-effort `let _ = self.pulse.ingest(...)`.
- **Tests turned GREEN**:
  - `cinder_place_produces_a_pulse_metric_point_under_same_tenant`
  - `cinder_place_serialises_each_tier_as_lowercase_string`
  - `two_tenants_cinder_place_events_land_in_isolated_pulse_buckets`

### Slice 02 — `record_migrate` (US-02, OK2)

- **RED (verified)**: 2 acceptance tests panicked at `record_migrate` body.
- **GREEN**: implemented `record_migrate(tenant, from, to)` to emit `cinder.migrate.count` with `value=1.0`, `attributes={"from": lc(from), "to": lc(to)}`, best-effort emission.
- **Tests turned GREEN**:
  - `cinder_migrate_produces_a_pulse_point_with_from_and_to_attributes`
  - `two_tenants_cinder_migrate_events_land_in_isolated_pulse_buckets`
- **Stayed GREEN**: `cinder_migrate_failure_with_unknown_item_emits_no_pulse_point` — Cinder still doesn't call `record_migrate` on failure, so quiescence holds.

### Slice 03 — `record_evaluate` (US-03, OK3)

- **RED (verified)**: 2 acceptance tests panicked at `record_evaluate` body.
- **GREEN**: implemented `record_evaluate(tenant, migrated)` to emit `cinder.evaluate.migrated.count` with `kind=Sum`, `value=migrated as f64` (VALUE-encoded per DISCUSS D2), `attributes={}` (per ADR-0038 §2), best-effort emission. Slice 03 depends on Slice 02 because the dual-emission test cross-asserts both `cinder.migrate.count` (5 per-item points from cascaded `record_migrate` calls inside `evaluate_at`) AND `cinder.evaluate.migrated.count` (1 per-tenant point) after a single `TieringStore.evaluate_at`.
- **Tests turned GREEN**:
  - `cinder_evaluate_emits_per_item_migrate_points_and_one_evaluate_point` (dual-emission)
  - `cinder_evaluate_across_two_tenants_emits_per_tenant_counts`
- **Stayed GREEN**: `cinder_evaluate_with_no_eligible_items_emits_no_evaluate_point` — Cinder still doesn't call `record_evaluate` when zero items migrated.

## Refactor (L1–L4, post-GREEN)

The scaffold's three `record_*` panic bodies were replaced in a single edit, but the GREEN code already had the refactored shape — mirroring `lumen_bridge.rs`:

1. **Extract Method (L2)** — private `emit(tenant, metric_name, value, attributes)` builds the single-point `MetricBatch` and ingests it best-effort. One place owns the `SystemTime::now() → UNIX_EPOCH → u64` timestamp source, the `kind=Sum`, `unit="1"`, empty resource-attribute conventions, and the `let _ = ...` swallow.
2. **Extract Function (L2)** — free function `tier_lowercase(Tier) -> &'static str` is the **one location** that enforces DISCUSS D4 (the wire-format tier serialisation). Both `record_place` (tier attribute) and `record_migrate` (from + to attributes) call it. Inverting any arm breaks both call sites.
3. **Rename (L1)** — struct field `_pulse` → `pulse` per ADR-0038 §1 (the leading-underscore was a scaffold-time "field never read" lint suppressor; now the `emit` helper reads it).
4. **Remove dead scaffolding (L1)** — removed `// SCAFFOLD: true` marker comment, removed the "RED scaffold" panic-text doc comments, removed the inline doc note about DELIVER replacing the body. Doc comment on the struct now describes the *real* behaviour (best-effort ingest, empty `MetricStoreError`).

No L3 (Polymorphism) or L4 (introduce trait) refactoring was needed — the symmetry across three `record_*` methods is data-driven (each method differs only in metric name, value source, attribute keys) and Rust's monomorphic dispatch through the existing `cinder::MetricsRecorder` trait is exactly the right shape.

## Gates after refactor

| Gate | Command | Result |
|------|---------|--------|
| 1. Active acceptance tests pass | `cargo test -p self-observe --test cinder_to_pulse` | **11 passed; 0 failed; 0 ignored** |
| 2-4. All workspace tests pass | `cargo test --workspace --all-targets` | **107 test-result rollups, 107 ok, 0 failed** |
| 5. Code formatting | `cargo fmt --all -- --check` (cinder_bridge.rs) | **clean** |
| 6. Static analysis | `cargo clippy --workspace --all-targets -- -D warnings` | **clean (zero warnings)** |
| 7. Build | `cargo build --workspace` (implicit via tests) | **clean** |
| 8. No test skips | `tests/cinder_to_pulse.rs` has zero `#[ignore]` | **clean** |
| 9. Test budget | 9 behavioural + 2 cross-cutting + 1 compile-time = 11 tests / 3 behaviours-per-port × 2 = 18 budget | **well within budget** |
| 10. No mocks inside hexagon | Bridge uses real `InMemoryMetricStore` (driven port real impl) and real `InMemoryTieringStore` (driving port real impl); no `Mock<…>` anywhere | **clean** |
| 11. Business language | Tests read as "place events land as queryable pulse points under same tenant" etc. | **clean** |

## Mutation testing (ADR-0005 Gate 5)

```
cargo mutants -p self-observe \
  --file crates/self-observe/src/cinder_bridge.rs \
  --in-place --timeout 30

Found 6 mutants to test
ok       Unmutated baseline in 0s build + 0s test
6 mutants tested in 24s: 6 caught
```

**Kill rate: 6/6 = 100%** (Gate 5 PASS per ADR-0005).

Six mutants were generated and all six were caught by the acceptance tests. No surviving mutants. The acceptance tests cross-assert on:

- the exact `value` (`1.0` for place/migrate, `migrated as f64` for evaluate) — mutating to `0.0` or `value + 1.0` is caught,
- the exact `attribute` keys (`tier`, `from`, `to`) — mutating to wrong key returns `None` from `.get(…)`,
- the exact tier-string mapping (`"hot"|"warm"|"cold"`) — `tier_lowercase` arm swaps are caught by `cinder_place_serialises_each_tier_as_lowercase_string` (asserts `BTreeSet{"hot","warm","cold"}` equality) and the from/to-bearing migrate tests,
- per-tenant isolation — bucket-leak mutations are caught by the two-tenant tests,
- the dual-emission contract — Slice 03's `cinder_evaluate_emits_per_item_migrate_points_and_one_evaluate_point` cross-asserts both metric names from a single `evaluate_at` call.

## Final test counts

```
cargo test -p self-observe --test cinder_to_pulse
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

cargo test -p self-observe
(all five test binaries plus doctests)
test result: ok. (every binary green)

cargo test --workspace --all-targets
107 test-result lines, 107 ok, 0 failed
```

## Confirmations

- **Scaffold marker removed**: the `// SCAFFOLD: true` comment in the module doc block is gone. The `RED scaffold` panic-text language is gone from both struct doc and impl bodies.
- **Field renamed**: `pub struct CinderToPulseRecorder { _pulse: ... }` → `pub struct CinderToPulseRecorder { pulse: ... }` per ADR-0038 §1.
- **Public surface unchanged otherwise**: constructor signature, trait `impl`, metric names, attribute keys, value encoding, `Send + Sync` bound — all exactly as locked by ADR-0038 §§1-2.

## Deviations from spec

**None.** Every constraint in the DELIVER brief held:

- ADR-0038 §1 public surface: held (field name `pulse`, constructor signature, trait impl — all exact)
- ADR-0038 §2 per-event emission: held (metric names exact, attribute keys exact, value encoding exact per slice)
- DISCUSS D5 (best-effort emission, `let _ = pulse.ingest(...)`): held
- Lumen bridge shape mirrored: held (`emit` helper + helper for one tier serialisation)
- `cargo test -p self-observe`: passes
- `cargo test --workspace --all-targets`: passes
- `cargo clippy --workspace --all-targets -- -D warnings`: passes
- `cargo fmt --all -- --check` on `cinder_bridge.rs`: passes

## Fmt note on the acceptance test file

`cargo fmt --all -- --check` reports 7 cosmetic line-wrap deltas in
`crates/self-observe/tests/cinder_to_pulse.rs` (the DISTILL-authored
test file). They are pre-existing in the DISTILL handover and the
DELIVER constraints explicitly forbid modifying the acceptance test
file ("Do NOT modify the acceptance test file"). Cosmetic fmt of the
test file is out of scope for DELIVER and is best deferred to a
DISTILL-touch follow-up or rolled into Andrea's atomic commit at his
discretion. The implementation file (`cinder_bridge.rs`) is fmt-clean.

## What did NOT happen (per HARD constraints)

- No modification of public surface beyond the locked rename `_pulse` → `pulse`
- No new public API beyond ADR-0038 §1
- No new dependencies (cinder was already in the manifest)
- No modification of the acceptance test file
- No modification of `lumen_bridge.rs` or any other existing source
- No commit, no push (Andrea will atomic-commit all four waves together)

## Files changed by DELIVER

- `crates/self-observe/src/cinder_bridge.rs` — scaffold panic bodies replaced with real
  emission paths; field renamed; helpers extracted; scaffold markers removed.

## Files created by DELIVER

- `docs/feature/cinder-to-pulse-bridge-v0/deliver/wave-decisions.md` — this file.
