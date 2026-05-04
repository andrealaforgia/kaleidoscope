# Observability Design — `aperture` v0 (DEVOPS)

> **Wave**: DEVOPS (`nw-platform-architect` / Apex).
> **Date**: 2026-05-04.
> **Author**: Apex.
> **Companion documents**: `wave-decisions.md`, `kpi-instrumentation.md`,
> `monitoring-alerting.md`.
> **Source of truth for the event vocabulary**:
> [`docs/feature/aperture/discuss/wave-decisions.md > D1`](../discuss/wave-decisions.md)
> plus DESIGN's four additions in
> [`adr-0009-aperture-observability-strategy.md`](../../../product/architecture/adr-0009-aperture-observability-strategy.md).

---

## Framing

This document is the **operator-facing observability runbook** for
Aperture v0. It answers, from the operator's perspective:

1. Where does Aperture put its observability data?
2. How does my log aggregator capture it?
3. How do I express the four guardrail alerting rules in my chosen
   alerting system?
4. What do `/healthz` and `/readyz` mean?
5. What MUST I avoid doing (the no-`/metrics` policy)?

Aperture's observability is deliberately small: structured JSON to
stderr, two HTTP probe endpoints, no `/metrics`, no OTLP-out, no
agent of any kind. The whole point is that the operator's existing
observability stack consumes Aperture's output without Aperture
demanding anything be installed.

---

## What Aperture emits (the data feed)

### Three surfaces

| Surface | Format | Consumer |
|---|---|---|
| **stderr** | JSON Lines (`application/jsonl` if you must label it; one event per line) | Operator's log aggregator |
| **`GET /healthz`** | HTTP `200 ok` (always 200 if the process is up) | Operator's liveness probe |
| **`GET /readyz`** | HTTP `200 ready` when both listeners bound and not draining; `503 starting` during startup; `503 draining` during shutdown | Operator's readiness probe |

Both `/healthz` and `/readyz` are served on the OTLP HTTP listener
(port 4318). There is no separate admin port at v0 (DISCUSS Q6;
ADR-0009).

### The closed event vocabulary

Per DISCUSS D1, the closed set of structured event names Aperture
writes to stderr in v0 is 16 event names; per DESIGN ADR-0009 + four
additions (`health.startup.refused`, `config_validation_failed`,
`internal_invariant_violation`, `request_received` was already in
DISCUSS D1's set), the v0 closed vocabulary is **20 names total**.
The full list lives in `crates/aperture/src/observability/events.rs`
(DELIVER produces this file; the names are locked at DESIGN time).

The full v0 vocabulary, grouped by purpose:

```
Lifecycle:
  startup
  listener_bound
  listener_closing
  listener_bind_failed
  ready
  readiness_changed
  shutdown_initiated
  shutdown_complete
  in_flight_drained
  drain_deadline_exceeded

Per-request (the volume-shaped data):
  request_received
  sink_accepted
  sink_failed

Validation / framing failures:
  unsupported_media_type
  body_too_large

Backpressure:
  concurrency_cap_hit

Configuration / startup contract:
  tls_not_supported_in_v0
  config_validation_failed
  health.startup.refused

Catastrophic:
  internal_invariant_violation
```

(The dotted form `health.startup.refused` is intentional and matches
ADR-0009's naming; the rest are underscore-separated. The mix is
allowed because the vocabulary is closed-set and renames are
version-bump-able.)

### Event field schema

Every JSON line carries at minimum:

```json
{
  "timestamp": "2026-05-04T12:34:56.789Z",
  "level": "info",
  "event": "request_received",
  "fields": { ... }
}
```

The `fields` object is per-event; specific fields are documented in
`component-design.md` (DESIGN-owned). The volume-shaped events
(`request_received`, `sink_accepted`, `sink_failed`) all carry:

- `transport`: `"grpc"` or `"http_protobuf"`
- `signal`: `"logs"` / `"traces"` / `"metrics"`
- `service.name`: extracted from the resource attribute (when present)
- For `sink_accepted`: `record_count` / `span_count` /
  `data_point_count` (per signal); `latency_ms`; `sink` (`"stub"`,
  `"forwarding"`, etc.)
- For `sink_failed`: `error` (variant name from `SinkError` enum)
- For `concurrency_cap_hit`: `transport`; `cap` (the integer cap)

Operators querying the event stream rely on these field names being
stable across the v0 line; the closed vocabulary AND the field
schema are the operator-facing observability contract.

---

## Operator log-aggregation patterns (recommended)

Aperture has **no opinion on which log aggregator the operator
uses**. The patterns below are starting points, not requirements.

### Pattern 1: kubernetes + Loki + promtail (or grafana-agent)

Most common pilot deployment shape (as of 2026, Kaleidoscope's
target audience). The operator's deployment manifest has Aperture
running as a sidecar or as a Deployment; promtail (or grafana-agent)
is already capturing all pod stderr; LogQL queries the structured
JSON directly.

Sample LogQL queries:

```logql
# Acceptance ratio per transport (KPI 2)
sum by (transport) (
  count_over_time({app="aperture"} | json | event="sink_accepted" [5m])
)
/
sum by (transport) (
  count_over_time({app="aperture"} | json | event="request_received" [5m])
)

# Per-signal acknowledgement ratio (KPI 4)
sum by (signal) (
  count_over_time({app="aperture"} | json | event="sink_accepted" [5m])
)
/
sum by (signal) (
  count_over_time({app="aperture"} | json | event="request_received" [5m])
)

# Concurrency cap hit count (KPI 5 surface)
sum by (transport) (
  count_over_time({app="aperture"} | json | event="concurrency_cap_hit" [5m])
)

# Drain-deadline exceeded events (KPI 8 surface — should be zero
# under healthy operation)
sum (
  count_over_time({app="aperture"} | json | event="drain_deadline_exceeded" [1h])
)
```

### Pattern 2: kubernetes + Splunk Connect for Kubernetes

```spl
` (Splunk Search Processing Language) `
index=k8s sourcetype=kube:container:aperture
| spath input=event
| stats count by event, transport, signal

` Acceptance ratio: `
index=k8s sourcetype=kube:container:aperture event=sink_accepted
| stats count as accepted by transport
| join transport [
    search index=k8s sourcetype=kube:container:aperture event=request_received
    | stats count as received by transport
  ]
| eval ratio=accepted/received
```

### Pattern 3: systemd + journald

Aperture writes to stderr; systemd-journald captures it. Querying:

```bash
# All Aperture events, JSON-formatted (each line is journald's
# wrapper around Aperture's own JSON line):
journalctl -u aperture --output=json --no-pager

# Acceptance count last hour:
journalctl -u aperture --since='1 hour ago' --no-pager \
  | jq -r 'select(.MESSAGE) | .MESSAGE | fromjson? | select(.event=="sink_accepted") | .fields.transport' \
  | sort | uniq -c

# Cap hits in the last 5 minutes:
journalctl -u aperture --since='5 minutes ago' --no-pager \
  | jq 'select(.MESSAGE) | .MESSAGE | fromjson? | select(.event=="concurrency_cap_hit") | .fields' \
  | jq -s 'group_by(.transport) | map({transport: .[0].transport, count: length})'
```

### Pattern 4: ELK (Elasticsearch + Logstash + Kibana) or OpenSearch

The Logstash filter for Aperture's structured JSON:

```ruby
filter {
  if [kubernetes][container][name] == "aperture" {
    json {
      source => "message"
    }
  }
}
```

KQL queries in Kibana / OpenSearch Dashboards:

```kql
event:"sink_accepted" AND transport:"grpc"

event:"concurrency_cap_hit"

event:"drain_deadline_exceeded" AND fields.dropped_count > 0
```

### Pattern 5: Vector / Fluent Bit / Filebeat → ClickHouse / OpenSearch / etc

A Vector source-and-transform pipeline:

```yaml
sources:
  aperture_stderr:
    type: kubernetes_logs
    extra_label_selector: "app=aperture"

transforms:
  parse:
    type: remap
    inputs: [aperture_stderr]
    source: |
      .parsed = parse_json!(.message)
      .event = .parsed.event
      .transport = .parsed.fields.transport
      .signal = .parsed.fields.signal
```

### Pattern 6: bare-metal + plain-text log file

Operator pipes Aperture's stderr to a file:

```bash
aperture --config /etc/aperture.toml 2> /var/log/aperture.jsonl
```

…and uses standard text-processing tooling (`jq`, `awk`, etc.) to
mine it.

The point: Aperture's output is plain JSON Lines. Every modern
observability stack handles JSON Lines. Aperture has no opinion on
which one the operator uses.

---

## Operator query patterns (per outcome KPI)

Documented in `kpi-instrumentation.md > Per-KPI specification`,
copied here as a self-contained reference for the operator.

| KPI | Query shape (LogQL flavour; translate to your aggregator) |
|---|---|
| KPI 2 — gRPC OK / HTTP 200 ratio | `count_over_time({app="aperture"} \| json \| event="sink_accepted") / count_over_time({app="aperture"} \| json \| event="request_received") [5m]`, grouped by `transport` |
| KPI 4 — Per-signal acknowledgement ratio | Same shape, grouped by `signal` |
| KPI 5 — Concurrency saturation events | `count_over_time({app="aperture"} \| json \| event="concurrency_cap_hit") [5m]`, grouped by `transport` |
| KPI 7 — Downstream-acceptance ratio | `count_over_time({app="aperture"} \| json \| event="sink_accepted" \| sink="forwarding") / (count_over_time({app="aperture"} \| json \| event="sink_accepted" \| sink="forwarding") + count_over_time({app="aperture"} \| json \| event="sink_failed" \| sink="forwarding")) [5m]`, optionally cross-checked with the operator's downstream-side request-receive count |
| KPI 8 — Graceful-restart drop ratio | `count_over_time({app="aperture"} \| json \| event="drain_deadline_exceeded") [24h]` should be 0 under healthy operation |

---

## Healthz and readyz semantics

### `GET /healthz`

> **Liveness probe**: is the process alive at all?

| Response | Meaning | Operator action |
|---|---|---|
| `200 ok` | Process is up. Aperture's main thread responds; the HTTP listener thread responds. | None; this is the healthy state. |
| `(no response, connection refused, timeout)` | Process is not up, OR process is wedged (e.g. deadlock; though Aperture has no mutex pairs that could deadlock — the trait calls are async-await). | Restart the pod / process. |

`/healthz` returns `200 ok` for the entire process lifetime,
including during `starting` (listeners not yet bound) and `draining`
(listeners closing). The contract is "is the process up?", not
"is the process serving traffic?". The latter is `/readyz`'s job.

If `/healthz` is non-200 for ANY reason during the process lifetime,
that is a fatal invariant violation; per DISCUSS handoff this is a
page-level alert.

### `GET /readyz`

> **Readiness probe**: is the process serving traffic right now?

| Response | Meaning | Operator action (k8s) |
|---|---|---|
| `200 ready` | Both listeners bound; not draining. Pod is in service. | k8s adds the pod to the Service endpoints. |
| `503 starting` | Process up; one or both listeners not yet bound. Probably very brief (≤ 1 second on a healthy host). | k8s holds the pod out of Service endpoints. Repeated retries are normal. |
| `503 draining` | SIGTERM received; listeners closing; in-flight requests draining. | k8s removes the pod from Service endpoints. Existing in-flight connections complete; new connections go to other replicas. |

The three-state machine (`Starting` → `Ready` → `Draining`) is the
load-bearing contract DISCUSS US-AP-02 names; KPI 3 is the structural
defence. The transitions are atomic (`AtomicReadinessState` per
ADR-0009); the operator's probe never sees an in-between state.

### k8s probe configuration recommendation

```yaml
livenessProbe:
  httpGet:
    path: /healthz
    port: 4318
  initialDelaySeconds: 5
  periodSeconds: 30
  timeoutSeconds: 5
  failureThreshold: 3

readinessProbe:
  httpGet:
    path: /readyz
    port: 4318
  initialDelaySeconds: 1
  periodSeconds: 5
  timeoutSeconds: 2
  failureThreshold: 1
  successThreshold: 1
```

Rationale:

- **Liveness `initialDelaySeconds: 5`**: more than enough for
  Aperture's ~50 ms startup. Generous because liveness failures
  trigger pod restart, which is expensive.
- **Liveness `failureThreshold: 3`**: tolerate two transient probe
  failures (e.g. a brief network blip in the host) before declaring
  the pod dead.
- **Readiness `initialDelaySeconds: 1`**: short; readiness should
  flip to 200 within ~50 ms of process start.
- **Readiness `periodSeconds: 5`**: fast cycle; we want
  out-of-service drains to be visible quickly.
- **Readiness `failureThreshold: 1`**: a single 503 should remove
  the pod from the endpoints. Aperture sends 503 for `starting` and
  `draining`; both states are transient and intentional.

These are recommendations, not requirements. Operators tune to their
host environment's noise floor.

---

## Configuration knobs that affect observability

### `bind_address`

Per ADR-0008. Default `0.0.0.0:4317` (gRPC) and `0.0.0.0:4318` (HTTP).
The HTTP listener serves OTLP **and** `/healthz` and `/readyz`. There
is no separate admin port at v0 (DISCUSS US-AP-02; ADR-0009).

### `drain_deadline_ms`

Per DISCUSS D8 + ADR-0009. Default `30000` (30 s) to align with k8s'
default `terminationGracePeriodSeconds`. Operators running outside
k8s (systemd, bare metal) may set a longer deadline if their
orchestrator allows it.

If `drain_deadline_ms` is exceeded during shutdown, Aperture exits 1
with a `event=drain_deadline_exceeded dropped_count=N` warn line.
Operators alerting on this event detect rolling-deployment
configuration mismatches (e.g. operator set
`terminationGracePeriodSeconds: 5` but `drain_deadline_ms: 30000`).

### `max_concurrent_requests`

Per DISCUSS D7 + ADR-0010. Default `1024` per transport. The cap
controls when `event=concurrency_cap_hit` fires; smaller fleets
should lower the cap to fit pod memory; larger fleets should run
more replicas before raising it.

### Sink configuration

Per ADR-0008 + ADR-0007. `sink = "stub"` (logs the summary line)
or `sink = "forwarding"` (POSTs to the configured downstream).
`StubSink` is appropriate for development / smoke testing;
`ForwardingSink` is the production sink.

The probe contract (Earned-Trust) catches misconfigured downstreams
at startup with `event=health.startup.refused`. The operator should
treat this as a fatal startup failure and inspect the configured
endpoint.

---

## What NOT to do

### Do not add a `/metrics` endpoint at v0

DISCUSS Q6 explicit: no Prometheus or OTLP-out metrics in v0.
Pulse-shaped concern, deferred to Phase 4. The CI invariant
`no_telemetry_on_telemetry` (DISCUSS D4; gate-7 future) is the
load-bearing defence.

If an operator strongly demands a Prometheus exporter for v0,
**that is a request for Pulse**, not for Aperture. The honest
answer is "Aperture v0 emits structured stderr; please instrument
your existing log aggregator with the queries above; if Pulse's
phase blocks your adoption, raise that as feedback".

### Do not add an OTLP-out from Aperture itself

Same rule. The same CI invariant catches it. ForwardingSink is the
ONLY allowed outbound network traffic from Aperture; everything else
is a regression.

### Do not parse `/healthz` for anything other than HTTP 200

The body string `"ok"` is informational and may change in a future
version (it is a closed-vocabulary string; a `/healthz` response
body change would be a non-breaking documentation update). Operators
relying on the string content for parsing are coupling to an
implementation detail; the contract is the HTTP status code.

### Do not parse `/readyz` for anything other than HTTP 200 and the body name

The three body strings (`"ready"`, `"starting"`, `"draining"`) are
part of the contract; they correspond 1:1 to the three states of the
ReadinessState machine. Operators can parse the body to distinguish
`starting` from `draining` if they want richer observability; both
return HTTP 503, so the body IS the discriminator.

### Do not rely on log-line ordering across events from different transports

Aperture writes JSON Lines from multiple Tokio tasks; the
`tracing-subscriber` JSON layer serialises writes per-line (one
event per line; never partial — per ADR-0009 R8 mitigation). But
events from different transports may interleave; the absolute
ordering of two events emitted from different transport tasks is
not guaranteed (and would be wrong to rely on for correctness).

If the operator's analysis requires ordering, sort by the `timestamp`
field (ISO-8601 with millisecond precision; per ADR-0009).

### Do not log credentials or PII through Aperture's stderr

The closed event vocabulary plus the field schema (DESIGN-locked)
do not include any of: bearer tokens, API keys, request bodies,
end-user identifiers, IP addresses (other than the listener bind
addresses, which are operator-public configuration). If an operator
modifies Aperture to log additional fields, that is a fork; v0's
contract is "log the closed vocabulary and the documented fields,
nothing else".

The closed vocabulary IS the data-loss-prevention contract for
operators handling sensitive observability.

---

## Three pillars of observability — Aperture's posture

Per the `infrastructure-and-observability` skill:

| Pillar | Aperture v0 |
|---|---|
| **Logs** | YES — structured JSON Lines to stderr; closed vocabulary; this IS the primary data feed at v0. |
| **Metrics** | NO — deliberately deferred to Pulse Phase 4. Operators compute counts/ratios from the log stream. |
| **Traces** | YES, but Aperture is the consumer, not the emitter. The OTel SDKs that target Aperture emit spans (KPI 4 traces); Aperture validates and forwards them; Aperture itself does not span its own request handling at v0 (would be telemetry-on-telemetry). |

The "metrics" pillar is the deliberate gap. The DISCUSS handoff is
explicit: at v0, the operator's log-aggregator-driven counts are the
metric surface. This is unconventional but justified by the no-
telemetry-on-telemetry commitment.

---

## Cardinality and volume considerations

Aperture's stderr volume is roughly proportional to request volume:
~3 events per accepted request (`request_received`, `sink_accepted`,
plus per-request observability hooks DELIVER may add) plus 0 events
for in-process activity (no per-request internal trace). At realistic
OTel SDK export rates (a few exports per second per pod, batched), a
single Aperture replica produces tens-to-hundreds of events per
second — well within any modern log aggregator's ingest budget.

Cardinality concerns:

- `transport` field: cardinality 2 (`grpc`, `http_protobuf`). Bounded.
- `signal` field: cardinality 3 (`logs`, `traces`, `metrics`). Bounded.
- `sink` field: cardinality typically 2 (`stub`, `forwarding`); if
  Sieve lands in Phase 1 it adds variants. Bounded.
- `service.name` field: cardinality = number of distinct OTel-SDK-
  using services in the operator's fleet. Potentially unbounded;
  operators with many services should consider stripping or
  bucketing this field at log-aggregation time if cardinality
  explosion becomes a cost concern.

The closed event-name vocabulary itself is cardinality-bounded
(~20 names at v0); aggregation by event name is always safe.

---

## Summary

Aperture's v0 observability surface is:

- **Structured JSON Lines to stderr**, vocabulary closed (~20
  events), schema documented in `component-design.md`.
- **`GET /healthz`**: liveness; always 200 if process up.
- **`GET /readyz`**: readiness; 200 / 503 reflecting the
  three-state machine.
- **No `/metrics` endpoint**, no OTLP-out, no agent.

The operator brings their own observability stack. Aperture's job
is to emit data the operator's stack can consume; the stack itself
is operator-owned. The four guardrail alerting rules (page on
acceptance ratio drop, page on `/healthz` non-200, ticket on
`concurrency_cap_hit`, page on unexpected outbound traffic) are
documented in `monitoring-alerting.md` as queries operators
translate to their preferred alerting system.

This is the lightest possible v0 observability posture that is also
honest: Aperture genuinely does emit useful structured data; the
operator genuinely can build dashboards and alerts on it; nothing is
hidden behind a `/metrics` endpoint that does not exist by design.
</content>
</invoke>
