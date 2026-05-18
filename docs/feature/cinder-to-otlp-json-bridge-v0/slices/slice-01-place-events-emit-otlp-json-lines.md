# Slice 01 — Place events emit OTLP-JSON lines

**Story**: US-01
**Outcome KPI**: OK1
**Tag**: `@infrastructure` (library-level; cross-process dashboard wiring
is a post-v0 follow-up feature)
**Estimated effort**: ~4 hours

## Goal

Wire Cinder's `record_place(tenant, tier)` events into a `Write` sink as
one OTLP-JSON `ResourceMetrics` line per call, with metric name
`cinder.place.count`, scope `kaleidoscope.cinder`, per-tenant resource +
point attribute, and the entry tier as a point attribute. This slice
also introduces all the OTLP-JSON serde structs and the line-emission
function that slices 02 and 03 inherit unchanged.

## What ships in this slice

| Artifact | Change |
|----------|--------|
| `crates/self-observe/src/cinder_otlp_json.rs` | NEW. `CinderToOtlpJsonWriter<W: Write + Send + Sync>` struct with `record_place` implemented; `record_migrate` / `record_evaluate` ship as `todo!()` (or as no-op stubs — DESIGN decides) and are NOT exercised by this slice's tests. All OTLP-JSON serde structs (`OtlpResourceMetrics`, `OtlpResource`, `OtlpScopeMetrics`, `OtlpScope`, `OtlpMetric`, `OtlpSum`, `OtlpNumberPoint`, `OtlpAttr`, `OtlpAttrValue`) are introduced here, duplicating the shape from `lumen_otlp_json.rs:62-120` per `wave-decisions.md` D7. |
| `crates/self-observe/src/lib.rs` | `mod cinder_otlp_json;` + `pub use cinder_otlp_json::CinderToOtlpJsonWriter;`. |
| `crates/self-observe/Cargo.toml` | New `[[test]] name = "cinder_to_otlp_json", path = "tests/cinder_to_otlp_json.rs"`. No new external dep (cinder already added by the Pulse-sink sibling; serde + serde_json already present). |
| `crates/self-observe/tests/cinder_to_otlp_json.rs` | NEW. Slice 01 test block: 6 tests (happy path + envelope shape, three-tier serialisation, two-tenant resource isolation, no-event-no-byte, NDJSON line-termination invariant, Send+Sync compile-time check). |

## Acceptance tests for this slice

Names follow the Lumen OTLP-JSON test naming convention (see
`crates/self-observe/tests/lumen_to_otlp_json.rs`):

```rust
#[test]
fn cinder_place_emits_one_otlp_resource_metrics_line_under_same_tenant() {
    // happy path: place once for acme/Hot ->
    //   - 1 line
    //   - line parses as JSON
    //   - resource.attributes[0]                = {key:"tenant_id", stringValue:"acme"}
    //   - scopeMetrics[0].scope.name            = "kaleidoscope.cinder"
    //   - scopeMetrics[0].metrics[0].name       = "cinder.place.count"
    //   - sum.aggregationTemporality            = 2
    //   - sum.isMonotonic                       = true
    //   - sum.dataPoints[0].asInt               = "1"
    //   - sum.dataPoints[0].attributes contains {tier: "hot"}
    //   - sum.dataPoints[0].timeUnixNano parses as u64
}

#[test]
fn cinder_place_serialises_each_tier_as_lowercase_string() {
    // three items in Hot/Warm/Cold (same tenant) ->
    //   - 3 lines
    //   - every line metric name "cinder.place.count"
    //   - tier attribute set == {"hot","warm","cold"}
}

#[test]
fn two_tenants_cinder_place_emit_distinct_otlp_resource_attributes() {
    // acme x1, globex x2 ->
    //   - 3 lines
    //   - 1 line has resource.tenant_id="acme"
    //   - 2 lines have resource.tenant_id="globex"
    //   - every globex line has dataPoints[0].attributes containing {tier: "hot"}
}

#[test]
fn no_cinder_event_means_zero_bytes_in_the_ndjson_sink() {
    // writer constructed, never used -> buffer is empty
}

#[test]
fn output_is_ndjson_one_line_per_event_with_trailing_newline() {
    // fire 3 place events ->
    //   - sink ends with b"\n"
    //   - split-on-"\n" yields 3 non-empty lines
    //   - every non-empty line parses as a JSON object
    // (mirror of the Lumen writer's `output_is_ndjson_one_record_per_line_with_trailing_newline`)
}

#[test]
fn the_writer_is_send_and_sync() {
    // compile-time check; lives in this slice but covers all slices
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<CinderToOtlpJsonWriter<Vec<u8>>>();
}
```

The test harness type `SharedBuf(Arc<Mutex<Vec<u8>>>)` and the
`collect_lines` helper are copied verbatim from
`crates/self-observe/tests/lumen_to_otlp_json.rs:54-73`. DESIGN may
decide to extract this into a test-support module if Slice 02 or 03
benefits; this slice ships it inline.

## DoR satisfied for this slice

See `discuss/dor-validation.md` US-01 row.

## Out of scope for this slice

- `record_migrate` and `record_evaluate` implementations (Slices 02/03).
- Any CLI wiring or runbook documentation (post-v0 follow-up feature).
- Cross-writer interleaving tests against a real `File` (owned by the
  CLI follow-up feature).
- Extracting the OTLP-JSON serde structs into a shared module (rule of
  three not reached; `wave-decisions.md` D7).

## Risks specific to this slice

| Risk | Mitigation |
|------|------------|
| Tier serialisation choice ("hot" vs "Hot" vs numeric) becomes load-bearing for Slices 02/03 | This slice's `cinder_place_serialises_each_tier_as_lowercase_string` test locks the convention by string-equality assert. Slices 02/03 reuse the same helper. |
| The `record_migrate`/`record_evaluate` stubs accidentally satisfy the trait silently and let Slice 02/03 ship without implementations | DESIGN wave should pick stubs that fail loudly (`todo!()` or `unimplemented!()`) rather than empty impls. Acceptance tests in Slices 02/03 would catch the gap regardless. |
| The OTLP-JSON envelope shape from the Lumen writer's `[OtlpAttr; 1]` fixed-size array does not generalise cleanly to Cinder's 2-attribute case in Slice 02 | This slice has a 2-attribute point too (`tenant_id` + `tier`), so the generalisation pressure surfaces HERE, not in Slice 02. If `[OtlpAttr; 2]` works for place, Slice 02 will work with `[OtlpAttr; 3]` for migrate (`tenant_id` + `from` + `to`). DESIGN should choose between fixed-size arrays per metric (mirrors Lumen's `[OtlpAttr; 1]` choice) and `Vec<OtlpAttr>` (simpler, slightly less efficient). |
| Cross-bridge metric-name drift with the Pulse-sink sibling | The slice's assert on `metric_name == "cinder.place.count"` matches the string literal in `crates/self-observe/src/cinder_bridge.rs:121`. A code reviewer diffing the two files will catch any drift. |

## Definition of Done for this slice

- All 6 tests above green under `cargo test --package self-observe --test cinder_to_otlp_json`.
- `cargo clippy --workspace --all-targets` clean (no new warnings).
- `crates/self-observe/src/lib.rs` re-export of `CinderToOtlpJsonWriter` visible in `cargo doc`.
- No drift detected between the metric name string in `cinder_otlp_json.rs` and the corresponding string in `cinder_bridge.rs` (reviewer diff check).
