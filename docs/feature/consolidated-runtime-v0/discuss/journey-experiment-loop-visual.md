# Journey — the experiment loop (`consolidated-runtime-v0`)

> Persona: **Andrea**, running Kaleidoscope locally to experiment with it. Secondary: a
> Kaleidoscope contributor evaluating a change; the integration suite / CI exercising the loop.
> Goal: send telemetry in and immediately see it come back out, without restarting anything.
> UX depth: lightweight (backend / infrastructure; the "interface" is a command + an OTLP push
> + an HTTP GET). British English, no human-effort estimation.

## The loop that fails today vs the loop C1 delivers

```
   TODAY (separate processes, frozen snapshot)        C1 (one process, shared Arc<Store>)
   ------------------------------------------         -----------------------------------
   start query-api  (store snapshot = empty)          start consolidated runtime
        |                                                   |  builds ONE store per signal,
   send a metric  ---> written to $ROOT/pulse WAL          |  Arc::clone -> sink AND router
        |                                                   |
   GET /api/v1/query_range                              send a metric  ---> store.ingest(&self)
        |                                                   |
   => EMPTY  (reader's in-memory map is frozen          GET /api/v1/query_range
      at its own open(); never re-reads)                    |
        |                                              => RETURNS THE METRIC  (same Mutex,
   restart query-api to see it  <-- the failure            written-then-read, T+epsilon)
                                                       no restart of anything
```

## Happy-path flow (the three beats)

```
[Trigger]                 [Beat 1: Run]            [Beat 2: Push]          [Beat 3: Query]
"I want to try            one command starts       send one OTLP           GET the query
 Kaleidoscope"            the consolidated         metric to :4318         endpoint :9090
                          runtime (ingest +              |                       |
                          all query routers,             |                       v
                          one shared store/signal)       |                 => the metric I just
        |                       |                         |                    sent comes back
        v                       v                         v                       |
   Feels: curious,        Sees: ports bound,        Sees: 200 OK from       Feels: RELIEVED,
   slightly wary          "runtime ready",          the ingest endpoint     "it actually works,
   ("will this be         empty store, no                                    end to end, no
    five binaries         restart needed                                     restart"
    and a restart
    dance?")
```

## Emotional arc (Problem Relief pattern)

| Beat | Entry emotion | Exit emotion | Design lever |
|------|---------------|--------------|--------------|
| Run | Curious, wary of plumbing | Oriented, ready | One command; clear "runtime ready" + which ports are bound; auth off by default so no token ceremony |
| Query-before-push | (Skeptic's probe) | Reassured | An empty store returns an empty *success* (HTTP 200, empty result), NOT an error or a crash |
| Push | Focused | Acknowledged | Ingest endpoint returns success promptly |
| Query-after-push | Hopeful, the peak tension | RELIEVED, confident | The metric comes back within ~1s, in the same process, no restart — the loop that used to fail now closes |

The arc is deliberately flat-to-relieved: the whole point of C1 is to remove a frustration
(the restart dance), so the emotional payoff is *relief and trust*, not novelty delight.

## Per-signal mockups (illustrative; ports/paths are shared artifacts)

### Beat 1 — Run (one command, all ports on one process)

```
+-- consolidated runtime ------------------------------------------+
| event=runtime_starting  pillar_root=/tmp/kal  default_tenant=acme |
| ingest:  gRPC ${INGEST_GRPC_PORT}   HTTP ${INGEST_HTTP_PORT}      |
| query:   metrics ${METRICS_PORT}  logs ${LOGS_PORT}  traces ${TRACES_PORT} |
| event=runtime_ready  (one shared store per signal; no restart needed) |
+------------------------------------------------------------------+
```

### Beat 3 — Query a metric back (metrics, :9090)

```
$ curl ":${METRICS_PORT}/api/v1/query_range?query=request_count&start=...&end=..."
{ "status":"success",
  "data": { "resultType":"matrix",
            "result": [ { "metric": {"__name__":"request_count","tenant":"acme"},
                          "values": [ [T, "1"] ] } ] } }
                          ^-- the point I ingested at T, returned at T+epsilon, no restart
```

### Beat 3 (variant) — Query before any ingest returns empty success, not an error

```
$ curl ":${METRICS_PORT}/api/v1/query_range?query=request_count&start=...&end=..."
{ "status":"success", "data": { "resultType":"matrix", "result": [] } }
   ^-- empty store => empty 200, never a 500 or a stale value
```

### Logs (:9091, /api/v1/logs) and traces (:9092, /api/v1/traces + /api/v1/traces/by_id)

Same three beats, same shared-store mechanism: a log body `"checkout failed: card declined"`
ingested for tenant `acme` is returned by `GET /api/v1/logs` moments later; a span
`GET /api/v1/query_range` under trace id `4bf92f...` is returned by `/api/v1/traces` (window)
and `/api/v1/traces/by_id` (point lookup) without a restart.

## Integration checkpoints (validated by US-05 capstone)

1. The same process binds ingest 4317/4318 AND query 9090/9091/9092 with no conflict.
2. For each signal, the store the sink writes is the SAME `Arc` the router reads.
3. Tenant isolation holds: a read scoped to `globex` never sees `acme`'s data (US-02).
4. The runtime started with an empty store; post-startup appends are visible without restart.

## Shared artifacts in this journey

The port numbers, the tenant identifier, the pillar root, and the per-signal store instance
recur across every beat and every binary. They are tracked in
`shared-artifacts-registry.md` — the store *instance* identity (one `Arc` per signal shared
between sink and router) is the load-bearing shared artifact of the whole feature.
