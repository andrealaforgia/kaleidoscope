# Observe Kaleidoscope with a real OTLP collector

Kaleidoscope's CLI can emit OTLP-JSON metrics for its own activity
to any path. The previous commits proved that the bytes leaving the
binary are spec-compliant OTLP. This document closes the loop: a
real OpenTelemetry Collector running unmodified consumes those
bytes, and you can see your `kaleidoscope-cli ingest` activity in a
real collector's pipeline.

## What you'll set up

A three-process pipeline on one machine:

1. `kaleidoscope-cli ingest --observe-otlp <path>` writes one
   OTLP-JSON `ResourceMetrics` line per Lumen event to `<path>`.
2. The provided shell sidecar (`scripts/observe-with-otlp-collector.sh`)
   `tail -F`s that file, wraps each line in a `MetricsData`
   envelope, and POSTs it to a collector's `/v1/metrics`.
3. A Docker-hosted `otel/opentelemetry-collector-contrib` parses
   the metric and writes it to its debug exporter (stdout).

No new Rust crate. No `opentelemetry-otlp` dependency. No `tokio`.
Three small moving parts in three languages a sysadmin already
speaks: Rust (the bridge, already shipped), bash (the sidecar,
20 lines), YAML (the collector config, 10 lines).

## Step-by-step

### 1. Collector config

```yaml
# collector.yaml
receivers:
  otlp:
    protocols:
      http:
        endpoint: 0.0.0.0:4318

exporters:
  debug:
    verbosity: detailed

service:
  pipelines:
    metrics:
      receivers: [otlp]
      exporters: [debug]
```

### 2. Start the collector

```bash
docker run -d --name kal-otlp-collector \
  -p 14318:4318 \
  -v "$(pwd)/collector.yaml:/etc/otelcol-contrib/config.yaml" \
  otel/opentelemetry-collector-contrib:latest
```

The `:4318` HTTP receiver is mapped to host port `14318` so it
does not collide with any other collector you might have running.

### 3. Start the sidecar

```bash
./scripts/observe-with-otlp-collector.sh \
  /tmp/otlp.log \
  http://localhost:14318/v1/metrics &
```

The script creates `/tmp/otlp.log` if it does not exist (so the
`tail -F` does not race with the first writer) and POSTs every
line to the collector.

### 4. Ingest

```bash
echo '{"observed_time_unix_nano":100,"severity_number":9,"severity_text":"INFO","body":"hello","attributes":{},"resource_attributes":{"service.name":"checkout"},"trace_id":null,"span_id":null}' \
  | ./target/release/kaleidoscope-cli ingest acme ./data \
      --observe-otlp /tmp/otlp.log
```

### 5. Watch the collector

```bash
docker logs --since 10s kal-otlp-collector
```

You should see a section like:

```text
Resource attributes:
     -> tenant_id: Str(acme)
ScopeMetrics #0
InstrumentationScope kaleidoscope.lumen
Metric #0
Descriptor:
     -> Name: lumen.ingest.count
     -> DataType: Sum
     -> IsMonotonic: true
     -> AggregationTemporality: Cumulative
NumberDataPoints #0
Data point attributes:
     -> tenant_id: Str(acme)
Value: 1
```

The collector reports exactly what the bridge claims: a
cumulative monotonic sum named `lumen.ingest.count`, scoped under
`kaleidoscope.lumen`, attributed to tenant `acme`, with the count
of records the ingest call processed.

## What this proves

That the four-commit OTLP arc (the in-workspace Pulse bridge, the
hand-rolled OTLP-JSON writer, the `--observe-otlp` CLI flag, and
this sidecar recipe) ends at "a real OpenTelemetry collector
ingests your data". From here every existing OTel ecosystem tool
applies: Prometheus, Grafana, Datadog, Honeycomb, Splunk, any of
the SaaS or self-hosted backends that speak OTLP. Kaleidoscope's
self-observability is not a self-referential closed loop; it
joins the larger observability ecosystem on day one.

## What v2 would add

The sidecar is deliberately a 20-line bash script. It has no
retry, no batching, no local queue, no metrics about its own
forwarding rate. Those are real concerns in a production
deployment, but they are operator concerns that the operator's
environment usually has better answers for than a Kaleidoscope
opinion. v2 may ship a richer Rust sidecar with retry+queue
semantics, OR document a fluent-bit / Vector / Filebeat recipe
that does the same job with battle-tested tooling. Today the
recipe is "use bash because bash works".

## Tear-down

```bash
docker rm -f kal-otlp-collector
# kill the sidecar (find its PID with `jobs` or `pgrep`)
```
