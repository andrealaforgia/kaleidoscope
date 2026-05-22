# Wave Decisions: lumen-query-api-v0 (DESIGN)

Author: `@nw-solution-architect` (Morgan), DESIGN wave, 2026-05-22.
Interaction mode: propose. Scope: application. British English. No em dashes.

This wave resolves the three flags DISCUSS handed to DESIGN (response contract,
crate placement, route and request shape), pins the status mapping, and confirms
the lumen `LogStore` trait is unchanged. The full rationale, alternatives, and
consequences are in `docs/product/architecture/adr-0047-lumen-log-query-api-contract-and-crate-layout.md`.

## Key Decisions

| # | Decision | Resolution | Rationale (short) |
|---|----------|-----------|-------------------|
| 1 | Response contract (FLAG 1 / RED CARD 1) | **Plain JSON array of `LogRecord`s**; empty arm is `[]` with HTTP 200; error arm reuses `{status:"error", error}` | No prism log consumer pins a contract yet; the plain array is honest and lossless over the OTLP `LogRecord` field set; Loki-shaping is lossy and speculative, deferred behind the same route |
| 2 | Crate placement (FLAG 2 / RED CARD 2) | **NEW crate `crates/log-query-api`**, lib + thin binary | Logs are a different domain (different store trait, record type, contract) from the metrics-specific `query-api`; reuse the PATTERN, not the metrics types |
| 2a | Extract vs duplicate | **Duplicate the minimum; extract nothing now** | The shared surface is ~30 lines and the two `TimeRange` types differ; a shared crate is a clean later refactor once a third pillar appears, not speculative now |
| 3 | Route and request shape (FLAG 3 / RED CARD 4) | **`GET /api/v1/logs?start=&end=`**, epoch seconds, float-tolerant, converted to half-open `[start, end)` ns | Sibling to `/api/v1/query_range` under the same `/api/v1` prefix; query-string mirrors the metrics endpoint for operator muscle memory; a window is a filter, not a resource identity |
| 4 | Tenancy (RED CARD 3) | Configured single tenant `KALEIDOSCOPE_LOG_QUERY_TENANT`, fail-closed, behind the `Option<TenantId>` router seam | Mirrors the metrics `KALEIDOSCOPE_QUERY_TENANT` and the gateway default; header/Bearer deferred behind the same seam |
| 5 | lumen trait | **UNCHANGED**; read through existing `LogStore::query(&tenant, range)` | No method added or re-signed; `query_with(predicate)` exists but is out of scope for slice 01 |
| 6 | Earned-Trust probe | wire -> probe -> use; trivial query over an empty range before the listener binds | Refuses a half-up listener; mirrors ADR-0042 Decision 8, reproduced for the new crate |

## Architecture Summary

A new thin workspace crate `crates/log-query-api`, lib + binary, mirroring the
`query-api` shape. The `[lib]` exposes one driving port,
`router(store: Arc<dyn LogStore + Send + Sync>, tenant: Option<TenantId>) -> Router`,
serving `GET /api/v1/logs?start=&end=`. The handler orchestration is:
resolve-tenant (fail-closed 401) -> parse-bounds (400 on non-numeric or
inverted) -> `LogStore::query(&tenant, range)` -> serialise the `Vec<LogRecord>`
as a bare JSON array (200, `[]` when empty) -> map `PersistenceFailed` to a 500
that never fabricates an empty. The thin `[[bin]]` composition root opens the
durable `FileBackedLogStore`, resolves the tenant from
`KALEIDOSCOPE_LOG_QUERY_TENANT`, runs the Earned-Trust probe, and binds the axum
listener. The only genuine polymorphism is `Arc<dyn LogStore>` (the durable
adapter in production, a failing double in tests); everything else is data plus
free functions per CLAUDE.md.

### Status mapping (the honest three-way distinction)

| Outcome | Condition | HTTP | Body |
|---|---|---|---|
| Success | non-empty `query` result | 200 | JSON array of `LogRecord`s, ascending `observed_time` |
| Calm empty | `Ok(Vec::new())` (empty window OR unknown tenant) | 200 | `[]` |
| Bad window | non-numeric bound, or `start > end` | 400 | `{status:"error", error:"<names the invalid window>"}`; no store query run |
| Fail-closed | no tenant resolves | 401 | `{status:"error", error:"no tenant resolvable: ..."}` |
| Store failure | `PersistenceFailed` | 500 | `{status:"error", error:"the backing log store could not be read"}` |

Half-open: a record at exactly `start` is included, at exactly `end` excluded.
The error text never echoes a forwarded header/credential value.

### Exact success JSON shape (FLAG 1, pinned)

`LogRecord` already derives `serde::Serialize` (`crates/lumen/src/record.rs:44`),
so the array serialises faithfully with no hand-written mapping:

```json
[
  {
    "observed_time_unix_nano": 1716200005000000000,
    "severity_number": 17,
    "severity_text": "ERROR",
    "body": "checkout: payment timeout",
    "attributes": { "http.status_code": "503" },
    "resource_attributes": { "service.name": "checkout" },
    "trace_id": [/* 16 bytes or null */],
    "span_id": [/* 8 bytes or null */]
  }
]
```

The empty case is `[]`.

## Reuse Analysis (MANDATORY)

Reuse of the metrics `query-api` HTTP scaffolding, weighed item by item. The
verdict is REUSE THE PATTERN (reproduce cheaply in the new crate), NOT reuse the
metrics types (which would mix domains in one crate).

| Asset in `query-api` | Reusable as code? | Verdict for `log-query-api` |
|---|---|---|
| axum `Router` + `axum::serve` + tokio/hyper stack | Pattern, not code | REPRODUCE: same stack, a `/api/v1/logs` route over the `LogStore` port; no new dependency beyond what the workspace already has |
| Fail-closed `Option<TenantId>` router seam | Pattern, ~5 lines | REPRODUCE (duplicate): same seam, refuse with 401 when `None` |
| `error_response` JSON helper `{status, error}` | Pattern, ~6 lines | REPRODUCE (duplicate): identical error shape, reused for cross-pillar symmetry |
| `parse_time_range` epoch-seconds bounds parser | Pattern, ~30 lines | REPRODUCE (duplicate): same float-tolerant parse and inversion check, but produces a `lumen::TimeRange`, NOT `pulse::TimeRange`, so it is not type-identical |
| tower `oneshot` test posture | Pattern (dev) | REPRODUCE: drive the router with no bound port, ingest into a real `FileBackedLogStore`, query, assert |
| wire-then-probe-then-use composition root + `probe()` | Pattern | REPRODUCE: trivial empty-range query against the resolved tenant before binding; `health.startup.refused` on failure |
| Optional `ServeDir` same-origin static serving | Pattern (deferred) | OMIT at v0: no prism log UI exists (FLAG 3 deferred); add behind the same router later, non-breaking |
| `MetricStore` driven port | Metrics-specific | DO NOT REUSE: logs use `lumen::LogStore` |
| PromQL `selector` parser | Metrics-specific | DO NOT REUSE: no query language in slice 01 |
| `matrix` translator + Prometheus `{status,data}` success envelope | Metrics-specific | DO NOT REUSE: logs return a plain array (Decision 1) |
| `regex` dependency | Metrics-specific (label matchers) | DO NOT REUSE: no matchers in slice 01 |

**Verdict.** EXTEND `query-api` was rejected: the crate is metrics-domain
specific end to end and folding logs in would mix two domains and two response
envelopes to share ~40 lines of boilerplate. A NEW crate `log-query-api` keeps
the domains clean and reproduces the reusable PATTERN cheaply.
**Extract-vs-duplicate: DUPLICATE the minimum, extract nothing.** The shared
surface is ~30 lines, the two `TimeRange` types differ, and a shared
`query-http-common` crate now would couple two crates through a third on
speculation. A future extraction is a clean refactor once a THIRD HTTP read
pillar proves the shared shape.

## Constraints

- Logs are not metrics: no PromQL, no metric name, no query language; query
  inputs are tenant + `[start, end)` only (DISCUSS Key Decision 1).
- Read against the real durable `FileBackedLogStore` via `LogStore::query`, not
  a fixture (DISCUSS Key Decision 2).
- Tenant resolved fail-closed; zero cross-tenant leak (DISCUSS Key Decision 3,
  US-03).
- Slice 01 is one thin walking skeleton: tenant + window only. OUT of scope and
  declared: severity/level filtering, full-text body search, attribute/resource
  matchers (even though `query_with(predicate)` exists), pagination/limits, any
  prism UI, same-origin static serving (FLAG 3), Loki-shaping.
- The lumen `LogStore` trait is UNCHANGED (Decision 5).
- Paradigm: Rust idiomatic per CLAUDE.md (data + free functions; traits only
  where polymorphism is genuinely needed, i.e. the `Arc<dyn LogStore>` seam).
- No new external dependency beyond axum/hyper/serde/tokio/tower-http already in
  the workspace; no `regex` (no matchers).

## DEVOPS Handoff Annotation

For `@nw-platform-architect` (Apex) at the DEVOPS / graduation wave:

- **NEW crate `crates/log-query-api`** -> a **NEW CI job
  `gate-5-mutants-log-query-api`**: `cargo mutants` scoped to
  `crates/log-query-api/src/` via `--in-diff` at the project 100% kill-rate gate
  (ADR-0005 Gate 5; CLAUDE.md). Primary mutation targets: the half-open boundary,
  the empty-vs-error distinction, the bounds parser, and the fail-closed refusal.
- **A NEW per-crate tag at graduation** for `log-query-api` (a graduation matter,
  mirroring the `query-api` tag).
- **No new external dependency**: axum 0.7, hyper, serde, serde_json, tokio, and
  tower (dev) are already in the workspace; the crate adds NO crate not already
  in `Cargo.lock`. `regex` is NOT pulled in (no label matchers). Gate-4
  (`cargo deny`) should see no new licence, advisory, or yanked crate.
- **External integrations: none.** The endpoint reads the in-process
  first-party `lumen` store through the `LogStore` trait, not a network service.
  No pinned external consumer contract exists for the logs response yet (no prism
  log panel), which is why the plain-array contract was chosen; a consumer-driven
  contract is introduced only when a real consumer (prism log panel / Grafana)
  appears.
- **Earned Trust: a NEW probe** for the new crate's composition root
  (wire -> probe -> use), with the three-orthogonal-layer enforcement reproduced
  from ADR-0042 Decision 8 (subtype at the composition-root boundary, AST
  pre-commit that the binary probes before binding, behavioural gold-test with a
  lying store double asserting `health.startup.refused`).
- **Per-feature mutation testing 100%** scoped to the modified files (CLAUDE.md;
  ADR-0005 Gate 5).
- DELIVER paradigm: Rust idiomatic (data + free functions; the crafter owns the
  GREEN/REFACTOR internal structure; this design fixes only the public
  `router(store, tenant)` port, the route, the status mapping, the plain-array
  success shape, and the fail-closed/probe invariants).

## Contradictions with DISCUSS

None. DISCUSS pinned the BEHAVIOUR (in-window records as JSON, fail-closed
tenancy, calm empty 200, bad-window 400, store-failure 5xx, full field fidelity)
and explicitly FLAGGED the contract, placement, request shape, and tenant
mechanism to DESIGN. This wave resolves exactly those flags within the pinned
behaviour, and confirms the lumen trait is unchanged as DISCUSS verified.
