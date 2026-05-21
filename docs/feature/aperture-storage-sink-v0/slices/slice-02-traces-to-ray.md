# Slice 02: Traces persist to ray end to end

## Story
US-02 (traces to ray and survive a restart).

## Elevator Pitch
With the storage sink configured, send an OTLP trace over gRPC `:4317`, query ray,
and the spans are there with trace id, parent, kind, status, events and links
intact, surviving a restart.

## Entry point and observable output
- **Entry point**: OTLP traces over the gateway (gRPC `:4317` / HTTP `:4318`).
- **Observable output**: `event=sink_accepted sink=storage signal=traces` on accept;
  ray `get_trace(tenant, trace_id)` / `query` returning the spans with structure
  intact — before and after a restart.

## Carpaccio taste tests
- **End-to-end?** Yes: send trace -> persist -> query -> restart -> query.
- **Shippable alone?** Yes: a working trace storage pipeline; ray gains a consumer.
- **Thin?** Yes: one signal; the sink scaffold from Slice 01 is reused, so this is
  purely translation fidelity (the richest mapping: events, links, parent).
- **User-visible value?** Yes: operator decision "tracing pipeline is production-ready".

## Scope
`ExportTraceServiceRequest` -> `Vec<ray::Span>` translation (ids, parent, name,
kind, times, status, attributes, resource attributes, events, links) +
`TraceStore::ingest` + restart re-query. Reuses Slice 01 config/probe/tenant rule.

## Outcome KPI
KPI-2 (traces round-trip fidelity + durability); KPI-4; KPI-5.

## Out of scope
Logs, metrics, profiles.
