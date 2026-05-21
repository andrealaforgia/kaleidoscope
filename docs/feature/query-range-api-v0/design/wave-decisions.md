# Wave Decisions: query-range-api-v0 (DESIGN)

Architect: Morgan (`nw-solution-architect`). Interaction mode: propose.
British English throughout. No em dashes.

This wave turns the DISCUSS requirements and the externally pinned Prism
contract into a technical design the acceptance-designer and the crafter
can execute without ambiguity. The response shape is NOT open for design
(ADR-0027 plus `apps/prism/src/lib/promql/queryRange.ts` lock it); DESIGN
owns where the service lives, the parser boundary, the translation rule,
the tenancy seam, and the route path.

## Mandatory reads checklist

- [x] `apps/prism/src/lib/promql/queryRange.ts` — request builder + 5-arm validator
- [x] `apps/prism/src/lib/promql/types.ts` — `QueryRangeContext.backend` doc: base URL is `/api/v1`
- [x] `apps/prism/src/lib/config/types.ts` — `RuntimeConfig.backend.url` is the prefix
- [x] `crates/pulse/src/metric.rs` — `Metric` / `MetricPoint` / `MetricName` / `MetricKind` / `TimeRange`
- [x] `crates/pulse/src/store.rs` — `MetricStore::query(&TenantId, &MetricName, TimeRange) -> Vec<(Metric, MetricPoint)>`
- [x] `crates/pulse/src/file_backed.rs` — `FileBackedMetricStore::open(base_path, recorder)`
- [x] `crates/aperture/src/transport.rs` — axum `Router` + `axum::serve(listener, router)` pattern
- [x] `crates/aperture/src/lib.rs` — lib/binary split, `spawn`/`run`/`Handle`, wire-then-probe-then-use
- [x] `crates/aperture/Cargo.toml` — axum 0.7 + hyper + tokio confirmed as the workspace HTTP stack
- [x] `crates/kaleidoscope-gateway/src/main.rs` — opens `FileBackedMetricStore` at `pillar_root/pulse`, resolves default tenant fail-closed, `wire -> probe -> use`
- [x] `crates/aegis/src/validator.rs` — `pub struct TenantId(pub String)`
- [x] DISCUSS: wave-decisions.md, user-stories.md, story-map.md, outcome-kpis.md
- [x] ADR-0027 (Prism client contract), ADR-0041 (storage-sink translation + tenancy + probe)
- [x] ADR scan: highest existing number is 0041; next free is 0042

## The pinned contract (verified against the files, not assumed)

`buildUrl` in `queryRange.ts` builds `${backend}/query_range?query=...&start=...&end=...&step=15s`.
`QueryRangeContext.backend` is documented as the base URL `/api/v1`, and
`RuntimeConfig.backend.url` is that prefix. Therefore the final request
path the backend MUST serve is **`/api/v1/query_range`** with the four
query parameters `query` (raw PromQL string), `start` and `end` (float
epoch SECONDS), and `step` (`"15s"`). Prism's validators:

- `isPromSuccess`: `status === 'success'` AND `Array.isArray(data.result)`.
- `isPromError`: `status === 'error'` AND `typeof error === 'string'`.
- `parseValue`: the string `"NaN"` maps to `Number.NaN`; otherwise `parseFloat`.
- `result: []` is a calm success arm (the `empty` outcome), never an error.
- A 400 with `{status:'error', error:'...'}` becomes the `parse-error` arm.

These are reproduced verbatim in `application-architecture.md` and frozen.

## Design Decisions (DD)

- **DD1 (placement: new crate, lib + thin binary).** A NEW workspace crate
  `query-api` with a `[lib]` (parser, translation, axum handler, tenant
  seam, store-port) plus a thin `[[bin]]` composition root that opens the
  Pulse store read-only and binds the listener. Mirrors the
  aperture / kaleidoscope-gateway split. Justified over a single binary
  crate because the parser and translation carry real mutable logic that
  must be unit- and mutation-tested without spawning a server; a lib seam
  is the only way to test them in isolation (ISO 25010 testability).

- **DD2 (HTTP stack: reuse axum + hyper + tokio).** The crate uses
  `axum` 0.7 (`Router`, `axum::serve(listener, router)`) on `tokio`,
  identical to `crates/aperture/src/transport.rs`. No new web framework is
  introduced (CLAUDE.md Rust-idiomatic posture; OSS-first; one HTTP shape
  across the platform). Route registered with `.route("/api/v1/query_range", get(handle_query_range))`.

- **DD3 (minimal PromQL parser: bare metric name only at slice 01).** A
  `selector` module in the lib parses ONLY a bare metric-name selector.
  Accepted grammar (slice 01): after trimming surrounding ASCII whitespace,
  the entire query MUST match the Prometheus metric-name production
  `[a-zA-Z_:][a-zA-Z0-9_:]*` and nothing else. Anything else (empty,
  `{`-matcher, `[`-range-vector, `(`-function/aggregation, any operator
  character, whitespace-separated tokens) returns HTTP 400 `status:error`.
  Honest "unsupported" rather than a silent wrong answer. Slice 02 adds a
  single `{label="value"}` matcher; full PromQL (operators, functions,
  aggregations, `rate()`) is deferred to v1. Boundary is executable
  (US-03, US-05).

- **DD4 (translation: Pulse rows -> Prometheus matrix).** A `matrix` module
  groups `Vec<(Metric, MetricPoint)>` into one `PromMatrixEntry` per
  distinct merged label set. Label-set derivation rule (DD4a): the label
  map for a point is `{"__name__": metric.name} ∪ metric.resource_attributes ∪ point.attributes`,
  with point attributes winning on key collision, then resource attributes,
  then `__name__` always present (Prometheus convention). Time conversion
  (DD4b): `time_unix_nano` -> seconds as an integer (`time_unix_nano / 1_000_000_000`),
  matching the example `1716200000` in US-01. Value conversion (DD4c):
  `f64` -> string; `NaN` -> `"NaN"`; whole-valued floats render without a
  trailing `.0` (e.g. `0.0` -> `"0"`, per US-01 example 4), matching
  Prometheus' minimal-decimal rendering. Each series' `values` array is in
  ascending time order (Pulse already returns ascending; the grouping
  preserves it).

- **DD5 (start/end/step: raw points, no re-stepping at v0).** v0 converts
  `start`/`end` seconds to nanoseconds and returns the Pulse points that
  fall within the half-open `[start, end)` range WITHOUT step-alignment,
  downsampling, or staleness handling. `step` is accepted and ignored at
  v0. This is acceptable for Prism's matrix renderer: `queryRange.ts`
  maps each `[seconds, value]` pair straight to a chart point and the chart
  tolerates irregular spacing (`connectNulls:false`, `smooth:false`); the
  validator does not require regular spacing (RED CARD 2 closed). Step
  alignment and staleness are documented as v1.

- **DD6 (response + error arms).** Success and empty serialise to
  `{status:'success', data:{resultType:'matrix', result:[...]}}` (empty
  array for the empty arm). Errors serialise to HTTP 400
  `{status:'error', error:'<reason>'}`. 400 cases: unparseable / unsupported
  query (DD3); non-numeric `start`/`end`/`step`; inverted bounds
  (`start > end`). A Pulse `PersistenceFailed` returns HTTP 5xx with a
  `status:error` body (US-01 scenario: never a fabricated empty success).
  Error text NEVER echoes a forwarded header value (ADR-0027 §6 symmetry).

- **DD7 (tenancy: configured single tenant, fail-closed).** Slice 01
  resolves the tenant from `KALEIDOSCOPE_QUERY_TENANT` (env), fail-closed
  if unset or empty, mirroring the gateway's `KALEIDOSCOPE_DEFAULT_TENANT`
  posture (`crates/kaleidoscope-gateway/src/main.rs`). The resolved
  `aegis::TenantId` is passed to `pulse.query`. A `TenantResolver` port in
  the lib makes the mechanism swappable: header-based tenancy
  (`X-Scope-OrgID`, the Mimir/Cortex convention) or an aegis Bearer token
  is deferred to a later slice and lands behind the same seam without
  changing the query path (US-04 AC4). RED CARD 1 closed: configured single
  tenant, fail-closed.

- **DD8 (read-only store open).** The binary opens the Pulse store via the
  same `FileBackedMetricStore::open(pillar_root/pulse, recorder)` the
  gateway uses. v0 has no read-only file mode in the Pulse API; the query
  binary opens the same store and uses only `query` (never `ingest`). The
  binary and the gateway are NOT run against the same live directory
  concurrently at v0 (single-writer assumption); concurrent
  reader-against-live-writer is a v1 concern flagged below. The store port
  in the lib is `MetricStore` (the existing trait), so an in-memory double
  drives the lib tests.

- **DD9 (Earned-Trust probe).** The binary follows wire-then-probe-then-use
  (ADR-0041 / gateway). After opening the store and before binding the
  listener, the composition root runs a `probe()`: it issues a trivial
  `query` against the resolved tenant for a sentinel metric name over an
  empty range and asserts it returns `Ok` (the store is readable and the
  tenant resolves). A probe failure refuses startup with
  `event=health.startup.refused` and a non-zero exit, never a half-up
  listener. The probe contract is enforced the same three orthogonal ways
  the platform already uses (subtype at the composition-root boundary,
  structural AST check, behavioural gold-test); see ADR-0042 Verification.

## Reuse Analysis (MANDATORY)

| Need | Existing asset | Reuse / extend / new | Justification |
|------|----------------|----------------------|---------------|
| Metric read surface | `pulse::MetricStore::query(&TenantId, &MetricName, TimeRange)` | Reuse verbatim | Exact shape the feature needs; no new store method |
| Durable store open | `pulse::FileBackedMetricStore::open(path, recorder)` | Reuse verbatim | Gateway already opens it this way at `pillar_root/pulse` |
| Tenant identity | `aegis::TenantId(pub String)` | Reuse verbatim | Same vocabulary the write path persisted under (US-04 scenario 3) |
| Tenant resolution posture | gateway `KALEIDOSCOPE_DEFAULT_TENANT` fail-closed | Extend (new env `KALEIDOSCOPE_QUERY_TENANT`) | Mirror the write-path posture; symmetric operator surface |
| HTTP server | aperture `axum::serve(listener, Router)` on tokio/hyper | Reuse pattern | One HTTP stack across the platform; no new framework (DD2) |
| lib+binary split | aperture / kaleidoscope-gateway layout | Reuse pattern | Testable lib seam + thin composition root (DD1) |
| wire-then-probe-then-use | gateway `sink.probe()` before listen; ADR-0041 | Reuse pattern | Earned-Trust invariant (DD9) |
| Header redaction posture | ADR-0027 §6 (Prism side) | Mirror on the server | status:error must not echo a forwarded header value (DD6) |
| PromQL parser | none in workspace | NEW (`selector` module) | No existing PromQL parser; minimal bare-name subset (DD3) |
| Matrix translation | none in workspace | NEW (`matrix` module) | Pulse rows -> Prometheus matrix is feature-specific (DD4) |

Searched: Glob over `crates/*/Cargo.toml`, Grep for `query_range`, `PromQL`,
`matrix`, `TenantId`. No existing query-API crate, parser, or matrix
translator. The two genuinely new modules (`selector`, `matrix`) carry the
only mutable logic; everything else is reuse.

## DEVOPS handoff annotation (for Apex, `@nw-platform-architect`)

- **gate-5-mutants-query-api**: the new `query-api` crate carries real
  mutable logic (the `selector` parser and the `matrix` translation,
  including time and value formatting and label-set merging). A
  per-crate `cargo mutants` job scoped to `crates/query-api/src/` is
  warranted at the project 100% kill-rate gate (CLAUDE.md mutation
  strategy; ADR-0005 Gate 5). Flag for Apex to add the job.

- **External integration / contract test**: `/api/v1/query_range` is the
  consumer-driven contract boundary with Prism. ADR-0027 already records
  the Prism-side contract-test recommendation (Pact-JS or a
  container-fixture posture). The provider side (this crate) SHOULD be
  exercised by a contract test that runs Prism's own `isPromSuccess` /
  `isPromError` shapes against this backend in CI, so a response-shape
  drift fails the build, not production. Recommended to Apex:
  consumer-driven contract via the four pinned response shapes
  (success, empty, parse-error, 5xx) asserted against Prism's validators.

- **Config surface**: new env var `KALEIDOSCOPE_QUERY_TENANT` (fail-closed),
  plus the existing `KALEIDOSCOPE_PILLAR_ROOT` to locate the Pulse store.
  Document in the deployment runbook.

## Deferred to v1 (recorded, not silently dropped)

- Step alignment, downsampling, staleness handling (DD5).
- Full PromQL (operators, functions, aggregations, range vectors); slice 02
  adds a single `{label="value"}` matcher only (DD3).
- Header-based / Bearer-token tenancy behind the `TenantResolver` seam (DD7).
- Concurrent reader-against-live-writer on the same Pulse directory (DD8);
  v0 assumes single-writer.
- Logs/traces query endpoints (different Prism panels, out of scope).

## ADR verdict

ADR-0042 (`query-api-contract-and-promql-subset`) is warranted: it records
the pinned query-API contract, the minimal-PromQL-subset boundary, the
label-set derivation rule, and the fail-closed tenancy policy with its
swappable seam. Two-plus alternatives evaluated. Written under
`docs/product/architecture/adr-0042-query-api-contract-and-promql-subset.md`.

## Quality gates (DESIGN)

- [x] Requirements (US-01..US-05) traced to components and DDs
- [x] Component boundaries with clear responsibilities (lib modules + binary)
- [x] Technology choices in ADR-0042 with 2+ alternatives
- [x] Quality attributes: testability (lib seam), reliability (fail-closed,
      5xx on persistence error), security (tenant isolation, header redaction)
- [x] Dependency-inversion: `MetricStore` + `TenantResolver` ports, deps inward
- [x] C4 L1 + L2 (Mermaid) in application-architecture.md
- [x] Integration pattern specified (sync HTTP GET, pinned JSON contract)
- [x] OSS-first: axum/hyper/tokio/serde_json, all already in the workspace
- [x] AC behavioural, not implementation-coupled (inherited from DISCUSS)
- [x] External integration annotated for contract testing (Prism)
- [x] Enforcement tooling recommended (cargo mutants gate-5; probe 3-layer)
- [x] Peer review completed (self-review fallback; verdict recorded below)

## Peer review (structured self-review fallback)

The `solution-architect-reviewer` subagent was not separately dispatchable
in this run; per the established fallback, an equivalent structured
self-review against the `nw-sa-critique-dimensions` criteria was applied
and the verdict recorded.

```yaml
review_id: "arch_rev_query-range-api-v0_design"
reviewer: "self-review (solution-architect-reviewer fallback)"
artifact: "design/application-architecture.md, design/wave-decisions.md, adr-0042"
iteration: 1

strengths:
  - "Placement reuses the aperture/gateway lib+binary split; no new framework (DD1, DD2)."
  - "Response shape is treated as a pinned external contract verified against the actual files, not assumed (route /api/v1/query_range derived from backend.url prefix + buildUrl)."
  - "Scope boundary is executable: every unsupported PromQL form is a tested 400 (DD3, ADR-0042 Decision 3)."
  - "Tenancy is fail-closed AND swappable behind a TenantResolver port (DD7), with two rejected unsafe alternatives recorded."
  - "Earned-Trust probe specified with three orthogonal enforcement layers (ADR-0042 Verification)."

issues_identified:
  architectural_bias:
    - issue: "Resume-driven / latest-tech check"
      severity: "low"
      location: "DD2 / ADR-0042 Decision 2"
      recommendation: "None. axum/hyper/tokio are the existing workspace stack; no trendy tech introduced. Pass."
  decision_quality:
    - issue: "ADR alternatives present"
      severity: "low"
      location: "ADR-0042 Alternatives"
      recommendation: "Five alternatives across placement / PromQL / tenancy, each with rejection rationale. Exceeds the 2+ minimum. Pass."
  completeness_gaps:
    - issue: "Performance architecture for re-stepping is deferred"
      severity: "low"
      location: "DD5"
      recommendation: "Acceptable: Prism's renderer tolerates irregular spacing; deferral is explicit and v1-scoped, not silent."
  implementation_feasibility:
    - issue: "Testability"
      severity: "low"
      location: "DD1"
      recommendation: "lib seam + MetricStore/TenantResolver ports + in-memory double make parser and translation isolatable. Pass."
  priority_validation:
    q1_largest_bottleneck:
      evidence: "The bottleneck is the missing read half of an existing write loop; QueryPanel is unmounted (US-01 baseline 0%)."
      assessment: "YES"
    q2_simple_alternatives:
      assessment: "ADEQUATE"
    q3_constraint_prioritization:
      assessment: "CORRECT"
    q4_data_justified:
      assessment: "JUSTIFIED"

approval_status: "approved"
critical_issues_count: 0
high_issues_count: 0
```

Verdict: APPROVED, iteration 1. No critical or high issues. Handoff to
DISTILL (acceptance-designer) and the DEVOPS annotation for Apex stand.
</content>
</invoke>
