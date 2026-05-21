# Journey: Persist OTLP to the durable pillars (visual)

Feature: `aperture-storage-sink-v0`
Persona: **Priya Nair**, platform operator running the Kaleidoscope gateway in a
self-hosted observability stack. She has the pillars (lumen, ray, pulse) compiled
and durable, and aperture v0 listening on :4317/:4318, but nothing yet writes
received telemetry into the pillars. Her job: close the loop so the platform runs
end to end and survives a restart.

## End-to-end flow

```
[Trigger: Priya configures             [Send OTLP            [Query the              [Restart the
 the gateway with the                   over the gateway]     pillar]                 process, query again]
 storage sink]
        |                                     |                    |                        |
        v                                     v                    v                        v
+----------------------+        +----------------------+   +------------------+    +----------------------+
| sink.kind=storage    |        | otel SDK / otlp-cli  |   | lumen/ray/pulse  |    | restart, same store  |
| pillar roots set     |  --->  | exports logs/traces/ |-->| query returns    |--> | dir; query returns   |
| gateway starts,      |        | metrics to :4317     |   | the records, all |    | the SAME records.    |
| probe passes         |        | gateway accepts (Ok) |   | fields faithful  |    | Zero loss.           |
+----------------------+        +----------------------+   +------------------+    +----------------------+
  Feels: deliberate,             Feels: hopeful, watching   Feels: relief,          Feels: confident,
  slightly anxious               for the accept ack         "it's actually there"   "this is production-real"
  (will it wire up?)
  Artifacts:                     Artifacts:                 Artifacts:              Artifacts:
   ${sink_kind}                   ${otlp_endpoint}           ${tenant_id}            ${pillar_root}
   ${pillar_root}                 ${signal}                  ${service_name}         ${tenant_id}
   ${tenant_id}                                              records/spans/points    (same dir survives)
```

Emotional arc: **Problem Relief** (frustrated that the platform does not run
end to end -> hopeful while wiring and sending -> relieved when the query returns
the data -> confident after the restart proves durability).

## Step 1 — Configure the gateway with the storage sink

```
+-- Configure storage sink -----------------------------------------+
| sink:                                                             |
|   kind: ${sink_kind}        # "storage"                            |
|   storage:                                                         |
|     pillar_root: ${pillar_root}    # ./data (lumen/ray/pulse dirs) |
|     default_tenant: ${tenant_id}   # "acme" when no tenant.id attr |
|                                                                   |
| $ aperture --config aperture.toml                                  |
| event=probe_ok sink=storage pillar_root=./data                     |
| event=listening grpc=0.0.0.0:4317 http=0.0.0.0:4318                |
+-------------------------------------------------------------------+
```

Entry: deliberate. Exit: reassured the sink wired up (probe passed before listen).

## Step 2 — Send OTLP over the gateway, receive accept

```
+-- Send logs over gRPC ---------------------------------------------+
| $ otlp-cli logs --endpoint ${otlp_endpoint} \                      |
|     --service checkout-api --body "order 1001 placed"              |
|                                                                    |
| gateway: event=sink_accepted sink=storage signal=${signal} \       |
|          record_count=1 resource.service.name=checkout-api         |
| client : OK (gRPC 200)                                             |
+--------------------------------------------------------------------+
```

Entry: hopeful. Exit: encouraged — the gateway acknowledged after the pillar
ingest returned Ok (aperture awaits `accept` before responding to the SDK).

## Step 3 — Query the pillar, the records are there

```
+-- Query lumen -----------------------------------------------------+
| LogStore::query(tenant="${tenant_id}", range=all)                  |
|   -> 1 record                                                      |
|      observed_time_unix_nano : 1_716_240_000_000_000_000           |
|      severity_text           : "INFO"                              |
|      body                    : "order 1001 placed"                 |
|      resource.service.name   : "checkout-api"                      |
+--------------------------------------------------------------------+
```

Entry: anticipation. Exit: relief — the data is faithfully present, field for field.

## Step 4 — Restart, query again, same data

```
+-- Restart and re-query --------------------------------------------+
| (process restarted; same ${pillar_root})                          |
| LogStore::query(tenant="${tenant_id}", range=all)                  |
|   -> 1 record  (identical to pre-restart)                          |
| Zero loss. Faithful round-trip across a process boundary.          |
+--------------------------------------------------------------------+
```

Entry: a little anxious (will it survive?). Exit: confident — durable, production-real.

## Failure modes (for DISTILL error-scenario generation)

- Pillar root not writable / missing -> probe fails, gateway refuses to start
  (Earned-Trust: `wire_then_probe_then_use`), clear `event=probe_failed` reason.
- A record carries no resolvable tenant and no `default_tenant` configured ->
  sink refuses with `SinkError::Internal` naming the missing tenant rule.
- A field cannot be translated faithfully (e.g. unsupported metric point type,
  malformed trace id length) -> record refused with a reason naming the field;
  no partial/silently-dropped persistence.
- Pillar ingest returns `PersistenceFailed` -> surfaces as `SinkError::Internal`;
  the gateway returns a retryable status to the SDK (the SDK retries upstream).
