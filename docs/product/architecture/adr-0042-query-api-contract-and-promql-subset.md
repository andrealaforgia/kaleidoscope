# ADR-0042 — query-api contract, minimal PromQL subset, and tenancy

- **Status**: Accepted
- **Date**: 2026-05-21
- **Author**: `nw-solution-architect` (Morgan)
- **Feature**: `query-range-api-v0`
- **Supersedes**: none
- **Superseded by**: none
- **Related**: ADR-0027 (Prism backend HTTP client + error mapping — the
  consumer side of this contract), ADR-0041 (aperture-storage-sink
  translation + tenancy + wire-then-probe-then-use), ADR-0006 (aperture
  transport stack: axum + hyper + tonic)

## Context

Prism is built and pins the read contract; metrics persist into the
durable Pulse store via the ingest gateway, but nothing serves them back.
Prism's `queryRange.ts` issues `GET {backend}/query_range` where
`backend` is the `/api/v1` prefix, so the backend must serve
`/api/v1/query_range?query=<raw PromQL>&start=<epoch_seconds>&end=<epoch_seconds>&step=15s`
and return a Prometheus `matrix` response its pinned validators accept
(`isPromSuccess`, `isPromError`; `"NaN"` honoured; `result: []` is a calm
empty arm; errors are HTTP 400 `{status:'error', error}`).

The response shape is an external contract, not a design choice. DESIGN
owns: where the service lives, the PromQL subset it parses, the Pulse-to-
matrix translation, the start/end/step semantics, the route path, and the
tenancy mechanism. This ADR locks those.

Pulse exposes `MetricStore::query(&TenantId, &MetricName, TimeRange) -> Vec<(Metric, MetricPoint)>`
(per-tenant, per-metric-name, half-open `[start, end)` in nanoseconds).
`aegis::TenantId(pub String)` is the tenant vocabulary. The gateway opens
`FileBackedMetricStore` at `pillar_root/pulse` and resolves a fail-closed
default tenant from `KALEIDOSCOPE_DEFAULT_TENANT`.

## Decision

### 1. Placement: a new `query-api` crate, lib + thin binary

A new workspace crate `query-api` with a `[lib]` (the `selector` parser,
the `matrix` translator, the axum `handler`, a `TenantResolver` port, and
the `MetricStore` port) plus a thin `[[bin]]` composition root that opens
the Pulse store, runs the Earned-Trust probe, and binds the axum listener.
This mirrors the aperture / kaleidoscope-gateway lib+binary split. The lib
seam exists so the parser and translation (the only mutable logic) are
unit- and mutation-tested without spawning a server.

### 2. HTTP stack: reuse axum + hyper + tokio

The crate uses `axum` 0.7 (`Router`, `axum::serve(listener, router)`) on
`tokio`, identical to `crates/aperture/src/transport.rs`. No new web
framework is introduced. Route:
`.route("/api/v1/query_range", get(handle_query_range))`.

### 3. Minimal PromQL subset (slice 01: bare metric name only)

After trimming surrounding ASCII whitespace, the `query` MUST match the
Prometheus metric-name production `[a-zA-Z_:][a-zA-Z0-9_:]*` in its
entirety. Anything else is rejected with HTTP 400 `status:error`:

- empty query -> `"empty query: provide a metric name"`,
- a function / aggregation / operator / range-vector / label-matcher ->
  `"unsupported query: ... use a bare metric name"` naming the form.

This is an honest "unsupported" rather than a silent wrong answer. Slice
02 adds a single `{label="value"}` matcher behind the same parser; full
PromQL (operators, functions, aggregations, `rate()`) is deferred to v1.

### 4. Pulse-to-matrix translation

`Vec<(Metric, MetricPoint)>` is grouped into one `PromMatrixEntry` per
distinct merged label set. The label map for a row is the union, in
precedence order:

1. `metric.resource_attributes`,
2. `point.attributes` (win on key collision),
3. `"__name__": metric.name` (always present; authoritative for the name).

Time: `time_unix_nano / 1_000_000_000` as an integer seconds number.
Value: `f64` to a minimal-decimal string; `NaN` -> `"NaN"`; `0.0` -> `"0"`.
Each series' `values` array is ascending in time (Pulse returns ascending).

### 5. start/end/step — raw points at v0

`start`/`end` (float epoch seconds) convert to nanoseconds; the query uses
the half-open `[start, end)` `TimeRange`. v0 returns the stored points that
fall in range WITHOUT step-alignment, downsampling, or staleness handling.
`step` is accepted and ignored at v0. This is acceptable for Prism's
matrix renderer, which maps each `[seconds, value]` pair directly and
tolerates irregular spacing; the validator does not require regular
spacing. Step alignment and staleness are v1.

### 6. Response and error arms

Success / empty: HTTP 200 `{status:'success', data:{resultType:'matrix', result:[...]}}`
(empty array for the empty arm). Parse / unsupported / malformed-bounds /
inverted-bounds: HTTP 400 `{status:'error', error:'<reason>'}`. Pulse
`PersistenceFailed`: HTTP 5xx `status:error` (never a fabricated empty
success). status:error text NEVER echoes a forwarded header value
(symmetry with ADR-0027 §6).

### 7. Tenancy: configured single tenant, fail-closed, behind a seam

A `TenantResolver` port resolves exactly one `aegis::TenantId` per request.
The slice-01 adapter reads `KALEIDOSCOPE_QUERY_TENANT` (fail-closed when
unset or empty), mirroring the gateway's `KALEIDOSCOPE_DEFAULT_TENANT`
posture. Header-based tenancy (`X-Scope-OrgID`, the Mimir/Cortex
convention) or an aegis Bearer token is deferred and lands behind the same
port without changing the query path.

### 8. Earned-Trust probe (wire-then-probe-then-use)

The composition root, before binding the listener, runs `probe()`: a
trivial `query` for a sentinel metric over an empty range against the
resolved tenant, asserting `Ok`. A failure emits
`event=health.startup.refused` and exits non-zero. Mirrors the gateway's
`sink.probe()` and ADR-0041.

## Alternatives considered

### Placement A (rejected): a single binary crate, no lib

A single `main.rs` binary with the parser and translation inline.
For: fewer files. Against: the parser and translation are the only mutable
logic and must be mutation-tested at the project 100% kill-rate; an inline
binary cannot be exercised without spawning a server, and a server-only
test surface is slow and coarse. The lib seam (Decision 1) is the testable
shape. Rejected.

### Placement B (rejected): fold the endpoint into kaleidoscope-gateway

Add the route to the existing ingest binary. For: one process to deploy.
Against: the gateway is the write path (single-writer assumption on the
Pulse directory) and is on the platform plane's ingest hot path; coupling
a read API into it conflates two lifecycles and two scaling profiles, and
muddies the gateway's clear "OTLP -> durable" responsibility. A separate
read service keeps the boundaries clean and lets the reader scale and
deploy independently. Rejected for v0; revisit only if operational
simplicity outweighs the coupling cost.

### PromQL subset A (rejected): a full PromQL engine

Parse and evaluate the full PromQL language (selectors, matchers,
operators, functions, aggregations, range vectors). For: feature parity
with Prometheus. Against: PromQL is a large language; a full engine is far
beyond the read-loop-closure job and would dwarf the rest of the feature.
The honest minimal subset (bare metric name) lets Prism render a real
series now; everything else returns a clear 400. Rejected for v0;
deferred to v1.

### PromQL subset B (rejected): silently best-effort parse

Attempt to extract a metric name from any query and ignore the rest. For:
"it mostly works". Against: a silent wrong answer during an incident is
worse than an honest refusal; an operator pasting `rate(x[5m])` would see a
plotted raw `x` and be misled. Rejected; the boundary is an explicit,
tested 400 (US-03, US-05).

### Tenancy A (rejected): header-based (X-Scope-OrgID) at slice 01

Resolve the tenant from a request header now. For: multi-tenant from day
one. Against: Prism does not pin a tenancy header name, and the write path
resolves a configured default; matching the write-path posture
(configured, fail-closed) is the smallest honest slice. The seam
(Decision 7) makes the header path a non-breaking later swap. Rejected for
slice 01.

### Tenancy B (rejected): default to all tenants when none resolves

Return every tenant's series when no tenant is configured. For:
"convenient in dev". Against: this is a cross-tenant data leak by
construction; the write path fails closed and the read path must match.
Rejected; fail-closed is mandatory (US-04).

## Consequences

### Positive

- **Contract round-trips by construction**. The output is shaped to pass
  Prism's own `isPromSuccess` / `isPromError`; a contract test asserts it.
- **The scope boundary is executable**. Every unsupported query form is a
  tested 400; no partial or fabricated answers.
- **Tenancy is fail-closed and swappable**. Zero cross-tenant leak; the
  mechanism can evolve behind `TenantResolver` without touching the query
  path.
- **Reuse over reinvention**. axum/hyper/tokio, the Pulse store, the
  TenantId vocabulary, the lib+binary split, and the probe posture are all
  existing platform assets.
- **Earned-Trust at startup**. A store that opens but cannot be read
  refuses to start, rather than serving fabricated empties.

### Negative

- **A new crate to maintain**. Mitigated: it is thin, and the lib+binary
  split is the established platform shape.
- **No re-stepping at v0**. Charts show raw points; an operator expecting
  Prometheus-exact step alignment sees irregular spacing. Mitigated:
  Prism's renderer tolerates this; v1 adds alignment. Documented in the
  feature wave-decisions.md deferral list.
- **`__name__` precedence is a convention to hold**. If a resource or point
  attribute were ever literally named `__name__`, the metric name wins;
  this is the Prometheus convention and is asserted by a translation test.

### Trade-off summary

The endpoint is intentionally minimal: a bare-name parser, a raw-point
translation, a configured fail-closed tenant. The trade-off is "Prometheus
feature parity" against "an honest, testable, shippable read-loop closure
now". v0 takes the latter and records every deferral.

## External-integration handoff

Per principle 10, `/api/v1/query_range` is the consumer-driven contract
boundary with Prism. ADR-0027 records the Prism (consumer) side. The
provider (this crate) handoff to `@nw-platform-architect` (Apex):

> **External integrations requiring contract tests**:
> - **Prism query client** (`/api/v1/query_range`): the provider MUST
>   satisfy Prism's `isPromSuccess` (success + empty arms) and
>   `isPromError` (400 parse-error arm) and the 5xx transport arm.
>   Recommended: consumer-driven contract via the four pinned response
>   shapes asserted against Prism's own validators in the CI acceptance
>   stage (Pact-JS or a container-fixture posture, Apex's choice).

## Verification

- A contract test asserts the success, empty, parse-error, and 5xx shapes
  pass Prism's `isPromSuccess` / `isPromError`.
- Acceptance tests, one per unsupported query form, assert the 400 arm
  (US-03, US-05).
- A tenant-isolation test (two-tenant fixture) asserts zero cross-tenant
  leak; a no-tenant fixture asserts fail-closed refusal (US-04).
- A translation test pins the label-set merge, the `__name__` precedence,
  the seconds conversion, the minimal-decimal value, and `"NaN"`.
- **Earned-Trust probe enforcement (three orthogonal layers)**: (a) subtype
  check at the composition-root boundary (the store is used through the
  `MetricStore` port; the probe consumes that port); (b) an AST structural
  pre-commit check that the binary calls `probe()` before binding the
  listener; (c) a behavioural gold-test exercising a store double that lies
  (open succeeds, read returns `PersistenceFailed`) and asserting startup
  refuses with `event=health.startup.refused`. A single-layer bypass is
  caught by at least one of the other two.
- Mutation testing: `cargo mutants` scoped to `crates/query-api/src/` at
  the project 100% kill-rate gate (ADR-0005 Gate 5; CLAUDE.md). Flagged to
  Apex as `gate-5-mutants-query-api`.
```
