# DISTILL-wave Gherkin scenarios for Aperture v0.
#
# This file extends `discuss/journey-aperture.feature` (locked at
# DISCUSS) with the executable-test-shaped scenarios DISTILL produced.
# Where a DISCUSS scenario is unchanged, it is referenced by name
# (Gherkin's @from-discuss tag) and not duplicated. New scenarios
# below are the slice-shaped form the integration tests under
# `crates/aperture/tests/slice_*.rs` ride 1:1 with.
#
# Tag legend (DISTILL conventions):
#   @walking_skeleton  — the slice's user-goal-shaped scenario
#   @real-io           — exercises real loopback transports (Strategy C)
#   @driving_adapter   — enters Aperture through a driving port
#   @adapter-integration — exercises a real driven adapter (Mandate 6)
#   @property          — universal invariant; DELIVER may implement as PBT
#   @kpi               — observability-shaped scenario tied to outcome-kpis.md
#   @from-discuss      — scenario inherited verbatim from journey-aperture.feature
#   @us-ap-NN          — story traceability per user-stories.md

Feature: Aperture v0 — OTLP gateway acceptance scenarios (DISTILL)
  As an OpenTelemetry SDK client (machine), an operator (human reading
  stderr and probing /healthz / /readyz), or a future Sieve component
  (the next-stage `OtlpSink` consumer)
  I want a Kaleidoscope-component endpoint that accepts standard OTLP
  exports, validates each via the conformance harness, hands accepted
  records to a sink, and remains observable, backpressure-aware, and
  gracefully restartable
  So that I can integrate Kaleidoscope into my application's telemetry
  pipeline without bespoke wire-format work, without silent drops, and
  without surprises during rolling restarts.

  Background:
    Given a freshly-launched Aperture instance bound to ephemeral loopback ports
    And the real otlp-conformance-harness is the validation gate
    And an OtlpSink test double records every accepted record

  # =========================================================================
  # Slice 01 — Walking skeleton (gRPC + logs + sink hand-off)
  # =========================================================================
  # Stories: US-AP-01 (gRPC arm), US-AP-03 (gRPC arm), US-AP-04 (gRPC arm)
  # Test file: crates/aperture/tests/slice_01_walking_skeleton.rs

  @walking_skeleton @real-io @driving_adapter @us-ap-01 @us-ap-03 @kpi
  Scenario: Customer exports one log record over gRPC and receives gRPC OK
    Given an OpenTelemetry SDK configured with the gRPC endpoint of the running Aperture
    And the SDK is emitting one log record with resource.service.name "payments-api"
    When the SDK calls its OTLP/gRPC log exporter once
    Then the SDK receives gRPC status 0 (OK)

  @real-io @driving_adapter @us-ap-03 @adapter-integration
  Scenario: Customer exports one log record and the typed record reaches the sink
    Given an OpenTelemetry SDK configured with the gRPC endpoint of the running Aperture
    And the SDK is emitting one log record with resource.service.name "payments-api"
    When the SDK calls its OTLP/gRPC log exporter once
    Then exactly one record reaches the OtlpSink
    And the SinkRecord variant is Logs

  @real-io @driving_adapter @us-ap-03
  Scenario: Customer exports three log records and the sink receives all three
    Given an OpenTelemetry SDK configured with the gRPC endpoint of the running Aperture
    And the SDK is emitting three log records with resource.service.name "checkout-api"
    When the SDK calls its OTLP/gRPC log exporter once
    Then the sink receives one Logs SinkRecord carrying three log records

  @real-io @driving_adapter @us-ap-01 @kpi
  Scenario: Startup emits a listener_bound stderr line for the gRPC transport
    Given Aperture has just been launched
    When startup completes
    Then stderr contains a JSON line with event=listener_bound and transport=grpc

  @real-io @driving_adapter @us-ap-03 @kpi
  Scenario: Customer's request emits a request_received stderr line naming the logs signal
    Given an OpenTelemetry SDK configured with the gRPC endpoint
    When the SDK calls its OTLP/gRPC log exporter once
    Then stderr contains a JSON line with event=request_received and signal=logs

  @real-io @driving_adapter @us-ap-03 @kpi
  Scenario: Sink_accepted line names the record_count
    Given an OpenTelemetry SDK is emitting one log record
    When the SDK calls its OTLP/gRPC log exporter once
    Then stderr contains a JSON line with event=sink_accepted and record_count=1

  @real-io @driving_adapter @us-ap-03 @kpi
  Scenario: Sink_accepted line names the resource.service.name
    Given an OpenTelemetry SDK is emitting one log record with resource.service.name "payments-api"
    When the SDK calls its OTLP/gRPC log exporter once
    Then stderr contains a JSON line with event=sink_accepted and resource.service.name="payments-api"

  @real-io @driving_adapter @us-ap-04
  Scenario: Customer sends empty body over gRPC and receives INVALID_ARGUMENT
    Given Aperture's gRPC listener is accepting connections
    When the SDK sends a zero-records ExportLogsServiceRequest
    Then the SDK receives gRPC status 3 (INVALID_ARGUMENT)

  @real-io @driving_adapter @us-ap-04
  Scenario: Empty-body grpc-message names the EmptyInput rule
    Given Aperture's gRPC listener is accepting connections
    When the SDK sends a zero-records ExportLogsServiceRequest
    Then the grpc-message contains "rule=EmptyInput"

  @real-io @driving_adapter @us-ap-04
  Scenario: Empty-body grpc-message names the Logs signal
    Given Aperture's gRPC listener is accepting connections
    When the SDK sends a zero-records ExportLogsServiceRequest
    Then the grpc-message contains "signal=Logs"

  @real-io @driving_adapter @us-ap-04
  Scenario: Empty-body grpc-message names the GrpcProtobuf framing
    Given Aperture's gRPC listener is accepting connections
    When the SDK sends a zero-records ExportLogsServiceRequest
    Then the grpc-message contains "framing=GrpcProtobuf"

  @real-io @driving_adapter @us-ap-04
  Scenario: Rejected request never reaches the sink
    Given Aperture's gRPC listener is accepting connections
    When the SDK sends a zero-records ExportLogsServiceRequest
    Then no record reaches the OtlpSink

  # =========================================================================
  # Slice 02 — HTTP/protobuf transport + /healthz + /readyz
  # =========================================================================
  # Stories: US-AP-02, US-AP-03 (HTTP arm), US-AP-04 (HTTP arm)
  # Test file: crates/aperture/tests/slice_02_http_protobuf_and_readiness.rs

  @walking_skeleton @real-io @driving_adapter @us-ap-02 @kpi
  Scenario: Operator probes /healthz and receives 200 "ok"
    Given Aperture's HTTP listener is bound on a loopback port
    When the operator GETs /healthz
    Then the response status is 200
    And the response body is "ok"

  @walking_skeleton @real-io @driving_adapter @us-ap-02 @kpi
  Scenario: Operator probes /readyz after startup and receives 200 "ready"
    Given Aperture's HTTP listener is bound and both transports are ready
    When the operator GETs /readyz
    Then the response status is 200
    And the response body is "ready"

  @real-io @driving_adapter @us-ap-03
  Scenario: Customer posts a valid logs body over HTTP and receives 200
    Given Aperture's HTTP listener is bound on a loopback port
    And a real ExportLogsServiceRequest body has been encoded by the OTel SDK shape
    When the client POSTs that body to /v1/logs with Content-Type application/x-protobuf
    Then the response status is 200

  @real-io @driving_adapter @us-ap-03 @adapter-integration
  Scenario: Customer's HTTP POST reaches the sink with one record
    Given Aperture's HTTP listener is bound on a loopback port
    When the client POSTs a one-record body to /v1/logs
    Then exactly one record reaches the OtlpSink

  @real-io @driving_adapter @us-ap-03 @kpi
  Scenario: Sink_accepted line names the http_protobuf transport
    Given Aperture's HTTP listener is bound on a loopback port
    When the client POSTs a one-record body to /v1/logs
    Then stderr contains a JSON line with event=request_received and transport=http_protobuf

  @real-io @driving_adapter @us-ap-02
  Scenario: Customer posts with the wrong Content-Type and receives 415
    Given Aperture's HTTP listener is bound on a loopback port
    When the client POSTs to /v1/logs with Content-Type application/json
    Then the response status is 415

  @real-io @driving_adapter @us-ap-02 @kpi
  Scenario: Wrong Content-Type emits an unsupported_media_type warn line
    Given Aperture's HTTP listener is bound on a loopback port
    When the client POSTs to /v1/logs with Content-Type application/json
    Then stderr contains a JSON line with level=warn and event=unsupported_media_type

  @real-io @driving_adapter @us-ap-02
  Scenario: Wrong Content-Type does not reach the sink
    Given Aperture's HTTP listener is bound on a loopback port
    When the client POSTs to /v1/logs with Content-Type application/json
    Then no record reaches the OtlpSink

  @real-io @driving_adapter @us-ap-02
  Scenario: Customer posts to an unknown OTLP path and receives 404
    Given Aperture's HTTP listener is bound on a loopback port
    When the client POSTs to /v1/profile with any body
    Then the response status is 404

  @real-io @driving_adapter @us-ap-04
  Scenario: Customer posts an empty body over HTTP and receives 400
    Given Aperture's HTTP listener is bound on a loopback port
    When the client POSTs an empty body to /v1/logs with Content-Type application/x-protobuf
    Then the response status is 400

  @real-io @driving_adapter @us-ap-04
  Scenario: Empty-body HTTP response body names the EmptyInput rule
    Given Aperture's HTTP listener is bound on a loopback port
    When the client POSTs an empty body to /v1/logs
    Then the response body contains "rule=EmptyInput"

  @real-io @driving_adapter @us-ap-04
  Scenario: Empty-body HTTP response body names the HttpProtobuf framing
    Given Aperture's HTTP listener is bound on a loopback port
    When the client POSTs an empty body to /v1/logs
    Then the response body contains "framing=HttpProtobuf"

  @real-io @driving_adapter @us-ap-04
  Scenario: Empty-body HTTP response Content-Type is text/plain
    Given Aperture's HTTP listener is bound on a loopback port
    When the client POSTs an empty body to /v1/logs
    Then the response Content-Type contains "text/plain"

  # =========================================================================
  # Slice 03 — Traces signal end-to-end
  # =========================================================================
  # Story: US-AP-05
  # Test file: crates/aperture/tests/slice_03_traces.rs

  @real-io @driving_adapter @us-ap-05 @kpi
  Scenario: Customer exports one span over gRPC and receives gRPC OK
    Given Aperture is running and the SDK is emitting one span with resource.service.name "payments-api"
    When the SDK calls its OTLP/gRPC trace exporter once
    Then the SDK receives gRPC status 0 (OK)

  @real-io @driving_adapter @us-ap-05
  Scenario: Spans reach the sink as a Traces SinkRecord
    Given the SDK is emitting one span
    When the SDK calls the OTLP/gRPC trace exporter once
    Then the SinkRecord variant is Traces

  @real-io @driving_adapter @us-ap-05 @kpi
  Scenario: Sink_accepted line names the traces signal
    Given the SDK is emitting one span
    When the SDK calls the OTLP/gRPC trace exporter once
    Then stderr contains a JSON line with event=sink_accepted and signal=traces

  @real-io @driving_adapter @us-ap-05 @kpi
  Scenario: Sink_accepted line names the span_count
    Given the SDK is emitting three spans
    When the SDK calls the OTLP/gRPC trace exporter once
    Then stderr contains a JSON line with event=sink_accepted and span_count=3

  @real-io @driving_adapter @us-ap-05
  Scenario: Customer posts a traces body over HTTP and receives 200
    Given Aperture's HTTP listener is bound
    When the client POSTs a real ExportTraceServiceRequest body to /v1/traces
    Then the response status is 200

  @real-io @driving_adapter @us-ap-05
  Scenario: Customer posts a logs body to /v1/traces and receives 400
    Given Aperture's HTTP listener is bound
    When the client POSTs a real ExportLogsServiceRequest body to /v1/traces
    Then the response status is 400

  @real-io @driving_adapter @us-ap-05
  Scenario: Logs-to-/v1/traces response body names the SignalMismatch rule
    Given Aperture's HTTP listener is bound
    When the client POSTs a real ExportLogsServiceRequest body to /v1/traces
    Then the response body contains "rule=WireType::SignalMismatch"

  @real-io @driving_adapter @us-ap-05
  Scenario: Logs-to-/v1/traces response body names the observed Logs signal
    Given Aperture's HTTP listener is bound
    When the client POSTs a real ExportLogsServiceRequest body to /v1/traces
    Then the response body contains "observed=Logs"

  @real-io @driving_adapter @us-ap-05
  Scenario: Logs-to-/v1/traces response body names the asserted Traces signal
    Given Aperture's HTTP listener is bound
    When the client POSTs a real ExportLogsServiceRequest body to /v1/traces
    Then the response body contains "asserted=Traces"

  @real-io @driving_adapter @us-ap-05
  Scenario: Logs-to-/v1/traces does not reach the sink
    Given Aperture's HTTP listener is bound
    When the client POSTs a real ExportLogsServiceRequest body to /v1/traces
    Then no record reaches the OtlpSink

  # =========================================================================
  # Slice 04 — Metrics signal end-to-end
  # =========================================================================
  # Story: US-AP-06
  # Test file: crates/aperture/tests/slice_04_metrics.rs

  @real-io @driving_adapter @us-ap-06 @kpi
  Scenario: Customer exports metrics over gRPC and receives gRPC OK
    Given the SDK is emitting one Sum and one Gauge data point
    When the SDK calls its OTLP/gRPC metrics exporter once
    Then the SDK receives gRPC status 0 (OK)

  @real-io @driving_adapter @us-ap-06
  Scenario: Metrics reach the sink as a Metrics SinkRecord
    Given the SDK is emitting metrics
    When the SDK calls the OTLP/gRPC metrics exporter once
    Then the SinkRecord variant is Metrics

  @real-io @driving_adapter @us-ap-06 @kpi
  Scenario: Sink_accepted line names data_point_count=2 for one Sum and one Gauge
    Given the SDK is emitting one Sum and one Gauge data point
    When the SDK calls the OTLP/gRPC metrics exporter once
    Then stderr contains a JSON line with event=sink_accepted and data_point_count=2

  @real-io @driving_adapter @us-ap-06 @kpi
  Scenario: Sink_accepted line names the metrics signal
    Given the SDK is emitting metrics
    When the SDK calls the OTLP/gRPC metrics exporter once
    Then stderr contains a JSON line with event=sink_accepted and signal=metrics

  @real-io @driving_adapter @us-ap-06
  Scenario: Customer posts a metrics body over HTTP and receives 200
    Given Aperture's HTTP listener is bound
    When the client POSTs a real ExportMetricsServiceRequest body to /v1/metrics
    Then the response status is 200

  @real-io @driving_adapter @us-ap-06
  Scenario: Customer posts a traces body to /v1/metrics and receives 400
    Given Aperture's HTTP listener is bound
    When the client POSTs a real ExportTraceServiceRequest body to /v1/metrics
    Then the response status is 400

  @real-io @driving_adapter @us-ap-06
  Scenario: Traces-to-/v1/metrics response body names the SignalMismatch rule
    Given Aperture's HTTP listener is bound
    When the client POSTs a real ExportTraceServiceRequest body to /v1/metrics
    Then the response body contains "rule=WireType::SignalMismatch"

  @real-io @driving_adapter @us-ap-06
  Scenario: Traces-to-/v1/metrics response body names observed=Traces and asserted=Metrics
    Given Aperture's HTTP listener is bound
    When the client POSTs a real ExportTraceServiceRequest body to /v1/metrics
    Then the response body contains "observed=Traces"
    And the response body contains "asserted=Metrics"

  # =========================================================================
  # Slice 05 — Backpressure (concurrency cap, deterministic refusal)
  # =========================================================================
  # Story: US-AP-07
  # Test file: crates/aperture/tests/slice_05_backpressure.rs

  @real-io @driving_adapter @us-ap-07 @kpi
  Scenario: Fifth concurrent gRPC request at cap=4 receives RESOURCE_EXHAUSTED
    Given Aperture's gRPC transport is configured with max_concurrent_requests=4
    And four requests are currently held in-flight by a barrier sink
    When a fifth client opens a gRPC stream and begins an Export call
    Then the fifth request receives gRPC status 8 (RESOURCE_EXHAUSTED)

  @real-io @driving_adapter @us-ap-07
  Scenario: Fifth gRPC request grpc-message names the cap
    Given Aperture's gRPC transport has max_concurrent_requests=4 and four in-flight
    When a fifth client begins an Export call
    Then the grpc-message names the configured cap (cap=4 or "cap of 4")

  @real-io @driving_adapter @us-ap-07 @kpi
  Scenario: gRPC concurrency_cap_hit emits a warn stderr event
    Given Aperture's gRPC transport is saturated at cap=2
    When a third client begins an Export call
    Then stderr contains a JSON line with level=warn and event=concurrency_cap_hit

  @real-io @driving_adapter @us-ap-07
  Scenario: concurrency_cap_hit event names the gRPC transport
    Given Aperture's gRPC transport is saturated
    When the cap is hit
    Then stderr contains a JSON line with event=concurrency_cap_hit and transport=grpc

  @real-io @driving_adapter @us-ap-07 @kpi
  Scenario: Fifth concurrent HTTP request at cap=4 receives 503
    Given Aperture's HTTP transport is configured with max_concurrent_requests=4
    And four HTTP requests are currently in-flight on a barrier sink
    When a fifth client POSTs /v1/logs
    Then the response status is 503

  @real-io @driving_adapter @us-ap-07
  Scenario: Fifth HTTP request includes a Retry-After header of 1
    Given Aperture's HTTP transport has cap=4 and four in-flight
    When a fifth client POSTs /v1/logs
    Then the response includes header "Retry-After: 1"

  @real-io @driving_adapter @us-ap-07
  Scenario: Fifth HTTP request body names the cap
    Given Aperture's HTTP transport has cap=4 and four in-flight
    When a fifth client POSTs /v1/logs
    Then the response body names the configured cap

  @real-io @driving_adapter @us-ap-07
  Scenario: Concurrency_cap_hit event names the http_protobuf transport
    Given Aperture's HTTP transport is saturated at cap=2
    When a third client POSTs /v1/logs
    Then stderr contains a JSON line with event=concurrency_cap_hit and transport=http_protobuf

  @real-io @driving_adapter @us-ap-07
  Scenario: Saturated gRPC transport does not block HTTP requests
    Given Aperture's gRPC transport is saturated at cap=2
    And HTTP transport's cap is 2 with zero in-flight
    When a client POSTs /v1/logs over HTTP
    Then the HTTP request receives status 200

  @property @real-io @driving_adapter @us-ap-07 @kpi
  Scenario: Backpressure never silently drops a request
    Given Aperture's HTTP transport has cap=2
    When ten clients POST /v1/logs concurrently
    Then every response is either status 200 (sink-accepted) or status 503 (cap-refused)
    And no response is a connection drop, a timeout, or any other status

  # =========================================================================
  # Slice 06 — ForwardingSink (downstream OTLP write)
  # =========================================================================
  # Story: US-AP-08
  # Test file: crates/aperture/tests/slice_06_forwarding_sink.rs

  @real-io @adapter-integration @us-ap-08 @kpi
  Scenario: ForwardingSink probe succeeds against an OPTIONS responder
    Given a wiremock downstream that returns 204 to OPTIONS /v1/logs
    And Aperture is configured with sink=forwarding pointing at the wiremock URL
    When Aperture starts up
    Then the composition root proceeds past the probe and listeners bind

  @real-io @adapter-integration @us-ap-08
  Scenario: ForwardingSink probe falls back to POST when OPTIONS returns 405
    Given a wiremock downstream that returns 405 to OPTIONS but 200 to POST
    When Aperture starts up
    Then the degraded probe (zero-records POST) succeeds and listeners bind

  @real-io @adapter-integration @us-ap-08
  Scenario: ForwardingSink probe refuses startup when the downstream lies (200 to OPTIONS, 503 to POST)
    Given a wiremock downstream that returns 200 to OPTIONS but 503 to POST
    When Aperture starts up
    Then startup is refused
    And stderr contains a JSON line with level=error and event=health.startup.refused

  @real-io @adapter-integration @us-ap-08 @kpi
  Scenario: Customer exports one log record and the downstream receives a protobuf POST
    Given a wiremock downstream that returns 204 to OPTIONS and 200 to POST
    And Aperture is configured with sink=forwarding pointing at the wiremock URL
    When the SDK calls its OTLP/gRPC log exporter once
    Then the wiremock downstream has received exactly one POST to /v1/logs

  @real-io @adapter-integration @us-ap-08 @kpi
  Scenario: Sink_accepted event for ForwardingSink names sink=forwarding
    Given Aperture is running with sink=forwarding pointing at a healthy wiremock
    When the SDK exports one log record
    Then stderr contains a JSON line with event=sink_accepted and sink=forwarding

  @real-io @adapter-integration @us-ap-08
  Scenario: Sink_accepted event for ForwardingSink includes downstream_latency_ms
    Given Aperture is running with sink=forwarding pointing at a healthy wiremock
    When the SDK exports one log record
    Then stderr contains a JSON line with event=sink_accepted and a numeric downstream_latency_ms field

  @real-io @adapter-integration @us-ap-08 @kpi
  Scenario: Customer exports when downstream returns 503 and receives gRPC UNAVAILABLE
    Given a wiremock downstream that returns 204 to OPTIONS but 503 to POST
    And Aperture is running with sink=forwarding pointing at the wiremock URL
    When the SDK calls its OTLP/gRPC log exporter once
    Then the SDK receives gRPC status 14 (UNAVAILABLE)

  @real-io @adapter-integration @us-ap-08
  Scenario: ForwardingSink failure emits sink_failed stderr event
    Given Aperture is running with a downstream that returns 503 to POSTs
    When the SDK exports one log record
    Then stderr contains a JSON line with level=error and event=sink_failed

  @real-io @adapter-integration @us-ap-08
  Scenario: ForwardingSink probe refuses startup when the endpoint is unreachable
    Given Aperture is configured with sink=forwarding pointing at a port that nothing is listening on
    When Aperture starts up
    Then startup is refused with a probe-error event

  @real-io @adapter-integration @us-ap-08
  Scenario: Customer exports when downstream hangs past timeout and receives UNAVAILABLE
    Given a wiremock downstream that delays POST responses past Aperture's timeout
    When the SDK exports one log record
    Then the SDK receives gRPC status 14 (UNAVAILABLE)

  # =========================================================================
  # Slice 07 — TLS / SPIFFE schema knob (forward-compat insurance)
  # =========================================================================
  # No companion story (only @infrastructure slice in v0)
  # Test file: crates/aperture/tests/slice_07_tls_schema_knob.rs

  @real-io @driving_adapter
  Scenario: Default-security config (TLS+SPIFFE keys at defaults) parses without error
    Given a TOML config containing tls.enabled=false and spiffe.enabled=false
    When the loader parses the TOML
    Then parsing succeeds with no error

  @real-io @driving_adapter
  Scenario: tls.enabled=true emits a tls_not_supported_in_v0 warn line
    Given a TOML config with tls.enabled=true
    When Aperture starts up
    Then stderr contains a JSON line with level=warn and event=tls_not_supported_in_v0

  @real-io @driving_adapter
  Scenario: tls.enabled=true emits exactly one warn line
    Given a TOML config with tls.enabled=true
    When Aperture starts up
    Then exactly one tls_not_supported_in_v0 event is emitted

  @real-io @driving_adapter
  Scenario: tls.enabled=true plaintext continues; /readyz reaches 200
    Given a TOML config with tls.enabled=true
    When Aperture starts up
    And the operator GETs /readyz
    Then the response status is 200

  @real-io @driving_adapter
  Scenario: spiffe.enabled=true emits an analogous warn line
    Given a TOML config with spiffe.enabled=true
    When Aperture starts up
    Then stderr contains a warn-level event indicating SPIFFE is not supported in v0

  @real-io @driving_adapter
  Scenario: Config with security keys omitted does not emit a tls warn line
    Given a TOML config with no security section
    When Aperture starts up
    Then stderr contains no tls_not_supported_in_v0 event

  @real-io @driving_adapter
  Scenario: Config with an unknown key is rejected at load
    Given a TOML config with a misspelled key (e.g. "max_concurent_requests")
    When the loader parses the TOML
    Then parsing fails with a config-load error

  # =========================================================================
  # Slice 08 — Graceful shutdown (drain in-flight, observable verdict)
  # =========================================================================
  # Story: US-AP-09
  # Test file: crates/aperture/tests/slice_08_graceful_shutdown.rs

  @real-io @driving_adapter @us-ap-09 @kpi
  Scenario: Shutdown flips /readyz to 503 "draining" within 100 ms
    Given Aperture is running with both listeners bound
    When the operator initiates shutdown
    Then within 100 ms /readyz returns status 503 with body "draining"

  @real-io @driving_adapter @us-ap-09 @kpi
  Scenario: In-flight request completes when drain finishes within deadline
    Given Aperture is running with drain_deadline=5s and a slow sink
    And one request is in-flight
    When the operator initiates shutdown
    And the slow sink releases within the deadline
    Then the in-flight request completes successfully
    And shutdown completes cleanly

  @real-io @driving_adapter @us-ap-09 @kpi
  Scenario: Clean drain emits an in_flight_drained stderr event
    Given Aperture is running with one in-flight request and a fast slow sink
    When the operator initiates shutdown
    Then stderr contains a JSON line with level=info and event=in_flight_drained

  @real-io @driving_adapter @us-ap-09
  Scenario: Shutdown_initiated event carries a signal field
    Given Aperture is running
    When the operator initiates shutdown
    Then stderr contains a JSON line with event=shutdown_initiated and a signal field

  @real-io @driving_adapter @us-ap-09 @kpi
  Scenario: Drain deadline exceeded emits a warn stderr event with dropped_count
    Given Aperture is running with drain_deadline=200ms and a sink that takes 5s
    And one request is in-flight when shutdown initiates
    When the deadline elapses
    Then stderr contains a JSON line with level=warn and event=drain_deadline_exceeded and a dropped_count field

  @from-discuss @us-ap-09
  Scenario: SIGTERM and SIGINT behave identically (DELIVER fixture)
    # The integration-test seam for "send a real SIGTERM to the
    # process" is non-trivial (requires forking a separate process).
    # DISTILL declares the intent as a `#[cfg(unix)] #[ignore]`d test;
    # DELIVER lands the process-spawning fixture if Handle::shutdown
    # is not a satisfactory proxy.

  # =========================================================================
  # Invariants — telemetry-on-telemetry forbidden, single validator per signal
  # =========================================================================
  # Test files: crates/aperture/tests/invariant_no_telemetry_on_telemetry.rs
  #             crates/aperture/tests/invariant_single_validator.rs

  @real-io @driving_adapter @kpi
  Scenario: Aperture does not expose a /metrics endpoint
    Given Aperture is running
    When the operator GETs /metrics
    Then the response status is 404

  @real-io @driving_adapter
  Scenario: Aperture does not expose a /telemetry endpoint
    Given Aperture is running
    When the operator GETs /telemetry
    Then the response status is 404

  @real-io @driving_adapter
  Scenario: Aperture with stub sink idle does not open any outbound connection
    Given Aperture is running with sink=stub
    When the instance is left idle for a brief observation window
    Then no outbound connection is opened
    # The load-bearing defence is a network-namespace integration
    # fixture owned by DEVOPS.

  @real-io @driving_adapter
  Scenario: One export produces exactly one record in the sink
    Given Aperture is running
    When the SDK exports one log record
    Then exactly one record reaches the OtlpSink
    # Behavioural corroboration of the AST-walk static check
    # (single_validator_per_signal) owned by DEVOPS.
