# Shared Artifacts Registry - `experimentable-stack-v0`

> Data values and names that appear in more than one place across the run story (compose), the
> generator, the docs, and Prism. Each needs a single source of truth so the run story, the
> generator, and the getting-started docs do not drift. British English, no em-dashes.

## Why this matters here

This feature spans three contexts (compose/run-story, generator, docs) plus Prism. The classic
horizontal-integration failure is the generator pushing one tenant or metric name while the docs
quote another and Prism queries a third. The registry pins the shared values so the "send" and the
"see" line up.

## Registry

```yaml
shared_artifacts:

  local_tenant:
    value: "acme"
    source_of_truth: "compose env KALEIDOSCOPE_TENANT (the one tenant, drives ingest default + all 3 query tenants - C1 environments.yaml tenant_posture)"
    consumers:
      - "the runtime service in compose (ingest default + query tenants)"
      - "the telemetry generator (pushes for this tenant)"
      - "the seed (US-05, scoped to this tenant)"
      - "the getting-started docs (quoted in examples)"
      - "Prism queries (resolved by the metrics router under this tenant)"
    owner: "experimentable-stack-v0 run story (compose)"
    integration_risk: "HIGH - if the generator/seed pushes a different tenant than the query routers resolve, the user sees an empty stack and concludes it is broken."
    validation: "the generator, the seed, and the docs all read/quote KALEIDOSCOPE_TENANT=acme; the smoke query uses the same tenant."

  pillar_root_volume:
    value: "one shared named volume mounted at the runtime's KALEIDOSCOPE_PILLAR_ROOT (sub-dirs pulse/lumen/ray)"
    source_of_truth: "compose volume definition + KALEIDOSCOPE_PILLAR_ROOT on the runtime service"
    consumers:
      - "the consolidated runtime (sole writer + reader; one process - C1)"
      - "(NOT a separate gateway - one-writer constraint, W7/C1)"
    owner: "experimentable-stack-v0 run story (compose)"
    integration_risk: "HIGH - two writers on one volume corrupt the WAL (assessment section 4). In the consolidated shape there is one process, so one writer by construction; compose must not add a second."
    validation: "exactly one service writes the volume; the runtime is the sole writer."

  ingest_ports:
    value: "gRPC 4317, HTTP 4318"
    source_of_truth: "C1 runtime (aperture::spawn); compose port mapping; Dockerfile.runtime EXPOSE (if added)"
    consumers: ["the generator (pushes OTLP here)", "the docs (send step)", "compose port mapping"]
    owner: "C1 consolidated-runtime (reused unchanged)"
    integration_risk: "MEDIUM - the generator must target the same host:port the run story publishes; a port already in use is the US-03 error path."
    validation: "the generator's target endpoint = the published ingest port; docs quote the same."

  query_ports:
    value: "metrics 9090 (/api/v1/query_range), logs 9091 (/api/v1/logs), traces 9092 (/api/v1/traces, /api/v1/traces/by_id)"
    source_of_truth: "C1 runtime query routers; compose port mapping"
    consumers: ["Prism (metrics 9090)", "the docs (see/query steps)", "the smoke/acceptance curls", "the generator's post-push verification (optional)"]
    owner: "C1 consolidated-runtime (reused unchanged)"
    integration_risk: "MEDIUM - Prism must query the same metrics port the run story publishes; docs must quote all three."
    validation: "Prism backend resolves to the 9090 router; docs and smoke use 9090/9091/9092."

  prism_backend_url:
    value: '"/api/v1" (relative, same-origin)'
    source_of_truth: "apps/prism/public/config.json -> backend.url"
    consumers: ["Prism at runtime (the URL it calls for query_range)"]
    owner: "apps/prism"
    integration_risk: "HIGH - if Prism is served same-origin from the 9090 router (KALEIDOSCOPE_QUERY_STATIC_DIR -> apps/prism/dist), the relative /api/v1 just works with NO config change and no CORS. If Prism is served as a SEPARATE service, this relative URL breaks and must be set to the runtime's absolute metrics URL (http://localhost:9090/api/v1), introducing CORS. This is DESIGN flag F4."
    validation: "if same-origin: config.json unchanged. If separate service: config.json backend.url updated AND CORS handled. Pick one and make it consistent."

  sample_metric_name:
    value: "request_count"
    source_of_truth: "the telemetry generator (US-04) / seed (US-05); reused from C1 user-stories sample data"
    consumers: ["the generator", "the seed", "the docs (Prism query example)", "the smoke/acceptance metrics query"]
    owner: "experimentable-stack-v0 generator"
    integration_risk: "MEDIUM - docs that tell the user to query request_count must match what the generator pushes; a mismatch shows an empty chart."
    validation: "generator, seed, docs, and smoke all use request_count."

  sample_log_body:
    value: '"checkout failed: card declined"'
    source_of_truth: "the telemetry generator (US-04); reused from C1 sample data"
    consumers: ["the generator", "the docs (logs query example)", "the logs smoke query"]
    owner: "experimentable-stack-v0 generator"
    integration_risk: "LOW - cosmetic, but keep consistent so the docs' expected row matches."
    validation: "generator and docs quote the same log body."

  sample_trace:
    value: 'span "GET /api/v1/query_range", trace id "4bf92f3577b34da6a3ce929d0e0e4736"'
    source_of_truth: "the telemetry generator (US-04); reused from C1 sample data"
    consumers: ["the generator", "the docs (traces query / by-id example)", "the traces smoke query"]
    owner: "experimentable-stack-v0 generator"
    integration_risk: "LOW - keep the trace id consistent so the by-id lookup example in the docs returns the span."
    validation: "generator and docs use the same trace id."

  bring_up_command:
    value: '"make up" (over "docker compose up") - exact wrapper is DESIGN flag F2'
    source_of_truth: "the make/just wrapper + compose file"
    consumers: ["the docs (the one command)", "the user"]
    owner: "experimentable-stack-v0 run story"
    integration_risk: "MEDIUM - the docs must quote the exact command the wrapper provides; a drift makes the getting-started steps wrong."
    validation: "the docs quote the actual target names the wrapper defines (up/down/demo/clean)."
```

## Integration checkpoints

1. **Tenant agreement**: the generator/seed push for `acme`; the query routers resolve `acme`;
   the docs and smoke query `acme`. One value, end to end.
2. **Prism backend agreement**: Prism's `/api/v1` resolves to the runtime's metrics router (F4
   decides same-origin vs separate-service-with-absolute-URL). The browser query reaches the same
   store the generator wrote.
3. **Sample-data vocabulary agreement**: `request_count`, the declined-checkout log, and the trace
   id are identical across the generator, the seed, the docs, and the smoke checks, and identical to
   C1's sample data so the whole consolidation family shares one vocabulary.
4. **One writer**: exactly one service (the consolidated runtime) writes the shared pillar volume.
5. **Command agreement**: the docs quote the exact wrapper commands.
