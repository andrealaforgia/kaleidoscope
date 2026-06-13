<!-- markdownlint-disable MD024 -->

# User Stories — `consolidated-runtime-v0`

> Persona note: the consumer of this runtime is the **experimenter** — Andrea running
> Kaleidoscope locally to try it, a contributor evaluating a change, and the integration suite
> / CI exercising the loop. C1 is built by AI agents; the personas below are *users* of the
> consolidated runtime, not its builders. House style: British English, no human-effort
> estimation. Scenario titles describe what the user achieves, never how the system implements
> it.

---

## System Constraints

These apply to every story below and are not repeated in each:

1. **Single-process consolidation** (W1). One process builds ONE store per signal, wraps it in
   `Arc`, and hands the SAME instance to both the ingest sink and the query router, so a write
   is immediately visible to a read. (Andrea-veto flag in `wave-decisions.md` W1 — if vetoed,
   the mechanism becomes a WAL-watch adapter but these stories' observable outcomes are
   unchanged.)
2. **Additive** (W2). C1 adds a consolidated entry point; the existing `kaleidoscope-gateway`,
   `query-api`, `log-query-api`, and `trace-query-api` binaries are NOT removed. Whether C1 is a
   new binary or an extension of the gateway is a DESIGN decision; these stories are neutral on
   it and say only "the consolidated runtime".
3. **Solution-neutral**. Stories state observable outcomes (a metric sent at T is queryable at
   T+epsilon, no restart), never technology or store internals. Port numbers, the exact binary
   name, and the concurrency mechanism are DESIGN's to fix.
4. **Minimal-friction local posture** (W3). For the experiment: auth OFF everywhere; one shared
   default tenant set for ingest (`KALEIDOSCOPE_DEFAULT_TENANT`) and for the query routers
   (`KALEIDOSCOPE_*_QUERY_TENANT`) to the SAME value; one shared pillar root; no tokens, no TLS.
5. **No regression** (W4). Existing tenant isolation, the optional fail-closed per-request
   read-auth (`router_with_auth`, ADR-0074), the gateway's ingest-auth posture, and the
   per-record fsync durability of the file-backed stores all continue to hold.
6. **One writer** (state assessment §4). The consolidated runtime owns its pillar root; it must
   not be co-run against a separate gateway writing the same root.
7. **Default ports** (illustrative, DESIGN confirms): ingest gRPC 4317 / HTTP 4318; query
   metrics 9090 (`/api/v1/query_range`), logs 9091 (`/api/v1/logs`), traces 9092
   (`/api/v1/traces` and `/api/v1/traces/by_id`).

---

## US-01 — Send a metric and immediately query it back, no restart

### Elevator Pitch

- **Before**: Andrea brings up the query API, sends a metric, queries it — and gets nothing,
  because the query process froze its store snapshot at its own startup and never re-reads. He
  has to restart the query API to see data he just sent. The natural experiment loop fails by
  construction.
- **After**: Andrea starts the consolidated runtime (one command, empty store), pushes one OTLP
  metric `request_count` for tenant `acme` to the ingest endpoint, then runs
  `curl ":9090/api/v1/query_range?query=request_count&start=..&end=.."` and the point he just
  sent comes back in the response body — same process, within about a second, **no restart of
  anything**. The acceptance command that demonstrates it is an integration test that ingests
  then queries in one process; the runtime entry point is the one-command consolidated runtime.
- **Decision enabled**: Andrea decides that Kaleidoscope is now something he can actually
  experiment with — send, look, iterate — which unblocks the rest of the consolidation roadmap
  (the run story C2, the telemetry generator C3, the getting-started doc C4).

### Problem

Andrea wants to try Kaleidoscope: bring it up, send a metric, and see it. Today ingest and the
metrics query API are separate processes sharing only a filesystem path, and the query side
loads its in-memory map once at `open()` and never re-reads
(`docs/analysis/consolidation-state-2026-06.md` §4). So a query API started before the metric
arrives returns empty until it is restarted. "Bring up the stack, send a metric, look" — the
single most basic experiment — does not work. This is the load-bearing gap C1 fixes.

### Who

- **Andrea, local experimenter**: runs the stack on his machine to learn what Kaleidoscope
  does; wants send-then-see to just work without a restart dance.
- **Kaleidoscope contributor**: evaluating a change end to end; needs to push a metric and
  confirm it is queryable immediately.
- **Integration suite / CI**: asserts the live-visibility property mechanically so it cannot
  silently regress.

### Solution

A consolidated runtime that builds the metric store once and shares the SAME instance between
the OTLP ingest sink and the `query-api` router (which already accepts an injected
`Arc<dyn MetricStore + Send + Sync>` — `crates/query-api/src/lib.rs:122`). A metric ingested
through the sink is therefore immediately visible to a query through the router, with no
restart and no second process. The runtime binds the ingest ports and the metrics query port
on one process.

### Domain Examples

#### 1: Andrea sends one counter and sees it a second later

Andrea runs the consolidated runtime with `KALEIDOSCOPE_DEFAULT_TENANT=acme` and the metrics
query tenant also `acme`, over `/tmp/kal`. He pushes an OTLP metric `request_count` with value
`1` at time T to `:4318`. At T+epsilon he runs
`curl ":9090/api/v1/query_range?query=request_count&start=T-60&end=T+60"` and the response is
`{"status":"success","data":{"resultType":"matrix","result":[{"metric":{"__name__":"request_count",...},"values":[[T,"1"]]}]}}`.
He did not restart anything.

#### 2: A contributor queries before sending anything and gets a clean empty answer

A contributor starts the runtime fresh and, before pushing any telemetry, runs the same
`query_range` request. The response is `{"status":"success","data":{"resultType":"matrix","result":[]}}`
— an empty success, HTTP 200, not a 500 and not a stale value. They then push `request_count`
and the next query returns it.

#### 3: The integration suite proves send-then-query in a single process

The integration suite, in one process, builds the shared metric store, ingests a
`request_count` point for tenant `acme`, then issues a `query_range` against the same runtime
and asserts the point is returned — all without dropping/reopening the store or starting a
second process. This is the mechanical guard that the frozen-snapshot bug cannot return.

### UAT Scenarios (BDD)

#### Scenario: A metric is queryable immediately after it is sent

```
Given the consolidated runtime is running with an empty metric store for tenant "acme"
When Andrea sends an OTLP metric "request_count" with value 1 at time T for tenant "acme"
And Andrea queries "/api/v1/query_range" for "request_count" over a window covering T
Then the response status is success
And the result contains a point with value 1 at time T
And Andrea did not restart any process between sending and querying
```

#### Scenario: Querying an empty store returns an empty success, not an error

```
Given the consolidated runtime is running and no telemetry has been sent
When Andrea queries "/api/v1/query_range" for "request_count" over any valid window
Then the response status is success
And the result is empty
And the response is not an error
```

#### Scenario: A metric sent after the runtime started is visible without a restart

```
Given the consolidated runtime started with an empty metric store before any telemetry arrived
When Andrea sends an OTLP metric "request_count" for tenant "acme" some time after startup
And Andrea queries "/api/v1/query_range" for "request_count" over a window covering the send time
Then the result contains the metric just sent
And no restart of the runtime or any component was required to see it
```

#### Scenario: The runtime serves ingest and metrics query from one process

```
Given the consolidated runtime has been started with one command
Then the OTLP ingest endpoint accepts a metric push
And the metrics query endpoint answers a query_range request
And both are served by the same single running process
```

### Acceptance Criteria

- [ ] After an OTLP metric is ingested, a `query_range` for that metric over a window covering
  its timestamp returns the metric, with no restart of any component.
- [ ] A `query_range` against a store that has received no telemetry returns
  `status:success` with an empty result (HTTP 200), never an error.
- [ ] A metric ingested AFTER the runtime started (the runtime began with an empty store) is
  returned by a subsequent query without any restart.
- [ ] The ingest endpoint and the metrics query endpoint are served by one process.
- [ ] The ingest sink and the metrics query router operate over the SAME store instance
  (observable: the write-then-read returns the value; verifiable by the single-process
  integration test).

### Outcome KPIs

- **Who**: the experimenter (Andrea / a contributor / the integration suite) running the
  consolidated runtime.
- **Does what**: queries back a metric they just sent, without restarting any process.
- **By how much**: from impossible today (0% without a restart) to 100% of send-then-query
  attempts returning the data, with freshness within 1 second (p95).
- **Measured by**: a single-process integration/acceptance test that ingests then queries and
  asserts the value returns; manual one-command experiment.
- **Baseline**: 0% — today the loop fails by construction; the metric is visible only after a
  query-process restart.

### Technical Notes

- Reuse seam: `query_api::router(store: Arc<dyn MetricStore + Send + Sync>, tenant, static_dir)`
  (`crates/query-api/src/lib.rs:122`) already takes an injected store.
- The gateway already builds `Arc::new(FileBackedMetricStore::open(..))` and `Arc::clone`s it
  into the sink (`crates/kaleidoscope-gateway/src/main.rs:84-96`); the only new move is to
  `Arc::clone` the SAME instance into the router.
- `MetricStore::ingest(&self, ..)` and `query(&self, ..)` both take `&self`
  (`crates/pulse/src/store.rs:72-99`); the store serialises through an interior `Mutex`
  (`crates/pulse/src/file_backed.rs:81`). No concurrency change is expected; DESIGN confirms.
- DESIGN decides new-binary-vs-extend-gateway and the port configuration. Tests should bind
  ephemeral ports and sweep+retry (fixed-port 4317/4318 flake, project memory).

### Dependencies

None upstream. This is the feature walking skeleton; US-02, US-03, US-04, US-05 build on its
composition pattern.

---

## US-02 — Tenant isolation holds in the consolidated process

### Elevator Pitch

- **Before**: Ingest and query were separate processes; now C1 puts them in one process sharing
  one store instance. Before trusting that shape, the experimenter needs to know that running
  ingest and query together has NOT weakened tenant isolation — that `globex` still cannot read
  `acme`'s data.
- **After**: Andrea ingests `request_count` for tenant `acme`, then queries the same metrics
  endpoint scoped to tenant `globex`, and gets an empty success — `acme`'s data is not leaked
  across the tenant boundary even though both signals now flow through one process. He sees the
  isolation hold exactly as it did with separate processes.
- **Decision enabled**: Andrea decides the consolidated shape is safe to run, including with
  more than one tenant present, so consolidation is not a security regression.

### Problem

The state assessment and the read-path-auth feature establish that every read is scoped by an
aegis `TenantId`. Collapsing ingest and query into one process must preserve that boundary. If
sharing a store instance accidentally widened reads (e.g. a query ignoring the tenant key), C1
would turn a freshness fix into a data-leak regression — the worst possible trade.

### Who

- **Andrea / a contributor** running a local stack with more than one tenant's data present;
  needs cross-tenant reads to stay empty.
- **A security-minded reviewer** deciding whether the consolidated runtime is acceptable to run
  where multiple tenants' telemetry could coexist.

### Solution

The consolidated runtime resolves the query tenant exactly as the standalone query APIs do (env
tenant in the local posture; the optional fail-closed bearer via `router_with_auth` when
configured), and the store's `query(tenant, ..)` continues to key by `(tenant, metric)`.
Sharing the store instance does not change the tenant key; a query for `globex` scans only
`globex`'s series.

### Domain Examples

#### 1: globex cannot see acme's metric

Andrea ingests `request_count`=1 for tenant `acme`. He then issues a `query_range` for
`request_count` with the query tenant resolved as `globex`. The result is empty success.
`acme`'s point is invisible to `globex`.

#### 2: The matching tenant does see its own metric

In the same runtime, a `query_range` for `request_count` scoped to `acme` returns the point.
Isolation excludes the wrong tenant without hiding the right one.

#### 3: A record without a tenant is handled by the configured default, fail-closed otherwise

Andrea runs with `KALEIDOSCOPE_DEFAULT_TENANT=acme`; an OTLP record arriving without a
`tenant.id` resource attribute is attributed to `acme` (the configured default) and is then
queryable under `acme`. With no default tenant configured, such a record is refused at ingest
(fail-closed), unchanged from the gateway's existing behaviour.

### UAT Scenarios (BDD)

#### Scenario: A query for one tenant never returns another tenant's data

```
Given the consolidated runtime holds a metric "request_count" ingested for tenant "acme"
When a query for "request_count" is made scoped to tenant "globex"
Then the response status is success
And the result is empty
And none of "acme"'s data is returned
```

#### Scenario: A query for the owning tenant returns its own data

```
Given the consolidated runtime holds a metric "request_count" ingested for tenant "acme"
When a query for "request_count" is made scoped to tenant "acme"
Then the result contains the metric ingested for "acme"
```

#### Scenario: An untenanted record uses the configured default tenant

```
Given the consolidated runtime is configured with default tenant "acme"
When an OTLP metric arrives without a tenant identifier
Then the metric is attributed to tenant "acme"
And a query scoped to "acme" returns it
```

### Acceptance Criteria

- [ ] A query scoped to a tenant returns only that tenant's data; a query scoped to a different
  tenant than the one that ingested the data returns an empty success.
- [ ] A query scoped to the ingesting tenant returns that tenant's data.
- [ ] With a default tenant configured, an untenanted record is attributed to the default and
  is queryable under it; with no default configured, ingest of an untenanted record is refused
  (fail-closed), unchanged from today.
- [ ] The existing per-request read-auth path (`router_with_auth` with a validator) remains
  available and fail-closed when configured; the local experiment posture leaves it off.

### Outcome KPIs

- **Who**: a multi-tenant-aware operator/reviewer of the consolidated runtime.
- **Does what**: confirms cross-tenant reads return nothing in the consolidated process.
- **By how much**: 0 cross-tenant leaks — 100% of cross-tenant reads return empty.
- **Measured by**: acceptance test ingesting for one tenant and reading as another; asserts
  empty.
- **Baseline**: separate-process tenant isolation already holds; the target is no regression
  (still 0 leaks) under consolidation.

### Technical Notes

- aegis `TenantId` and the store's `(tenant, metric_name)` key are unchanged; this story asserts
  the property survives store-instance sharing.
- This is a guardrail story for the slice-1 north-star outcome (US-01); it shares the metrics
  composition.

### Dependencies

US-01 (shares the metrics composition root).

---

## US-03 — Send a log and immediately query it back, no restart

### Elevator Pitch

- **Before**: After US-01/US-02 the metrics loop is live, but logs still go through a separate
  query process that freezes its snapshot at startup, so a freshly-ingested log is invisible
  until a restart.
- **After**: Andrea pushes an OTLP log record with body `"checkout failed: card declined"` for
  tenant `acme` to the ingest endpoint, then runs `curl ":9091/api/v1/logs?.."` against the
  same process and the log line comes back — no restart. The acceptance command is a
  single-process ingest-then-query test for logs.
- **Decision enabled**: Andrea decides that the live-visibility property generalises beyond
  metrics — logs behave identically — so the consolidated runtime is consistent across signals.

### Problem

Logs have a query API binary (`log-query-api`, `/api/v1/logs`, :9091) but the same
frozen-snapshot gap as metrics. Until the log store is shared in-process between sink and
router, "send a log, see it" fails the same way.

### Who

- **Andrea / a contributor** debugging by sending a log and expecting to read it back live.
- **Integration suite / CI** asserting the logs live-visibility property.

### Solution

Share the single `FileBackedLogStore` instance between the ingest sink and the
`log-query-api` router, which already accepts an injected `Arc<dyn LogStore + Send + Sync>`
(`crates/log-query-api/src/lib.rs:95`). Bind the logs query port on the same process.

### Domain Examples

#### 1: A declined-checkout log read back live

Andrea sends an OTLP log `"checkout failed: card declined"` for tenant `acme` at time T. He
then queries `/api/v1/logs` over a window covering T and the log record is in the response,
without a restart.

#### 2: Query before any log returns empty success

Before sending any logs, Andrea queries `/api/v1/logs`; the response is an empty success
(HTTP 200, no records), not an error.

#### 3: Logs are tenant-scoped in the consolidated process

A log ingested for `acme` is not returned by a `/api/v1/logs` query scoped to `globex`.

### UAT Scenarios (BDD)

#### Scenario: A log is queryable immediately after it is sent

```
Given the consolidated runtime is running with an empty log store for tenant "acme"
When Andrea sends an OTLP log "checkout failed: card declined" at time T for tenant "acme"
And Andrea queries "/api/v1/logs" over a window covering T
Then the response status is success
And the result contains the log "checkout failed: card declined"
And no restart was required
```

#### Scenario: Querying logs before any are sent returns an empty success

```
Given the consolidated runtime is running and no logs have been sent
When Andrea queries "/api/v1/logs" over any valid window
Then the response status is success
And the result is empty
And the response is not an error
```

#### Scenario: A log for one tenant is not returned to another tenant

```
Given the consolidated runtime holds a log ingested for tenant "acme"
When a "/api/v1/logs" query is made scoped to tenant "globex"
Then the result is empty
And none of "acme"'s logs are returned
```

### Acceptance Criteria

- [ ] After a log is ingested, a `/api/v1/logs` query over a window covering its timestamp
  returns it, with no restart.
- [ ] A `/api/v1/logs` query against a store with no logs returns `status:success` with an
  empty result.
- [ ] A log query scoped to a non-owning tenant returns empty; scoped to the owning tenant
  returns the log.
- [ ] The logs query endpoint is served by the same process as ingest.

### Outcome KPIs

- **Who**: the experimenter sending and reading logs in the consolidated runtime.
- **Does what**: queries back a log they just sent, without a restart.
- **By how much**: 0% → 100% of send-then-query log attempts return the record; freshness
  within 1 second (p95).
- **Measured by**: single-process logs ingest-then-query acceptance test.
- **Baseline**: 0% — frozen snapshot today.

### Technical Notes

- Reuse seam: `log_query_api::router(store: Arc<dyn LogStore + Send + Sync>, tenant)`
  (`crates/log-query-api/src/lib.rs:95`); `router_with_auth` for the optional bearer path.
- Same `Arc`-shared composition pattern as US-01, applied to lumen.

### Dependencies

US-01 (composition pattern).

---

## US-04 — Send a trace and immediately query it back, no restart

### Elevator Pitch

- **Before**: Traces have a query API (`trace-query-api`, `/api/v1/traces` window and
  `/api/v1/traces/by_id` lookup, :9092) but the same frozen-snapshot gap; a freshly-ingested
  span is invisible until a restart.
- **After**: Andrea pushes an OTLP span `GET /api/v1/query_range` under trace id
  `4bf92f3577b34da6a3ce929d0e0e4736` for tenant `acme`, then queries `/api/v1/traces` (window)
  and `/api/v1/traces/by_id` (point lookup) against the same process and the trace comes back —
  no restart.
- **Decision enabled**: Andrea decides the live-visibility property holds for the third and
  final signal across both trace query routes, so all of Kaleidoscope's signals are
  experimentable.

### Problem

Traces are the last signal still on the frozen-snapshot path, and they have TWO query routes (a
time-window scan and a by-id lookup), so the slice must prove live visibility on both.

### Who

- **Andrea / a contributor** sending a trace and reading it back, both by window and by id.
- **Integration suite / CI** asserting the traces live-visibility property on both routes.

### Solution

Share the single `FileBackedTraceStore` instance between the ingest sink and the
`trace-query-api` router, which already accepts an injected `Arc<dyn TraceStore + Send + Sync>`
(`crates/trace-query-api/src/lib.rs:100`). Bind the traces query port on the same process; both
the window route and the by-id route read the shared store.

### Domain Examples

#### 1: A span read back live by time window

Andrea sends a span `GET /api/v1/query_range` (trace id `4bf92f3577b34da6a3ce929d0e0e4736`) for
tenant `acme` at time T. A `/api/v1/traces` query over a window covering T returns it, no
restart.

#### 2: The same span read back by trace id

A `/api/v1/traces/by_id` lookup for `4bf92f3577b34da6a3ce929d0e0e4736` returns the span moments
after ingest, no restart.

#### 3: Lookup before any trace, and cross-tenant, both return empty

A by-id lookup before any trace is ingested returns an empty success. A trace ingested for
`acme` is not returned to a `globex`-scoped query.

### UAT Scenarios (BDD)

#### Scenario: A trace is queryable by time window immediately after it is sent

```
Given the consolidated runtime is running with an empty trace store for tenant "acme"
When Andrea sends an OTLP span "GET /api/v1/query_range" with trace id "4bf92f3577b34da6a3ce929d0e0e4736" at time T for tenant "acme"
And Andrea queries "/api/v1/traces" over a window covering T
Then the response status is success
And the result contains the span just sent
And no restart was required
```

#### Scenario: The same trace is retrievable by its trace id

```
Given the consolidated runtime holds the span with trace id "4bf92f3577b34da6a3ce929d0e0e4736" for tenant "acme"
When Andrea looks up "/api/v1/traces/by_id" for "4bf92f3577b34da6a3ce929d0e0e4736"
Then the result contains the span
And no restart was required
```

#### Scenario: A trace lookup before any trace is sent returns an empty success

```
Given the consolidated runtime is running and no traces have been sent
When Andrea looks up "/api/v1/traces/by_id" for any trace id
Then the response status is success
And the result is empty
And the response is not an error
```

#### Scenario: A trace for one tenant is not returned to another tenant

```
Given the consolidated runtime holds a trace ingested for tenant "acme"
When a "/api/v1/traces" query is made scoped to tenant "globex"
Then the result is empty
And none of "acme"'s traces are returned
```

### Acceptance Criteria

- [ ] After a span is ingested, a `/api/v1/traces` window query covering its timestamp returns
  it, with no restart.
- [ ] The same span is retrievable by `/api/v1/traces/by_id` for its trace id, with no restart.
- [ ] A by-id lookup with no matching trace (or an empty store) returns `status:success` with an
  empty result, not an error.
- [ ] A trace query scoped to a non-owning tenant returns empty; both trace routes are served by
  the same process as ingest.

### Outcome KPIs

- **Who**: the experimenter sending and reading traces in the consolidated runtime.
- **Does what**: queries back a trace they just sent, by window and by id, without a restart.
- **By how much**: 0% → 100% of send-then-query trace attempts (both routes) return the span;
  freshness within 1 second (p95).
- **Measured by**: single-process traces ingest-then-query acceptance test, both routes.
- **Baseline**: 0% — frozen snapshot today.

### Technical Notes

- Reuse seam: `trace_query_api::router(store: Arc<dyn TraceStore + Send + Sync>, tenant)`
  (`crates/trace-query-api/src/lib.rs:100`); both routes (`TRACES_ROUTE`, `TRACES_BY_ID_ROUTE`)
  share the validator/store on the same state.
- Same `Arc`-shared composition pattern as US-01, applied to ray.

### Dependencies

US-01 (composition pattern).

---

## US-05 — Bring the whole stack up with one command and exercise every signal live

### Elevator Pitch

- **Before**: Even with each signal's loop working, "experimenting with Kaleidoscope" today
  means launching five binaries by hand over a shared pillar root and worrying about ordering
  and restarts — that is plumbing, not experimenting.
- **After**: Andrea runs the consolidated runtime with one command; it binds the ingest ports
  AND all three query ports on one process over one shared store per signal; he then pushes a
  metric, a log, and a trace and queries all three back live, with no restart and no second
  process. The acceptance command is a single-process test that exercises all three signals end
  to end.
- **Decision enabled**: Andrea decides the consolidation spine (C1) is done — the objective
  "one command, send, see" is met — and the roadmap can proceed to the run story (C2),
  generator (C3), and getting-started doc (C4).

### Problem

The roadmap's bar for "we can start experimenting" is one process that runs OTLP ingest, the
three stores, and the three query routers over one shared store per signal, so telemetry
ingested at T is queryable at T. US-01/03/04 each prove one signal's loop; this capstone proves
they coexist on one process with all five ports bound — the genuine consolidated runtime.

### Who

- **Andrea, the experimenter**: wants the whole stack from one command, all signals live.
- **A contributor / CI**: runs the full three-signal loop as the feature's demonstrable proof.

### Solution

One composition root opens each of the three stores once and injects each `Arc` into both its
ingest sink and its query router, on one tokio runtime, binding ingest gRPC 4317 / HTTP 4318
and query 9090 / 9091 / 9092. (DESIGN decides new-binary-vs-extend-gateway and how ports are
configured; the requirement is one process, all signals, all ports, one command.)

### Domain Examples

#### 1: One command, three signals, all live

Andrea starts the consolidated runtime with one command (`KALEIDOSCOPE_DEFAULT_TENANT=acme`,
the three query tenants `acme`, shared pillar root `/tmp/kal`, auth off). He pushes
`request_count`, a log `"checkout failed: card declined"`, and a span
`GET /api/v1/query_range`, all for `acme`, then queries `/api/v1/query_range`, `/api/v1/logs`,
and `/api/v1/traces` — each returns what he sent, no restart.

#### 2: All five ports answer on the one process

After the single startup command, the runtime accepts an OTLP push on 4318 and answers GETs on
9090, 9091, and 9092 — all from the same running process, with no port conflict.

#### 3: A fresh stack is consistent, not half-empty

On a brand-new pillar root, before any telemetry, all three query endpoints return empty
successes; after one push per signal, all three return the data. No signal lags behind another.

### UAT Scenarios (BDD)

#### Scenario: One command brings up ingest and all three query endpoints on one process

```
Given Andrea starts the consolidated runtime with a single command
Then the OTLP ingest endpoint accepts pushes
And the metrics, logs, and traces query endpoints all answer requests
And all of them are served by the same single running process
```

#### Scenario: Every signal sent is queryable back live, no restart

```
Given the consolidated runtime is running for tenant "acme"
When Andrea sends one metric, one log, and one trace for "acme"
And Andrea queries the metrics, logs, and traces endpoints in turn
Then each query returns the telemetry just sent
And no restart of the runtime or any component was required
```

#### Scenario: A fresh stack returns empty successes across all signals before any telemetry

```
Given the consolidated runtime has just started on an empty pillar root
When Andrea queries the metrics, logs, and traces endpoints
Then each returns a success with an empty result
And none returns an error
```

### Acceptance Criteria

- [ ] One startup command runs ingest and all three query endpoints on a single process.
- [ ] A metric, a log, and a trace each ingested for `acme` are each queryable back from their
  respective endpoints with no restart.
- [ ] All five endpoints (ingest gRPC + HTTP, metrics, logs, traces query) bind and answer on
  the one process without port conflict.
- [ ] On a fresh pillar root, all three query endpoints return empty successes before any
  telemetry and the corresponding data after one push per signal.

### Outcome KPIs

- **Who**: the experimenter bringing up the whole stack.
- **Does what**: runs ingest + all three query signals from one command and reads back
  everything sent, with no restart.
- **By how much**: one command (down from five hand-launched binaries with restart ordering);
  3 of 3 signals live; 100% of send-then-query attempts succeed.
- **Measured by**: a single-process acceptance test exercising all three signals; the manual
  one-command experiment.
- **Baseline**: today five binaries, manual ordering, restart required to see fresh data.

### Technical Notes

- Capstone of US-01, US-03, US-04. The all-ports-bound and three-signals-coexist properties are
  this story's unique contribution; the per-signal loops are proven in the earlier stories.
- DESIGN flags (W1 W2, `wave-decisions.md`): new-binary-vs-extend-gateway; one `tracing`
  install for the composed process; ephemeral test ports (fixed-port flake).
- This story does NOT include the compose/Makefile (that is roadmap item C2) or the README
  walkthrough (C4); it proves the one-process runtime that those will wrap.

### Dependencies

US-01, US-03, US-04.
