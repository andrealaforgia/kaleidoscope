# Slice 03 — Evaluate events emit OTLP-JSON lines with per-tenant counts

**Story**: US-03
**Outcome KPI**: OK3
**Tag**: `@infrastructure`
**Estimated effort**: ~3 hours

## Goal

Wire Cinder's `record_evaluate(tenant, migrated)` events as one OTLP-JSON
`ResourceMetrics` line per (tenant, evaluate-call) pair where migrated >
0, with metric name `cinder.evaluate.migrated.count` and `asInt =
migrated.to_string()`. Zero-migration tenants produce no line (Cinder
does not call the recorder for them). Per-item migrate lines from
US-02 remain emitted by the SAME `evaluate_at` call into the SAME NDJSON
sink — the dual-emission contract.

This is the highest-information-density slice in the suite: the
cross-event-type assertion shape (count migrate lines AND count evaluate
lines from one `evaluate_at` call) is the strongest contract test the
feature ships.

## What ships in this slice

| Artifact | Change |
|----------|--------|
| `crates/self-observe/src/cinder_otlp_json.rs` | Replace the `record_evaluate` stub from Slice 01 with the real implementation: emit one OTLP-JSON line with `asInt = migrated.to_string()` and the `tenant_id` point attribute (no other point attribute on the evaluate metric). |
| `crates/self-observe/tests/cinder_to_otlp_json.rs` | Slice 03 test block appended: 3 tests (single-tenant dual emission with `asInt="5"`, zero-eligible quiescence, two-tenant per-tenant split with `asInt="5"` and `asInt="2"`). |

No new files, no new dependencies.

## Acceptance tests for this slice

```rust
#[test]
fn cinder_evaluate_emits_dual_lines_n_migrate_plus_one_evaluate() {
    // place 5 items for acme/Hot at t0
    // policy: Hot items older than 24h -> Warm
    // call evaluate_at(t0 + 25h, &policy) ->
    //   - 5 lines have metric "cinder.migrate.count" under acme
    //     with {from: hot, to: warm}
    //   - 1 line has metric "cinder.evaluate.migrated.count" under acme
    //   - that evaluate line's dataPoints[0].asInt = "5"
    //   - the cinder.evaluate.migrated.count line's dataPoints[0].attributes
    //     contains {tenant_id: "acme"}
    //   (the place events themselves emit 5 cinder.place.count lines too,
    //    so the total line count is 5 + 5 + 1 = 11; tests should assert
    //    the SUBSET shape, not the total count, to remain robust against
    //    DESIGN choices about whether place emits at place-time or at
    //    a deferred batch boundary — the current Cinder behaviour is
    //    emit-at-place-time, but this test should not depend on that)
}

#[test]
fn cinder_evaluate_with_zero_eligible_items_emits_no_evaluate_line() {
    // place 3 items for acme/Hot at t0
    // policy: Hot items older than 24h -> Warm
    // call evaluate_at(t0 + 1h, &policy) ->
    //   - no line in the sink has metric "cinder.evaluate.migrated.count"
    //     under acme
    //   - no line in the sink has metric "cinder.migrate.count" under acme
    //   (the 3 place lines from the placements remain present; that is
    //    expected and not asserted here)
}

#[test]
fn two_tenants_cinder_evaluate_emits_per_tenant_evaluate_lines() {
    // place 5 items for acme/Hot at t0
    // place 2 items for globex/Hot at t0
    // policy: Hot items older than 24h -> Warm
    // call evaluate_at(t0 + 25h, &policy) ->
    //   - exactly 1 evaluate line has tenant_id="acme"  with asInt="5"
    //   - exactly 1 evaluate line has tenant_id="globex" with asInt="2"
    //   - exactly 5 migrate lines have tenant_id="acme"
    //   - exactly 2 migrate lines have tenant_id="globex"
}
```

The first test is the "dual-emission contract" test referenced in
`wave-decisions.md` D8. It is intentionally complex because it pins
the cross-metric-name relationship from a single `evaluate_at` call.

## DoR satisfied for this slice

See `discuss/dor-validation.md` US-03 row.

## Out of scope for this slice

- Any CLI wiring (post-v0 follow-up feature).
- Tests for evaluate calls with extremely large migration counts
  (>2^53). The `usize -> String` rendering is exact for any usize; the
  collector's downstream behaviour with very large integers is a
  collector concern.

## Risks specific to this slice

| Risk | Mitigation |
|------|------------|
| The dual-emission test becomes brittle to changes in Cinder's per-tenant ordering inside `evaluate_at` (Cinder may visit tenants in any order, so line order may vary) | The assertions in `cinder_evaluate_emits_dual_lines_n_migrate_plus_one_evaluate` use COUNTS and SET MEMBERSHIP, not line indices. Same robustness pattern as the Lumen writer's two-tenant test (`tests/lumen_to_otlp_json.rs:160-189`). |
| `migrated.to_string()` for a `usize` on a 32-bit platform vs 64-bit platform produces the same numeric string for any operationally-meaningful count; no platform-specific risk at v0. | None needed. |
| Reviewer confusion about "why does evaluate emit one line with no point attribute other than tenant_id, while place/migrate emit lines with extra attributes?" | The story explicitly documents this: evaluate is the only metric where the VALUE carries information; place/migrate carry information in the attributes. See `wave-decisions.md` D4 + the story's Solution section. |

## Definition of Done for this slice

- All 3 new tests above green under `cargo test --package self-observe --test cinder_to_otlp_json`.
- All Slice 01 and Slice 02 tests still green.
- `cargo clippy --workspace --all-targets` clean.
- No drift between the `cinder.evaluate.migrated.count` literal in `cinder_otlp_json.rs` and the corresponding literal in `cinder_bridge.rs:135`.
- Reviewer confirms: the dual-emission assertion shape in `cinder_evaluate_emits_dual_lines_n_migrate_plus_one_evaluate` is preserved (asserts on subset shape, not total count).
