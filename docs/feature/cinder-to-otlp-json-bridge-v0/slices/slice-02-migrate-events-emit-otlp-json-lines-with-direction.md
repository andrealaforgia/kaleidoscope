# Slice 02 — Migrate events emit OTLP-JSON lines with direction attributes

**Story**: US-02
**Outcome KPI**: OK2
**Tag**: `@infrastructure`
**Estimated effort**: ~2 hours

## Goal

Wire Cinder's `record_migrate(tenant, from, to)` events as one OTLP-JSON
`ResourceMetrics` line per call, with metric name `cinder.migrate.count`,
`from` and `to` point attributes, and `asInt = "1"`. Failed migrates
(Cinder returns `Err(UnknownItem)`) produce zero lines because Cinder
does not call the recorder on failure — this slice asserts that
invariant.

## What ships in this slice

| Artifact | Change |
|----------|--------|
| `crates/self-observe/src/cinder_otlp_json.rs` | Replace the `record_migrate` stub from Slice 01 with the real implementation: emit one OTLP-JSON line with two point attributes (`from`, `to`) + the `tenant_id` point attribute. |
| `crates/self-observe/tests/cinder_to_otlp_json.rs` | Slice 02 test block appended: 3 tests (happy path direction attributes, failed-migrate quiescence, two-tenant isolation under simultaneous opposite-direction migrations). |

No new files, no new dependencies.

## Acceptance tests for this slice

```rust
#[test]
fn cinder_migrate_emits_line_with_from_and_to_attributes() {
    // place trade-001 for acme/Hot, then migrate to Warm ->
    //   - 2 lines total (place + migrate)
    //   - the migrate line has metric name "cinder.migrate.count"
    //   - that line's dataPoints[0].asInt = "1"
    //   - that line's dataPoints[0].attributes contains {from: "hot"}
    //   - that line's dataPoints[0].attributes contains {to: "warm"}
    //   - that line's resource.tenant_id = "acme"
}

#[test]
fn failed_cinder_migrate_emits_no_otlp_line() {
    // migrate(&acme, &item("ghost"), Tier::Warm, t) -> Err(UnknownItem)
    //   - zero lines in the sink have metric name "cinder.migrate.count"
    //   (the sink may or may not have other content; this test asserts
    //    absence of migrate lines specifically)
}

#[test]
fn two_tenants_cinder_migrate_emit_isolated_otlp_lines() {
    // place a1 for acme/Hot, place g1 for globex/Hot, then
    // migrate(acme, a1, Warm) and migrate(globex, g1, Cold), both Ok ->
    //   - exactly one migrate line has tenant_id="acme"
    //     with {from: hot, to: warm}
    //   - exactly one migrate line has tenant_id="globex"
    //     with {from: hot, to: cold}
    //   - no cross-tenant attribute leak
}
```

## DoR satisfied for this slice

See `discuss/dor-validation.md` US-02 row.

## Out of scope for this slice

- `record_evaluate` implementation (Slice 03).
- The cross-event-type dual-emission test (lives in Slice 03 because it
  needs both `migrate.count` and `evaluate.migrated.count` lines from
  one `evaluate_at` call).
- Any CLI wiring (post-v0 follow-up feature).

## Risks specific to this slice

| Risk | Mitigation |
|------|------------|
| Order-dependence of `from`/`to` attributes in the JSON output (some collectors care about attribute array order) | The acceptance test uses set-containment assertions (`contains {key: "from", ...}`), not array-index assertions. The DESIGN wave is free to emit them in either order. |
| `[OtlpAttr; 2]` for `from`+`to` plus `tenant_id` as a third point attribute means the migrate line has 3 point attributes total | This is the same pattern Slice 01 introduced (`[OtlpAttr; 2]` for `tenant_id` + `tier`). DESIGN should keep the choice consistent across all three metric methods. |
| Failed-migrate test is hard to distinguish from "no migrate ever happened" if the test doesn't include a successful migrate first | The test fires the failed migrate AFTER doing a successful place (which emits ZERO migrate lines), so the assertion is "zero migrate lines exist", which is unambiguous: a successful place emits a place line, not a migrate line. |

## Definition of Done for this slice

- All 3 new tests above green under `cargo test --package self-observe --test cinder_to_otlp_json`.
- All Slice 01 tests still green.
- `cargo clippy --workspace --all-targets` clean.
- No drift between the `cinder.migrate.count` literal in `cinder_otlp_json.rs` and the corresponding literal in `cinder_bridge.rs:128`.
