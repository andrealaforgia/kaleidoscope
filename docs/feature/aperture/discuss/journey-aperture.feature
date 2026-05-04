Feature: Aperture v0 — OTLP gateway journey
  As an OpenTelemetry SDK client (the consumer)
  I want a Kaleidoscope-component endpoint that accepts standard OTLP
  exports, validates them via the conformance harness, hands accepted
  records to a sink, and remains observable, backpressure-aware, and
  gracefully restartable
  So that I can integrate Kaleidoscope into my application's telemetry
  pipeline with the same confidence I have in any other OTel-compatible
  receiver.

  # ---------------------------------------------------------------------
  # Backbone Activity 1 — Bind listeners
  # ---------------------------------------------------------------------

  Scenario: Both OTLP listeners bind on configured ports
    Given Aperture is started with bind addrs grpc=0.0.0.0:4317 and http=0.0.0.0:4318
    When the process completes startup
    Then a TCP listener is accepting connections on 4317
    And a TCP listener is accepting connections on 4318
    And stderr contains a JSON line with event=listener_bound transport=grpc addr=0.0.0.0:4317
    And stderr contains a JSON line with event=listener_bound transport=http_protobuf addr=0.0.0.0:4318
    And GET /readyz returns 200 with body "ready"

  Scenario: Port already in use produces a structured failure
    Given another process is already listening on 0.0.0.0:4317
    When Aperture is started with the default config
    Then Aperture exits with a non-zero status code
    And stderr contains a JSON line with level=error, event=listener_bind_failed, transport=grpc, addr=0.0.0.0:4317
    And /readyz never returned 200 during the process lifetime

  Scenario: TLS knob set true on v0 emits a warning and continues plaintext
    Given the configuration sets aperture.security.tls.enabled = true
    When Aperture is started
    Then stderr contains a JSON line with level=warn, event=tls_not_supported_in_v0
    And the listeners bind in plaintext mode
    And GET /readyz returns 200 with body "ready"

  # ---------------------------------------------------------------------
  # Backbone Activity 2 — Receive payload
  # ---------------------------------------------------------------------

  Scenario: A real OpenTelemetry Rust SDK exports a logs batch over gRPC
    Given an OpenTelemetry Rust SDK 0.27 configured with endpoint http://localhost:4317
    And the SDK is emitting one log record with resource.service.name="payments-api"
    When the SDK calls its OTLP/gRPC log exporter once
    Then Aperture reads the full ExportLogsServiceRequest body before validation
    And stderr contains a JSON line with event=request_received transport=grpc signal=logs

  Scenario: An HTTP/protobuf POST with the wrong content type is refused
    Given Aperture's HTTP listener is bound on port 4318
    When a client POSTs /v1/logs with Content-Type "application/json"
    Then the response status is 415
    And stderr contains a JSON line with level=warn event=unsupported_media_type

  Scenario: An HTTP/protobuf POST to an unknown OTLP path is refused
    Given Aperture's HTTP listener is bound on port 4318
    When a client POSTs /v1/profiles with Content-Type "application/x-protobuf"
    Then the response status is 404

  Scenario: A body exceeding max_recv_msg_size is refused with a clear status
    # NB: max_recv_msg_size is set to 1 MiB here for fast-driving the test scenario; the v0 default is 4 MiB.
    Given Aperture's HTTP listener is configured with max_recv_msg_size=1048576
    When a client POSTs /v1/logs with a 2 MiB body
    Then the response status is 413
    And stderr contains a JSON line with level=warn event=body_too_large signal=logs bytes=2097152

  # ---------------------------------------------------------------------
  # Backbone Activity 3 — Validate via harness
  # ---------------------------------------------------------------------

  Scenario: An empty gRPC body is rejected with INVALID_ARGUMENT
    Given Aperture's gRPC listener is accepting connections on port 4317
    When a client opens a gRPC stream and sends a zero-length ExportLogsServiceRequest body
    Then the response gRPC status is 3 (INVALID_ARGUMENT)
    And the grpc-message contains "rule=EmptyInput"
    And the grpc-message contains "signal=Logs"
    And the grpc-message contains "framing=GrpcProtobuf"

  Scenario: An HTTP POST with traces bytes to /v1/logs returns SignalMismatch
    Given Aperture's HTTP listener is accepting connections on port 4318
    And a real ExportTraceServiceRequest body has been captured from the OpenTelemetry Rust SDK
    When the client POSTs that body to /v1/logs with Content-Type application/x-protobuf
    Then the response status is 400
    And the response body contains "rule=WireType::SignalMismatch"
    And the response body contains "observed=Traces"
    And the response body contains "asserted=Logs"

  Scenario: A truncated logs body is rejected with ProtobufDecode
    Given a real ExportLogsServiceRequest body has been captured and truncated at byte 50
    When the client POSTs that truncated body to /v1/logs with Content-Type application/x-protobuf
    Then the response status is 400
    And the response body contains "rule=WireType::ProtobufDecode"

  Scenario: A valid ExportLogsServiceRequest is accepted on gRPC
    Given a real ExportLogsServiceRequest body has been captured from the OpenTelemetry Rust SDK
    When the client sends that body over gRPC to localhost:4317
    Then the harness call returns Ok with the upstream typed record
    And the request proceeds to the sink hand-off step
    And the SDK ultimately receives gRPC status 0 (OK)

  Scenario: A valid ExportTraceServiceRequest is accepted on HTTP
    Given a real ExportTraceServiceRequest body has been captured from the OpenTelemetry Rust SDK
    When the client POSTs that body to /v1/traces with Content-Type application/x-protobuf
    Then the harness call returns Ok with the upstream typed record
    And the SDK ultimately receives HTTP 200

  Scenario: A valid ExportMetricsServiceRequest is accepted on gRPC
    Given a real ExportMetricsServiceRequest body has been captured from the OpenTelemetry Rust SDK
    When the client sends that body over gRPC to localhost:4317
    Then the harness call returns Ok with the upstream typed record
    And the SDK ultimately receives gRPC status 0 (OK)

  # ---------------------------------------------------------------------
  # Backbone Activity 4 — Hand off to sink
  # ---------------------------------------------------------------------

  Scenario: StubSink acknowledges a valid logs record by logging it
    Given Aperture is configured with sink=stub
    And a valid ExportLogsServiceRequest with resource.service.name="payments-api" and 3 log records is received
    When the request reaches the sink hand-off step
    Then sink.accept returns Ok(())
    And stderr contains a JSON line with event=sink_accepted, sink=stub, signal=logs, record_count=3, resource.service.name="payments-api"
    And the SDK receives gRPC status 0 (OK)

  Scenario: ForwardingSink writes downstream and propagates success
    Given Aperture is configured with sink=forwarding, endpoint=http://otel-backend:4318
    And the configured downstream backend is healthy
    When a valid ExportLogsServiceRequest is received over gRPC
    Then ForwardingSink POSTs the typed record to http://otel-backend:4318/v1/logs
    And sink.accept returns Ok(())
    And the SDK receives gRPC status 0 (OK)

  Scenario: ForwardingSink refusal becomes UNAVAILABLE upstream
    Given Aperture is configured with sink=forwarding, endpoint=http://otel-backend:4318
    And the configured downstream backend is returning HTTP 503
    When a valid ExportLogsServiceRequest is received over gRPC
    Then sink.accept returns Err(SinkError::DownstreamUnavailable)
    And the SDK receives gRPC status 14 (UNAVAILABLE)
    And stderr contains a JSON line with level=error, event=sink_failed, sink=forwarding

  # ---------------------------------------------------------------------
  # Backbone Activity 5 — Observe self
  # ---------------------------------------------------------------------

  Scenario: Liveness probe always succeeds while the process is up
    Given Aperture's HTTP listener is bound on port 4318
    When a client GETs /healthz
    Then the response status is 200
    And the body is "ok"

  Scenario: Readiness probe is 503 during startup, 200 once listeners are bound
    Given Aperture has just been launched
    When a client GETs /readyz before listeners have bound
    Then the response status is 503
    And the body is "starting"
    When both listeners have bound
    And the client GETs /readyz again
    Then the response status is 200
    And the body is "ready"

  Scenario: Aperture emits no telemetry-on-telemetry
    Given Aperture is running and has accepted 100 valid OTLP requests
    And Aperture is configured with sink=stub
    When the operator inspects the network traffic Aperture has originated
    Then Aperture has not opened any outbound connection on port 4317 or 4318
    And Aperture has not exposed a /metrics endpoint
    And Aperture's only outbound network traffic is from ForwardingSink (when configured) to the operator-specified downstream endpoint

  Scenario: Aperture continues serving traffic when stderr writes fail
    Given Aperture is running with the StubSink configuration
    And the process's stderr file descriptor has been redirected to a closed pipe (so writes return EPIPE)
    When a valid ExportLogsServiceRequest is received over gRPC
    Then the harness validation completes
    And the sink hand-off completes
    And the SDK receives gRPC status 0 (OK)
    And the process is still running afterwards (stderr backpressure does not bring Aperture down)

  # ---------------------------------------------------------------------
  # Backbone Activity 6 — Shut down gracefully
  # ---------------------------------------------------------------------

  Scenario: Graceful shutdown drains in-flight requests
    Given Aperture is running and has 7 in-flight requests
    When the process receives SIGTERM
    Then /readyz returns 503 "draining" within 100 ms
    And stderr contains a JSON line with event=shutdown_initiated, signal=SIGTERM
    And no new connections are accepted on either listener after the readiness flip
    And the 7 in-flight requests complete (sink-acknowledged and responded to client)
    And stderr contains a JSON line with event=in_flight_drained, drained_count=7
    And the process exits with status code 0

  Scenario: Drain deadline exceeded is observable, never silent
    Given Aperture is running with drain_deadline_ms=1000
    And 3 in-flight requests are blocked on a slow sink
    When the process receives SIGTERM
    And the drain deadline elapses with the requests still in-flight
    Then stderr contains a JSON line with level=warn, event=drain_deadline_exceeded, dropped_count=3
    And the process exits with status code 1

  # ---------------------------------------------------------------------
  # Cross-cutting — Backpressure
  # ---------------------------------------------------------------------

  Scenario: gRPC concurrency cap reached returns RESOURCE_EXHAUSTED
    Given Aperture's gRPC transport is configured with max_concurrent_requests=4
    And 4 requests are currently in-flight on the gRPC listener
    When a 5th client opens a gRPC stream and begins an Export call
    Then the 5th request receives gRPC status 8 (RESOURCE_EXHAUSTED)
    And the grpc-message names the configured concurrency cap
    And stderr contains a JSON line with level=warn, event=concurrency_cap_hit, transport=grpc, cap=4

  Scenario: HTTP concurrency cap reached returns 503 with Retry-After
    Given Aperture's HTTP transport is configured with max_concurrent_requests=4
    And 4 requests are currently in-flight on the HTTP listener
    When a 5th client POSTs /v1/logs
    Then the 5th request receives HTTP 503
    And the response includes a "Retry-After: 1" header
    And the response body names the configured concurrency cap
    And stderr contains a JSON line with level=warn, event=concurrency_cap_hit, transport=http_protobuf, cap=4

  @property
  Scenario: Backpressure never silently drops a request
    Given Aperture is running with any configured concurrency cap
    Then for every request that exceeds the cap, the client receives a deterministic refusal status (gRPC RESOURCE_EXHAUSTED or HTTP 503)
    And no request beyond the cap is silently buffered or discarded
    And every refusal is observable on stderr as a structured JSON line
