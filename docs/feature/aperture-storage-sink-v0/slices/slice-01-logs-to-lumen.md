# Slice 01: Logs persist to lumen end to end

## Story
US-01 (logs to lumen and survive a restart).

## Elevator Pitch
Configure the gateway with the storage sink, send OTLP logs over gRPC `:4317`,
query lumen, and the records are there and survive a restart.

## Entry point and observable output
- **Entry point**: OTLP logs over the gateway (gRPC `:4317` / HTTP `:4318`) —
  user-invocable, the real platform ingress.
- **Observable output**: `event=sink_accepted sink=storage signal=logs` on accept;
  a lumen `LogStore::query(tenant, all)` returning the persisted records (body,
  severity_text, resource service.name) — both before and after a restart.

## Carpaccio taste tests
- **End-to-end?** Yes: config -> send -> persist -> query -> restart -> query.
- **Shippable alone?** Yes: a working logs storage pipeline is independently
  valuable; lumen gains a real production consumer.
- **Thin?** Yes: one signal, one translation, the smallest faithful proof.
- **User-visible value?** Yes: operator decision "logs pipeline is production-ready".

## Scope
Storage sink + config + Earned-Trust probe + tenant-resolution rule (all reused by
slices 02/03) + `ExportLogsServiceRequest` -> `Vec<lumen::LogRecord>` translation +
`LogStore::ingest` + restart re-query.

## Outcome KPI
KPI-1 (logs round-trip fidelity + durability); KPI-4 latency guardrail; KPI-5 no
silent loss.

## Out of scope
Traces, metrics, profiles.
