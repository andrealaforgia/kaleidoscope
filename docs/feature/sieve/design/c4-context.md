# Sieve v0 — C4 System Context (L1)

The system context shows Sieve's place in the Kaleidoscope pipeline at
v0. Sieve is a library inside Aperture's process boundary; the
operator-facing personas are Riley (SRE) and Sasha (platform
engineer) per `docs/feature/sieve/discuss/journey-sieve.yaml`.

```mermaid
C4Context
  title System Context — Sieve v0

  Person(riley, "Riley (SRE)", "Reads the periodic INFO summary; configures SIEVE_NON_ERROR_TRACE_RATE; trusts Sieve to retain error traces")
  Person(sasha, "Sasha (platform engineer)", "Wires Sieve into Aperture's composition root; treats Sieve as a library dependency")

  System_Boundary(kaleidoscope, "Kaleidoscope") {
    System(aperture, "Aperture", "OTLP gateway. Validates inbound payloads through the conformance harness; forwards typed records to the configured OtlpSink. AGPL-3.0-or-later")
    System(sieve, "Sieve (this feature)", "Head-based trace sampler with error-bias retention. AGPL-3.0-or-later library inside Aperture's process. Logs and metrics passthrough at v0")
    System(downstream, "Next pipeline stage (Sluice or external OTel-compatible backend)", "Receives the kept-traces envelope plus all logs and metrics. Out of scope at v0")
  }

  System_Ext(otel_sdk, "OpenTelemetry SDK in instrumented application", "Emits OTLP to Aperture over gRPC :4317 or HTTP :4318")
  System_Ext(log_aggregator, "Operator's log aggregator", "Receives Sieve's tracing events on target=\"sieve\"")

  Rel(otel_sdk, aperture, "Sends OTLP traces, logs, metrics over", "gRPC / HTTP")
  Rel(aperture, sieve, "Routes inbound records through", "OtlpSink trait (in-process)")
  Rel(sieve, downstream, "Forwards kept traces and all logs/metrics to", "OtlpSink trait (in-process)")
  Rel(sieve, log_aggregator, "Emits per-decision DEBUG events and periodic INFO summary on", "tracing target=\"sieve\"")
  Rel(riley, sieve, "Configures SIEVE_NON_ERROR_TRACE_RATE for", "env var at startup")
  Rel(riley, log_aggregator, "Reads sampling-decision summary from", "log query")
  Rel(sasha, aperture, "Wires Sieve into Aperture's composition root via", "compose::wire_sink at DELIVER time")
```

## Notes

- Sieve is **inside Aperture's process boundary** at v0. The L1 box
  shows it as a separate system because the AGPL-licensed library is
  a separate compilation unit with its own ADRs and CI gates, but the
  data flow between Aperture and Sieve is in-process function calls,
  not network hops.
- The "Next pipeline stage" is Sluice or an external OTel-compatible
  backend. Sluice is out of scope at v0; the integration is via
  whatever inner `OtlpSink` Aperture's composition root constructs.
- The operator-facing flow has two arrows from Riley: one to
  configure the rate, one to read the summary. Riley does not
  interact with Sieve directly; she interacts with the deployment
  manifest (env var) and the log aggregator (summary readout).
- Sasha interacts with Aperture's composition root, not Sieve
  directly. The DELIVER-wave wiring is the platform-engineering
  touchpoint.
