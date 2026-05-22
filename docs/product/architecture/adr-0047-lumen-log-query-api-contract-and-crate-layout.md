# ADR-0047 — lumen log-query-api response contract and crate layout

- **Status**: Accepted
- **Date**: 2026-05-22
- **Author**: `nw-solution-architect` (Morgan)
- **Feature**: `lumen-query-api-v0`
- **Supersedes**: none
- **Superseded by**: none
- **Related**: ADR-0042 (the metrics query-api contract, PromQL subset, and
  fail-closed tenancy this slice takes as its directly analogous precedent;
  cited as the framing precedent, NOT modified). ADR-0043 (the Prism
  same-origin / `/api/v1` reconciliation; the static-serving posture deferred
  here for a future prism log UI; cited, NOT modified).

## Context

Kaleidoscope stores logs durably in the `lumen` crate
(`FileBackedLogStore`, `crates/lumen/src/file_backed.rs:211`); the gateway
writes them, but nothing reads them back over HTTP. This feature is the read
half of the logs pillar, the exact analogue of what `query-range-api-v0`
(ADR-0042) did for metrics. Slice 01 is one thin walking skeleton: given a
resolved tenant and a half-open window `[start, end)`, return the `LogRecord`s
that fall in the window as JSON, read from the real durable lumen store.

The store surface is verified and FIXED for this slice (`crates/lumen/src/store.rs:84`):

```text
LogStore::query(&self, tenant: &TenantId, range: TimeRange)
    -> Result<Vec<LogRecord>, LogStoreError>
```

Per-tenant isolation; ascending `observed_time_unix_nano` order; half-open
`[start, end)`. `LogRecord` (`record.rs:44`) carries `observed_time_unix_nano`,
`severity_number`, `severity_text`, `body`, `attributes`,
`resource_attributes`, and optional `trace_id`/`span_id`, and ALREADY derives
`serde::Serialize`. `LogStoreError::PersistenceFailed { reason }` is the only
typed failure (`store.rs:49`); the in-memory adapter never returns it, so the
5xx arm is exercised with a failing store double.

DISCUSS pinned the BEHAVIOUR (in-window records as JSON, fail-closed tenancy,
calm empty 200, malformed-window 400, store-failure 5xx, faithful field
fidelity) and FLAGGED three decisions to DESIGN, which this ADR resolves: the
response contract (Loki-shaped vs plain JSON), the placement (new crate vs
extend `query-api`), and the on-the-wire request shape and route. Logs are NOT
metrics: there is no PromQL, no metric name, and no query language in slice 01;
the query inputs are a tenant and a window only. This is the deliberate
divergence from the metrics endpoint and the reason ADR-0042's selector grammar
and matrix translation have no analogue here.

ADRs in this repository are immutable (superseded, never edited). ADR-0042 and
ADR-0043 are Accepted and referenced as precedents, not modified. ADR-0047 is
the next free number (the highest existing was 0046, verified).

## Decision

### 1. Response contract: a plain, explicit JSON array of `LogRecord`s (FLAG 1)

The success arm is a plain JSON array of the in-window `LogRecord`s, in the
store's ascending `observed_time_unix_nano` order, each record serialised
faithfully via the field set `LogRecord` already derives with `serde::Serialize`
(`record.rs:44`): `observed_time_unix_nano` (number), `severity_number`,
`severity_text`, `body`, `attributes` (object), `resource_attributes` (object),
`trace_id`, `span_id`. The empty arm is the empty array `[]` with HTTP 200. No
envelope wraps the array at v0.

```text
GET .../logs?start=<epoch_seconds>&end=<epoch_seconds>  ->  200
[
  {
    "observed_time_unix_nano": 1716200005000000000,
    "severity_number": 17,
    "severity_text": "ERROR",
    "body": "checkout: payment timeout",
    "attributes": { "http.status_code": "503" },
    "resource_attributes": { "service.name": "checkout" },
    "trace_id": [ ... 16 bytes ... ],
    "span_id": [ ... 8 bytes ... ]
  }
]

empty match / unknown tenant  ->  200  []
```

Loki-shaping (Grafana `streams`) is REJECTED for v0 because there is no prism
log consumer yet pinning a contract (the metrics endpoint, by contrast, had its
shape PINNED by Prism's existing client, ADR-0042 / ADR-0027). The most honest
and simple contract is to return the records the store actually holds, in their
native OTLP-shaped field set, with no lossy projection into a streams envelope.
When a consumer (a prism log panel or Grafana) needs Loki-shaping, it arrives as
a documented later slice behind the same route, a non-breaking additive
content-negotiation or a sibling route; this slice does not pre-build it.

The error arm reuses the metrics endpoint's shape EXACTLY for cross-pillar
symmetry: `{status:"error", error:"<reason>"}` at the relevant status code. The
reason names the fault (the invalid window, a backend read failure) and NEVER
echoes a forwarded header or credential value (DD redaction symmetry with
ADR-0042 Decision 6 and ADR-0027 §6). The success arm is the bare array, not
`{status:"success", ...}`, because logs have no Prometheus validator to satisfy
and a bare array is the simplest honest shape; the error arm borrows the
metrics envelope because an error has no records to carry and the shared shape
costs nothing.

### 2. Placement: a NEW crate `log-query-api`, lib + thin binary (FLAG 2)

A new workspace crate `crates/log-query-api`, mirroring the `query-api`
lib+binary split: a `[lib]` exposing one driving port `router(store, tenant)`
over the `lumen::LogStore` trait and an `Option<TenantId>` (fail-closed seam),
plus a thin `[[bin]]` composition root that opens the durable
`FileBackedLogStore`, resolves the tenant, runs the Earned-Trust probe, and
binds the axum listener. The existing `query-api` crate is NOT extended.

The MANDATORY reuse analysis (full table in the feature wave-decisions.md)
found that the HTTP SCAFFOLDING is reusable in SHAPE but not in CODE: `query-api`
is metrics-domain-specific throughout (its `MetricStore` port, the PromQL
`selector` parser, the `matrix` translator, the Prometheus `{status,data}`
envelope). Logs are a different domain with a different store trait
(`LogStore`), a different record type (`LogRecord`), and a different contract (a
plain array, Decision 1). Folding logs into `query-api` would mix two domains
and two response envelopes in one crate for the sake of sharing a dozen lines of
axum boilerplate. The reuse is of PATTERN (the lib+binary split, the
fail-closed `Option<TenantId>` router seam, the `error_response` shape, the
epoch-seconds bounds parser, the tower `oneshot` test posture, the
wire-then-probe-then-use composition root), reproduced in the new crate, not of
the metrics types.

**Extract-vs-duplicate call: DUPLICATE the minimum, extract NOTHING in this
slice.** The genuinely shared pieces are tiny and stable: the fail-closed
`Option<TenantId>` refusal, the `error_response` JSON helper, and the
epoch-seconds `parse_time_range` (which produces a `lumen::TimeRange` here
rather than a `pulse::TimeRange`, so it is not even type-identical). Extracting
a shared `query-http-common` crate now would be premature: it would couple two
crates through a third for ~30 lines, and the two `TimeRange` types differ.
These few lines are re-implemented in `log-query-api` and mutation-tested in
place. A future extraction is a clean refactor once a THIRD HTTP read pillar
appears and the shared surface has proven its shape; this slice does not pay
that cost on speculation.

### 3. Route and request shape: `GET /api/v1/logs?start=&end=`, epoch seconds (FLAG 3 / RED CARD 4)

The route is `GET /api/v1/logs`, sibling to the metrics endpoint's
`/api/v1/query_range` under the same `/api/v1` prefix (ADR-0043), so an operator
and any future same-origin prism log panel reach it with no extra mapping. The
window arrives as query-string parameters `start` and `end` in epoch SECONDS
(float-tolerant, mirroring the metrics endpoint for operator muscle memory),
converted exactly to the half-open `[start, end)` u64-nanosecond
`lumen::TimeRange`. Query-string is chosen over path segments (e.g.
`/logs/{start}/{end}`) because the window is a filter on a collection, not a
resource identity, and query-string mirrors the metrics endpoint the operator
already knows.

The status mapping:

| Outcome | Condition | HTTP | Body |
|---|---|---|---|
| Success | `LogStore::query` returns a non-empty `Vec` | 200 | JSON array of `LogRecord`s, ascending `observed_time` |
| Calm empty | `LogStore::query` returns `Ok(Vec::new())` (empty window OR unknown tenant) | 200 | `[]` |
| Bad window | non-numeric bound, or `start > end` | 400 | `{status:"error", error:"<names the invalid window>"}`; no store query is run |
| Fail-closed | no tenant resolves | 401 | `{status:"error", error:"no tenant resolvable: ..."}`; refused before the store |
| Store failure | `LogStore::query` returns `PersistenceFailed` | 500 | `{status:"error", error:"the backing log store could not be read"}`; never a fabricated empty |

The half-open rule is the store's: a record at exactly `start` is included, a
record at exactly `end` is excluded. The error text never echoes a forwarded
header value.

### 4. Tenancy: configured single tenant, fail-closed, behind the router seam (RED CARD 3)

The slice-01 adapter resolves exactly one `aegis::TenantId` from
`KALEIDOSCOPE_LOG_QUERY_TENANT` (fail-closed when unset or empty), mirroring the
metrics read path's `KALEIDOSCOPE_QUERY_TENANT` (ADR-0042 Decision 7) and the
gateway's `KALEIDOSCOPE_DEFAULT_TENANT`. The router takes an `Option<TenantId>`:
`None` is "no tenant resolvable" and every request is refused (401). Header
tenancy (`X-Scope-OrgID`) or an aegis Bearer token is deferred and lands behind
the same seam without touching the query path.

### 5. The lumen `LogStore` trait is UNCHANGED

The endpoint reads through the EXISTING `LogStore::query(&tenant, range)`
(`store.rs:84`) against the real `FileBackedLogStore`. No method is added,
removed, or re-signed on the trait. `query_with(predicate)` exists but is NOT
used in slice 01 (severity/attribute/body filtering is declared out of scope).

### 6. Earned-Trust probe (wire-then-probe-then-use)

The composition root, before binding the listener, runs `probe()`: a trivial
`query` over an empty range against the resolved tenant, asserting `Ok`. A
`None` tenant is the fail-closed refusal; a store error is a read refusal. A
failure emits `event=health.startup.refused` and exits non-zero. The
three-orthogonal-layer enforcement (subtype at the composition-root boundary,
AST pre-commit that the binary probes before binding, behavioural gold-test with
a lying store double) mirrors ADR-0042 Decision 8, reproduced for the new crate.

## Alternatives considered

### Response contract A (rejected): Loki-shaped (Grafana `streams`)

Shape the response as Loki's `{status, data:{resultType:"streams", result:[{stream:{...labels}, values:[[ts, line], ...]}]}}`.
For: instant Grafana Explore / Loki-datasource compatibility. Against: there is
NO prism log consumer pinning this contract yet (unlike the metrics endpoint,
whose shape Prism pinned), and Loki-shaping is LOSSY against the OTLP `LogRecord`
field set: it flattens `severity_number`, structured `attributes`, and
`trace_id`/`span_id` into a label/line projection, the exact fidelity US-01
example 3 requires the response to preserve. Adopting it now would invent a
consumer contract speculatively and lose fields. Rejected for v0; it arrives
behind the same route when a real consumer needs it.

### Response contract B (rejected): a platform-consistent `{status, data}` envelope around the array

Wrap the array in the metrics endpoint's `{status:"success", data:{result:[...]}}`
for cross-pillar symmetry. For: a single success envelope across both pillars.
Against: the metrics envelope exists to satisfy Prism's `isPromSuccess`
validator; logs have no such validator, so the envelope is pure ceremony around
a bare array, and it would imply a `resultType` the logs domain does not have.
The error arm DOES borrow the `{status:"error"}` shape (Decision 1) because an
error carries no records and the shared shape is free, but the success arm stays
a bare array, the simplest honest shape. Rejected for the success arm.

### Placement A (rejected): extend the existing `query-api` crate

Add the `/api/v1/logs` route to `query-api`. For: reuses the axum boilerplate,
the bounds parser, and the `error_response` helper in place; one crate, one CI
gate, one tag. Against: `query-api` is metrics-domain-specific end to end (the
`MetricStore` port, the PromQL `selector`, the `matrix` translator, the
Prometheus envelope); logs use a different store trait (`LogStore`), a different
record type, and a different contract (a plain array). Folding logs in mixes two
domains and two envelopes in one crate to share a dozen lines of boilerplate,
muddying both. The reuse worth having is of PATTERN, reproduced cheaply in a new
crate. Rejected; the domain separation outweighs the boilerplate saving.

### Placement B (rejected): extract a shared `query-http-common` crate now

Create a new crate holding the fail-closed seam, `error_response`, and the
bounds parser, depended on by both `query-api` and `log-query-api`. For: no
duplication. Against: the shared surface is ~30 lines and the two `TimeRange`
types differ (`pulse::TimeRange` vs `lumen::TimeRange`), so the parser is not
even type-identical; extracting now couples two crates through a third on
speculation, before a third pillar has proven the shared shape. Rejected for
this slice; revisit as a clean refactor when a third HTTP read pillar appears.

### Request shape A (rejected): the window in the path (`/api/v1/logs/{start}/{end}`)

Carry `start`/`end` as path segments. For: a "RESTful" resource-shaped URL.
Against: the window is a filter over a collection, not a resource identity, and
path segments diverge from the metrics endpoint the operator already knows.
Rejected; query-string mirrors the metrics endpoint and reads as a filter.

## Consequences

### Positive

- **The most honest, lowest-ceremony contract.** A plain array of the records
  the store holds, in their native OTLP field set, with no lossy projection and
  no speculative consumer envelope. Field fidelity (US-01 example 3) is satisfied
  by `LogRecord`'s existing `Serialize`, with no hand-written mapping to drift.
- **Clean domain boundaries.** Logs and metrics stay separate crates; neither
  envelope nor store trait leaks into the other. `query-api` is untouched.
- **Pattern reuse without coupling.** The lib+binary split, the fail-closed seam,
  the `error_response` shape, the epoch-seconds parser, the tower `oneshot` test
  posture, and the wire-then-probe-then-use root are all reproduced from the
  proven metrics precedent, cheaply, in the new crate.
- **The lumen trait is untouched.** The read path rides the existing
  `LogStore::query`; zero blast radius on the store and its other callers.
- **Fail-closed tenancy and honest three-way outcomes.** Empty is a calm 200
  `[]`, a bad window is a 400 that names the fault, a store failure is a 500 that
  never fabricates an empty success; the error text never leaks a forwarded
  header.

### Negative

- **A new crate to maintain, with a new CI gate and a new graduation tag.** A new
  `gate-5-mutants-log-query-api` job and a new per-crate tag at graduation (a
  DEVOPS / graduation matter; see the wave-decisions DEVOPS handoff). Mitigated:
  the lib+binary split is the established platform shape and the crate is thin.
- **~30 duplicated lines** (the fail-closed seam, `error_response`, the bounds
  parser) across `query-api` and `log-query-api`. Mitigated: deliberately small
  and mutation-tested in place; a shared crate is a clean later refactor once a
  third pillar justifies it, NOT speculatively now.
- **No Loki-shaping yet.** A Grafana operator cannot point a Loki datasource at
  this endpoint today. Mitigated: there is no such consumer yet, the records are
  carried losslessly so a later projection is additive, and the deferral is
  recorded.

### Trade-off summary

The slice trades a speculative Loki envelope and a premature shared crate for an
honest plain-array contract and a clean per-domain crate, buying lossless field
fidelity and clean boundaries at the cost of one new CI gate, one new tag, and
~30 deliberately duplicated, mutation-tested lines. Both costs are recorded so a
future reader understands they were chosen, not overlooked.

## External-integration handoff

No NEW external network integration. The endpoint reads the in-process durable
`lumen::FileBackedLogStore` through the `LogStore` trait; lumen is a first-party
library, not a network service. There is no pinned external consumer contract
for the logs response yet (no prism log panel exists), which is precisely why
Decision 1 chose the plain array. When a prism log panel or a Grafana datasource
becomes a real consumer, that boundary acquires a consumer-driven contract at
that time; this slice introduces none.

## Verification

- Acceptance tests via the tower `oneshot` pattern: ingest in-window and
  out-of-window records into a real `FileBackedLogStore`, query, assert only the
  in-window records return in ascending `observed_time` order (US-01); the
  half-open boundary (record at `start` included, at `end` excluded); full field
  fidelity (every `LogRecord` field round-trips, none dropped or renamed).
- The calm empty arm: empty window and unknown tenant both yield 200 `[]`
  (US-02).
- Tenant isolation: a two-tenant fixture asserts zero cross-tenant leak; a
  no-tenant fixture asserts the fail-closed 401 (US-03).
- The honest three-way distinction: a non-numeric bound and an inverted window
  each yield 400 naming the fault with no store query run; a `PersistenceFailed`
  store double yields 500, never a fabricated empty; a redaction test asserts the
  error text never contains a forwarded `Authorization` value (US-04).
- **Earned-Trust probe enforcement (three orthogonal layers)**: (a) subtype check
  at the composition-root boundary (the store is used through the `LogStore`
  port; the probe consumes that port); (b) an AST pre-commit check that the
  binary calls `probe()` before binding the listener; (c) a behavioural gold-test
  with a store double that lies (open succeeds, query returns `PersistenceFailed`)
  asserting startup refuses with `event=health.startup.refused`.
- Mutation testing: `cargo mutants` scoped to `crates/log-query-api/src/` via
  `--in-diff` at the project 100% kill-rate gate (ADR-0005 Gate 5; CLAUDE.md).
  Flagged to Apex as a NEW `gate-5-mutants-log-query-api`. Primary targets: the
  half-open boundary, the empty-vs-error distinction, the bounds parser, and the
  fail-closed refusal.
