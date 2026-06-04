# Slice 03 — Stale `__SCAFFOLD__`-over-green doc comments

- **Story**: US-03
- **Priority**: P3
- **Type**: Pure prose (doc comments)
- **Independently shippable**: yes
- **DESIGN weight**: light (but careful guardrail)

## Value

Two delivered, green crates stop claiming their bodies are `unimplemented!`
`__SCAFFOLD__` RED stubs.

## Exact loci (verified — STALE OVER GREEN, touch)

| File:line | False claim | Truth source |
|-----------|-------------|--------------|
| `query-http-common/src/lib.rs:30-42` | "DISTILL scaffold — DELIVER fills the bodies … All free functions are `unimplemented!("__SCAFFOLD__ query-http-common-v0 RED")`" | bodies live: `parse_time_range:178`, `resolve_tenant_or_refuse:239`, `error_response:268`, `init_tracing:317`; each fn doc already says "DELIVER state: implemented" |
| `trace-query-api/src/lib.rs:207-209` | "Scaffold for DISTILL Mandate 7 RED-not-BROKEN: … the handler is `unimplemented!`" | `TracesByIdParams` + handler live |
| `trace-query-api/src/lib.rs:228-232` | "the handler is `unimplemented!` … DELIVER implements the body" | `handle_traces_by_id:233-292`, `parse_trace_id:304-320` live |

## DO-NOT-TOUCH loci (GENUINELY IN-FLIGHT — leave intact)

These describe a TRUE current state and must NOT be altered:

- `log-query-api/tests/slice_05_body_regex.rs`, `slice_06_pagination.rs`,
  `slice_04_body_contains.rs`, `slice_03_severity_filter.rs` — `parse_*` are
  genuinely `unimplemented!` `__SCAFFOLD__` for in-flight features.
- `*/tests/v1_slice_0{3,4}*_crash_durability.rs` (lumen, ray, strata, cinder,
  sluice, beacon, pulse) — `#[ignore]`d RED in-flight.
- `aperture/tests/slice_09_tls_config_reject.rs` — RED markers.
- `kaleidoscope-gateway` / `log-query-api` tracing-subscriber slices — RED no-op.

## Acceptance shape (for DISTILL)

Bidirectional guard: the stale scaffold phrasing ABSENT in the two touched loci
AND the listed in-flight markers STILL PRESENT (proving the correction did not
over-reach). Touched crates' suites green.

## Guardrails

- The bidirectional guard is the safety mechanism. Never collapse "remove stale
  scaffold doc" into a blanket `__SCAFFOLD__` sweep.
