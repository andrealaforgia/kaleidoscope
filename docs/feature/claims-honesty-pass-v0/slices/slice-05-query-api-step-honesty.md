# Slice 05 — query-api `step` honesty (DESIGN flag #1)

- **Story**: US-05
- **Priority**: P5
- **Type**: DOCUMENT-vs-IMPLEMENT (DESIGN decides)
- **Independently shippable**: yes
- **DESIGN weight**: MEDIUM — a real decision

## Value

The read endpoint's stepped-grid implication matches reality — an evaluator
wiring Prometheus/Grafana tooling forms a correct `step` expectation.

## Exact loci (verified)

| File:line | State | Note |
|-----------|-------|------|
| `query-api/src/lib.rs:136-146` | ALREADY HONEST | field doc: "`step` is accepted and ignored at v0 (DD5: raw points, no re-stepping)" |
| `README.md:104-108` | OVERSTATED (framing) | brands endpoint "Prometheus-compatible `/api/v1/query_range`", implying stepped-grid semantics |

`step` is deserialised (`QueryRangeParams.step:143-146`) then never used; raw
native-timestamp points are returned via `to_matrix`.

## The decision (DESIGN owns it)

- **Option A — document (DISCUSS recommends)**: qualify the README so it states
  `step` is accepted-but-not-honoured at v0; raw points returned. Proportionate to
  an honesty pass. No code touch, no mutation obligation.
- **Option B — implement**: build the Prometheus stepped grid so `step`
  re-samples. Real feature; carries a code touch + per-feature mutation
  obligation; arguably belongs in its own feature.

## Acceptance shape (for DISTILL)

- Black-box (the verifier is already building it): two `step` values over the same
  window → the relationship between outputs (identical raw points under A;
  distinct stepped grids under B) matches the corrected claim.
- Doc guard on the README `step`/`query_range` description.
- The black-box result and the prose agree (0 gap).

## Guardrails

- DISCUSS does NOT decide A vs B. If B, the mutation obligation and a behaviour
  change apply; if A, pure prose. The verifier's black-box is satisfied by either.
