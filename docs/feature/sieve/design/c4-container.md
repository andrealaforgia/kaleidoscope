# Sieve v0 — C4 Container (L2)

The container diagram zooms into the Aperture-bearing process. Sieve
is a library compiled into the same binary; it is a "container" in
the C4 sense (a deployable / runnable unit of code) only insofar as
it is the boundary at which the slice tests run independently. In a
production deployment, every container in the diagram below runs
inside the same Aperture process.

```mermaid
C4Container
  title Container Diagram — Aperture process with Sieve wired in

  Person(riley, "Riley (SRE)", "Reads INFO summary; sets env vars")

  System_Boundary(process, "Aperture process (single OS process)") {
    Container(transport, "Aperture transport layer", "Rust + tonic + axum", "Binds gRPC :4317 and HTTP :4318; receives ExportLogsServiceRequest, ExportTraceServiceRequest, ExportMetricsServiceRequest")
    Container(harness, "OTLP conformance harness", "Rust crate (Apache-2.0)", "Validates every payload; rejects malformed records before they reach the sink")
    Container(compose, "Aperture compose / wire_sink", "Rust pub(crate) module", "Builds the OtlpSink stack; at DELIVER time wraps the inner sink with sieve::SamplingSink")
    Container(sieve_lib, "Sieve library", "Rust crate (AGPL-3.0-or-later)", "SamplingSink decorator + HeadSampler + Counters + summary timer task. Handles the SinkRecord::Traces variant; passes Logs and Metrics through unchanged")
    Container(inner_sink, "Inner OtlpSink (StubSink at v0; Sluice or external backend post-v0)", "Rust trait impl", "Receives the kept-traces envelope plus all logs and metrics")
    Container(observability, "Aperture observability + Sieve tracing emitter", "tracing crate", "Emits target=\"aperture\" and target=\"sieve\" events to the global subscriber")
  }

  System_Ext(otel_sdk, "OpenTelemetry SDK", "Application instrumentation")
  System_Ext(log_aggregator, "Operator's log aggregator", "Receives tracing events on target=\"sieve\"")

  Rel(otel_sdk, transport, "Sends OTLP envelopes to", "gRPC / HTTP")
  Rel(transport, harness, "Forwards payloads to", "validate(...)")
  Rel(harness, compose, "Hands accepted records to", "Arc<dyn OtlpSink>::accept")
  Rel(compose, sieve_lib, "Routes records through", "OtlpSink::accept (in-process call)")
  Rel(sieve_lib, inner_sink, "Forwards kept traces and all logs/metrics to", "OtlpSink::accept (in-process call)")
  Rel(sieve_lib, observability, "Emits per-decision DEBUG and periodic INFO via", "tracing macros")
  Rel(observability, log_aggregator, "Pushes structured events to", "operator's tracing subscriber")
  Rel(riley, log_aggregator, "Reads INFO summary from", "log query")
  Rel(riley, process, "Sets SIEVE_NON_ERROR_TRACE_RATE / SIEVE_SUMMARY_TICK_MS via", "deployment manifest env var")
```

## Notes

- The diagram uses C4 "Container" loosely: every Container box above
  compiles into the **same** OS process. The boundary is a
  compilation-unit / crate / module boundary, not a process boundary.
  This matches DISCUSS Q1's "library at v0" decision.
- `Sieve library` is one box at the container level; ADR-0021 §1
  documents the integration via the `SamplingSink<S, N>` decorator
  over the inner `OtlpSink + Probe`.
- The flow `transport → harness → compose → sieve_lib → inner_sink`
  is the single linear path every record takes. There is no
  fan-out, no cross-thread channel, no buffering layer between
  Aperture and Sieve at v0.
- `observability` is shown as a single Container because Aperture's
  and Sieve's tracing emissions both go through the same global
  `tracing` subscriber. Sieve uses `target="sieve"` to keep the
  vocabulary distinct.
- The `Probe` invocation flow is not shown in the runtime diagram
  because it fires once at startup, before any traffic. ADR-0021 §3
  documents the startup-refusal semantics.
