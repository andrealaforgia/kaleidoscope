# Wave Decisions: ray-query-api-v0 (DESIGN)

Author: `@nw-solution-architect` (Morgan), DESIGN wave, 2026-05-22.
Interaction mode: propose. Scope: application. British English. No em dashes.

This wave resolves the flags DISCUSS handed to DESIGN (the service-key question,
the response contract, the crate placement, the request shape, the tenant
mechanism), pins the status mapping, and confirms the ray `TraceStore` trait is
unchanged. The full rationale, alternatives, and consequences are in
`docs/product/architecture/adr-0048-ray-trace-query-api-contract-and-crate-layout.md`.

## Key Decisions

| # | Decision | Resolution | Rationale (short) |
|---|----------|-----------|-------------------|
| 1 | Service key (FLAG 3 / RED CARD 5) | **Explicit required `service` request parameter**; call the EXISTING `query(&tenant, &service, range)`; a missing/empty `service` is a **400** (named, no store query), NOT an empty result | Most honest to the verified store, which mandates a `&ServiceName`; no trait change; lets the operator name the service per request; a missing required parameter is a malformed request, not a misleading empty |
| 2 | Response contract (FLAG 1 / RED CARD 1) | **Plain JSON array of raw `Span`s**, ascending `start_time_unix_nano`; empty arm `[]` at 200; error arm reuses `{status:"error", error}` | No prism trace consumer pins a contract yet; the plain array is honest and lossless over the OTLP `Span` field set; assembled/Tempo shaping is lossy and speculative, deferred behind the same route |
| 3 | Granularity (FLAG 4) | **Raw spans, no trace assembly** | Slice 01 returns the store's natural `Vec<Span>` unit; parent/child stitching and `get_trace` lookup are deferred |
| 4 | Route and request shape (RED CARD 4) | **`GET /api/v1/traces?service=&start=&end=`**, epoch seconds, float-tolerant, converted to half-open `[start, end)` ns | Sibling to `/api/v1/query_range` and `/api/v1/logs` under the same `/api/v1` prefix; query-string mirrors the sibling read paths; window and service are filters, not a resource identity |
| 5 | Tenancy (RED CARD 3) | Configured single tenant `KALEIDOSCOPE_TRACE_QUERY_TENANT`, fail-closed, behind the `Option<TenantId>` router seam | Mirrors `KALEIDOSCOPE_LOG_QUERY_TENANT` / `KALEIDOSCOPE_QUERY_TENANT` and the gateway default; header/Bearer deferred behind the same seam |
| 6 | Crate placement (FLAG 2 / RED CARD 2) | **NEW crate `crates/trace-query-api`**, lib + thin binary | Traces are a third domain (third store trait, record type, contract) from the metrics-specific `query-api` and the logs-specific `log-query-api`; reuse the PATTERN, not the types |
| 6a | Extract vs duplicate | **Duplicate the minimum now; extract nothing in this slice; RECORD a forward-looking `query-http-common` extraction recommendation** | This is the third clone (rule-of-three trigger), but the bounds parser is not type-identical across `pulse`/`lumen`/`ray` `TimeRange`, the three contracts differ, and extracting now would couple three crates through a fourth as a rider on a thin slice; raise a dedicated extraction feature after this crate ships |
| 7 | ray trait | **UNCHANGED**; read through existing `TraceStore::query(&tenant, &service, range)` | No method added or re-signed; `get_trace` and `query_with(predicate)` exist but are out of scope for slice 01 |
| 8 | Earned-Trust probe | wire -> probe -> use; trivial empty-range query before the listener binds | Refuses a half-up listener; mirrors ADR-0047 Decision 6 / ADR-0042 Decision 8, reproduced for the new crate |

## Architecture Summary

A new thin workspace crate `crates/trace-query-api`, lib + binary, mirroring the
`log-query-api` shape (itself mirroring `query-api`). The `[lib]` exposes one
driving port,
`router(store: Arc<dyn TraceStore + Send + Sync>, tenant: Option<TenantId>) -> Router`,
serving `GET /api/v1/traces?service=&start=&end=`. The handler orchestration is:
resolve-tenant (fail-closed 401) -> read and validate `service` (400 on
missing/empty, before the store) -> parse-bounds (400 on non-numeric or
inverted, before the store) -> `TraceStore::query(&tenant, &service, range)` ->
serialise the `Vec<Span>` as a bare JSON array (200, `[]` when empty) -> map
`PersistenceFailed` to a 500 that never fabricates an empty. The thin `[[bin]]`
composition root opens the durable `FileBackedTraceStore`, resolves the tenant
from `KALEIDOSCOPE_TRACE_QUERY_TENANT`, runs the Earned-Trust probe, and binds
the axum listener. The only genuine polymorphism is `Arc<dyn TraceStore>` (the
durable adapter in production, a failing double in tests); everything else is
data plus free functions per CLAUDE.md.

### Status mapping (the honest outcomes)

| Outcome | Condition | HTTP | Body |
|---|---|---|---|
| Success | non-empty `query` result | 200 | JSON array of `Span`s, ascending `start_time_unix_nano` |
| Calm empty | `Ok(Vec::new())` (empty window OR unknown `(tenant, service)`) | 200 | `[]` |
| Bad request | missing/empty `service`, OR non-numeric bound, OR `start > end` | 400 | `{status:"error", error:"<names the missing service or invalid window>"}`; no store query run |
| Fail-closed | no tenant resolves | 401 | `{status:"error", error:"no tenant resolvable: ..."}` |
| Store failure | `PersistenceFailed` | 500 | `{status:"error", error:"the backing trace store could not be read"}` |

Half-open: a span at exactly `start` is included, at exactly `end` excluded. A
span with an empty `service.name` is indexed by trace only, not by service
(`file_backed.rs:325`), so it is not reachable by a service query; declared, not
silently lost. The error text never echoes a forwarded header/credential value
nor the raw `service`/`start`/`end` values.

### Exact success JSON shape (FLAG 1, pinned)

`Span` already derives `serde::Serialize` (`crates/ray/src/span.rs:183`), so the
array serialises faithfully with no hand-written mapping; `trace_id`/`span_id`
serialise as lowercase hex strings:

```json
[
  {
    "trace_id": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    "span_id": "0102030405060708",
    "parent_span_id": null,
    "name": "place-order",
    "kind": "Server",
    "start_time_unix_nano": 1716200005000000000,
    "end_time_unix_nano": 1716200005120000000,
    "status": { "code": "Error", "message": "upstream timeout" },
    "attributes": { "http.route": "/orders" },
    "resource_attributes": { "service.name": "checkout" },
    "events": [],
    "links": []
  }
]
```

The empty case is `[]`.

## Reuse Analysis (MANDATORY)

Reuse of the metrics `query-api` and logs `log-query-api` HTTP scaffolding,
weighed item by item. The verdict is REUSE THE PATTERN (reproduce cheaply in the
new crate), NOT reuse the other domains' types (which would mix domains in one
crate).

| Asset in `query-api` / `log-query-api` | Reusable as code? | Verdict for `trace-query-api` |
|---|---|---|
| axum `Router` + `axum::serve` + tokio/hyper stack | Pattern, not code | REPRODUCE: same stack, a `/api/v1/traces` route over the `TraceStore` port; no new dependency beyond what the workspace already has |
| Fail-closed `Option<TenantId>` router seam | Pattern, ~5 lines | REPRODUCE (duplicate): same seam, refuse with 401 when `None` |
| `error_response` JSON helper `{status, error}` | Pattern, ~6 lines | REPRODUCE (duplicate): identical error shape, reused for cross-pillar symmetry |
| `parse_time_range` epoch-seconds bounds parser | Pattern, ~30 lines | REPRODUCE (duplicate): same float-tolerant parse and inversion check, but produces a `ray::TimeRange`, NOT `lumen::`/`pulse::TimeRange`, so it is not type-identical |
| Required-`service` parameter read + 400 on missing/empty | NEW, trace-specific | NEW: traces alone need the service key (the store mandates `&ServiceName`); ~4 lines, the one structural divergence from logs |
| tower `oneshot` test posture | Pattern (dev) | REPRODUCE: drive the router with no bound port, ingest into a real `FileBackedTraceStore`, query, assert |
| wire-then-probe-then-use composition root + `probe()` | Pattern | REPRODUCE: trivial empty-range query against the resolved tenant before binding; `health.startup.refused` on failure |
| Optional `ServeDir` same-origin static serving | Pattern (deferred) | OMIT at v0: no prism trace UI exists (FLAG 5 deferred); add behind the same router later, non-breaking |
| `MetricStore` / `LogStore` driven ports | Other-domain-specific | DO NOT REUSE: traces use `ray::TraceStore` |
| PromQL `selector` parser / `matrix` translator / Prometheus envelope | Metrics-specific | DO NOT REUSE: no query language, no matrix, a plain array (Decision 2) |
| `regex` dependency | Metrics-specific (label matchers) | DO NOT REUSE: no matchers in slice 01 |

**Verdict.** EXTEND `query-api` or `log-query-api` was rejected: each is
domain-specific end to end and folding traces in would mix a third domain and a
third contract to share ~40 lines of boilerplate. A NEW crate `trace-query-api`
keeps the domains clean and reproduces the reusable PATTERN cheaply.
**Extract-vs-duplicate: DUPLICATE the minimum, extract nothing IN THIS SLICE, but
RECORD a forward-looking `query-http-common` extraction recommendation.** This is
the THIRD clone, the rule-of-three trigger, so the extraction is now genuinely
live and is recorded rather than dismissed. It is DEFERRED off slice 01 because
(i) the bounds parser is not type-identical across the three `TimeRange` types,
(ii) the three contracts differ in body shape so only the error envelope and the
~5-line seam are truly identical, and (iii) extracting now would couple three
crates through a fourth as a rider on a thin read slice, a refactor with its own
blast radius, ADR, and mutation gate. The recommendation: once this crate ships
and the shared surface is proven across three real call sites, raise a dedicated
`query-http-common` extraction feature touching all three crates together under
its own ADR. The ~30 shared lines are mutation-tested in place until then.

## Constraints

- Traces are not metrics and not logs: no PromQL, no metric name, no query
  language; query inputs are tenant + `service` + `[start, end)` (DISCUSS Key
  Decision 1, and the service-key reality of CONTRADICTION 1).
- Read against the real durable `FileBackedTraceStore` via `TraceStore::query`,
  not a fixture (DISCUSS Key Decision 2).
- Tenant resolved fail-closed; zero cross-tenant leak (DISCUSS Key Decision 3,
  US-03).
- Slice 01 is one thin walking skeleton: tenant + service + window only, raw
  spans. OUT of scope and declared: trace-id lookup (`get_trace`); trace
  assembly; filters by operation/duration; predicate matchers (even though
  `query_with(predicate)` exists); a tenant+range fan-out across services (which
  would need a store change); pagination/limits; any prism UI; same-origin static
  serving (FLAG 5); assembled/Tempo shaping.
- The ray `TraceStore` trait is UNCHANGED (Decision 7).
- Paradigm: Rust idiomatic per CLAUDE.md (data + free functions; traits only
  where polymorphism is genuinely needed, i.e. the `Arc<dyn TraceStore>` seam).
- No new external dependency beyond axum/hyper/serde/tokio/tower-http already in
  the workspace; no `regex` (no matchers).

## DEVOPS Handoff Annotation

For `@nw-platform-architect` (Apex) at the DEVOPS / graduation wave:

- **NEW crate `crates/trace-query-api`** -> a **NEW CI job
  `gate-5-mutants-trace-query-api`**: `cargo mutants` scoped to
  `crates/trace-query-api/src/` via `--in-diff` at the project 100% kill-rate
  gate (ADR-0005 Gate 5; CLAUDE.md). Primary mutation targets: the half-open
  boundary, the empty-vs-error distinction, the missing-service 400, the bounds
  parser, and the fail-closed refusal.
- **A NEW per-crate tag at graduation** for `trace-query-api` (a graduation
  matter, mirroring the `query-api` and `log-query-api` tags).
- **No new external dependency**: axum 0.7, hyper, serde, serde_json, tokio, and
  tower (dev) are already in the workspace; the crate adds NO crate not already
  in `Cargo.lock`. `regex` is NOT pulled in (no label matchers). Gate-4
  (`cargo deny`) should see no new licence, advisory, or yanked crate.
- **External integrations: none.** The endpoint reads the in-process first-party
  `ray` store through the `TraceStore` trait, not a network service. No pinned
  external consumer contract exists for the traces response yet (no prism trace
  panel), which is why the plain-array contract was chosen; a consumer-driven
  contract is introduced only when a real consumer (prism trace panel / Grafana
  Tempo) appears.
- **Earned Trust: a NEW probe** for the new crate's composition root
  (wire -> probe -> use), with the three-orthogonal-layer enforcement reproduced
  from ADR-0047 Decision 6 / ADR-0042 Decision 8 (subtype at the composition-root
  boundary, AST pre-commit that the binary probes before binding, behavioural
  gold-test with a lying store double asserting `health.startup.refused`).
- **Per-feature mutation testing 100%** scoped to the modified files (CLAUDE.md;
  ADR-0005 Gate 5).
- **Forward-looking refactor flag**: this is the THIRD HTTP read-API crate. A
  dedicated `query-http-common` extraction feature is recommended AFTER this
  crate ships (touching `query-api`, `log-query-api`, `trace-query-api` together
  under its own ADR). NOT part of this slice; recorded for planning.
- DELIVER paradigm: Rust idiomatic (data + free functions; the crafter owns the
  GREEN/REFACTOR internal structure; this design fixes only the public
  `router(store, tenant)` port, the route, the required `service` parameter, the
  status mapping, the plain-array raw-`Span` success shape, and the
  fail-closed/probe invariants).

## Contradictions with DISCUSS

None. DISCUSS pinned the BEHAVIOUR (the in-window spans for the tenant returned
as JSON, fail-closed tenancy, calm empty 200, bad-window 400, no-tenant 401,
store-failure 500, full field fidelity) and explicitly FLAGGED the service-key
mechanism, the response contract, the placement, the request shape, and the
tenant mechanism to DESIGN. This wave resolves exactly those flags within the
pinned behaviour: it adopts Luna's recommended default (service as an explicit
request parameter, ray trait unchanged), pins missing-service as a 400 (a
required parameter), and confirms the ray trait is unchanged as DISCUSS verified.
