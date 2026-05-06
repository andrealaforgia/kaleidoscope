Feature: Spark v0 — Kaleidoscope Rust SDK init journey
  As an application developer instrumenting a Rust service for Kaleidoscope
  I want a thin Apache-2.0 wrapper around the OpenTelemetry SDK that
  injects Kaleidoscope's house resource attributes (service.name, optional
  tenant.id, optional feature_flag.*, optional experiment.id) on every
  emitted signal, lints required attributes at startup so misconfiguration
  is caught at init time rather than emitted to the wire, ships to the
  operator's Aperture endpoint over OTLP/gRPC by default, and flushes
  pending exports synchronously when the returned guard drops
  So that I can integrate Kaleidoscope into my application's telemetry
  pipeline with one function call and the same confidence I have in the
  upstream OTel SDK itself.

  # ---------------------------------------------------------------------
  # Backbone Activity 1 — Configure (SparkConfig builder)
  # ---------------------------------------------------------------------

  Scenario: SparkConfig builder accepts the canonical configuration
    Given a Rust developer writing a service named "payments-api"
    When they call SparkConfig::for_service("payments-api")
    And chain .require_tenant_id()
    And chain .with_tenant_id("acme-prod")
    And chain .with_feature_flags([("checkout-v2", "on")])
    And chain .with_experiment_id("exp-2026-Q2-pricing")
    Then the resulting SparkConfig holds those values
    And no telemetry has been emitted

  Scenario: SparkConfig is plain data with no I/O
    Given a SparkConfig built from a sequence of builder calls
    When the test observes stderr, stdout, and the application's tracing facade
    Then no output has been written to any of those three channels
    And no OTLP export has reached any backend

  # ---------------------------------------------------------------------
  # Backbone Activity 2 — Lint (spark::init validates the config)
  # ---------------------------------------------------------------------

  Scenario: spark::init refuses missing required tenant.id with a precise error
    Given a SparkConfig built with for_service("payments-api").require_tenant_id() but no with_tenant_id call
    When the application calls spark::init(config)
    Then the call returns Err(SparkError::MissingRequiredAttribute { name: "tenant.id" })
    And no OTLP exporter was constructed
    And no telemetry has reached any backend

  Scenario: spark::init refuses empty-string tenant.id with the same error as missing
    Given a SparkConfig with require_tenant_id().with_tenant_id("")
    When the application calls spark::init(config)
    Then the call returns Err(SparkError::MissingRequiredAttribute { name: "tenant.id" })

  Scenario: spark::init refuses an invalid endpoint with a precise diagnostic
    Given a SparkConfig with with_endpoint("htp://typo:4317")
    When the application calls spark::init(config)
    Then the call returns Err(SparkError::InvalidEndpoint { ... })
    And the reason field names the parse failure
    And no OTLP exporter was constructed

  Scenario: spark::init refuses a second call in the same process
    Given spark::init has already returned Ok in this process
    When the application calls spark::init(config) a second time
    Then the call returns Err(SparkError::GlobalAlreadyInitialised)

  # ---------------------------------------------------------------------
  # Backbone Activity 3 — Initialise SDK (Resource composed, providers set)
  # ---------------------------------------------------------------------

  Scenario: spark::init constructs the OTel SDK with all four house attributes on the Resource
    Given a SparkConfig with service.name="payments-api", tenant.id="acme-prod", feature_flag={"checkout-v2":"on"}, experiment.id="exp-2026-Q2-pricing"
    When the application calls spark::init(config)
    Then the returned SparkGuard is Ok
    And the OTel global tracer provider's Resource includes service.name="payments-api"
    And the Resource includes tenant.id="acme-prod"
    And the Resource includes feature_flag.checkout-v2="on"
    And the Resource includes experiment.id="exp-2026-Q2-pricing"

  Scenario: spark::init writes its own diagnostic to the tracing facade, not the OTel pipeline
    Given the application has subscribed to its tracing facade and to a RecordingSink behind Aperture
    When spark::init succeeds
    Then exactly one tracing event with target="spark" and message containing "spark::init succeeded" is captured by the application's subscriber
    And no ExportTraceServiceRequest reaches the RecordingSink as a result of the init itself

  Scenario: SparkConfig::with_endpoint takes precedence over OTEL_EXPORTER_OTLP_ENDPOINT
    Given OTEL_EXPORTER_OTLP_ENDPOINT="http://env-endpoint:4317" is set in the environment
    And a SparkConfig built with .with_endpoint("http://config-endpoint:4317")
    When the application calls spark::init(config)
    Then the OTel exporter targets http://config-endpoint:4317
    And the resolved-config tracing event names http://config-endpoint:4317

  Scenario: OTEL_EXPORTER_OTLP_ENDPOINT is honoured when SparkConfig::with_endpoint is not called
    Given OTEL_EXPORTER_OTLP_ENDPOINT="http://env-endpoint:4317" is set
    And a SparkConfig built without with_endpoint
    When the application calls spark::init(config)
    Then the OTel exporter targets http://env-endpoint:4317

  # ---------------------------------------------------------------------
  # Backbone Activity 4 — Emit telemetry (standard OTel API + house attrs)
  # ---------------------------------------------------------------------

  Scenario: A traces export carries all four house attributes on the Resource
    Given an Aperture instance running locally with a RecordingSink
    And spark::init has succeeded with service.name="payments-api", tenant.id="acme-prod", feature_flag={"checkout-v2":"on"}, experiment.id="exp-2026-Q2-pricing"
    When the application records one span via opentelemetry::global::tracer("checkout-service").in_span("checkout.complete", ...)
    And the SparkGuard is dropped
    Then the RecordingSink received exactly one ExportTraceServiceRequest
    And the request's first ResourceSpans.resource.attributes contains service.name="payments-api"
    And the same Resource contains tenant.id="acme-prod"
    And the same Resource contains feature_flag.checkout-v2="on"
    And the same Resource contains experiment.id="exp-2026-Q2-pricing"

  Scenario: A logs export carries the same four house attributes on the Resource
    Given an Aperture instance running locally with a RecordingSink
    And spark::init has succeeded with the canonical configuration
    When the application emits one log record via opentelemetry::global::logger_provider().logger("checkout-service")
    And the SparkGuard is dropped
    Then the RecordingSink received an ExportLogsServiceRequest
    And the request's ResourceLogs.resource.attributes contains all four house attributes

  Scenario: A metrics export carries the same four house attributes on the Resource
    Given an Aperture instance running locally with a RecordingSink
    And spark::init has succeeded with the canonical configuration
    When the application increments one counter via opentelemetry::global::meter("checkout-service").u64_counter("orders.processed")
    And the SparkGuard is dropped
    Then the RecordingSink received an ExportMetricsServiceRequest
    And the request's ResourceMetrics.resource.attributes contains all four house attributes

  Scenario: A SparkConfig without require_tenant_id() succeeds without a tenant.id
    Given a SparkConfig built with for_service("payments-api") only
    When the application calls spark::init(config)
    And records one span
    And the guard drops
    Then the RecordingSink received the span
    And the Resource contains service.name="payments-api"
    And the Resource does NOT contain tenant.id

  # ---------------------------------------------------------------------
  # Backbone Activity 5 — Shutdown / flush (SparkGuard::Drop)
  # ---------------------------------------------------------------------

  Scenario: SparkGuard drop flushes pending exports within the configured deadline
    Given an Aperture instance running locally with a RecordingSink
    And spark::init has succeeded
    And the application has recorded 7 spans without flushing
    When the SparkGuard is dropped
    Then the RecordingSink eventually receives at least one ExportTraceServiceRequest with span_count summing to 7
    And one tracing INFO event with target="spark" and message containing "shutdown complete drained=unknown" is captured
    And the drop completes within the configured flush_timeout_ms

  Scenario: SparkGuard drop emits a deadline-exceeded warning when downstream is slow
    Given an Aperture instance configured to delay every accept by 10 seconds
    And the SparkConfig was built with .with_flush_timeout(Duration::from_millis(500))
    And the application has recorded 3 spans
    When the SparkGuard is dropped
    Then one tracing WARN event with target="spark" and message containing "flush deadline exceeded" is captured
    And the WARN event names the dropped count
    And the drop completes within ~500 ms (no indefinite block)

  Scenario: drop(guard) called explicitly is equivalent to scope-exit drop
    Given a SparkGuard returned from spark::init
    When the application calls drop(guard) before main returns
    Then the flush behaviour is identical to letting the guard drop at end of scope
    And one tracing INFO event with target="spark" and message containing "shutdown complete" is captured

  # ---------------------------------------------------------------------
  # Cross-cutting properties (every slice defends these)
  # ---------------------------------------------------------------------

  @property
  Scenario: No telemetry from telemetry — Spark never emits its own diagnostics through the OTel pipeline
    Given a Spark-instrumented application running with a RecordingSink behind Aperture
    When the application's lifecycle is observed end to end (init, emit, drop)
    Then every ExportTraceServiceRequest / ExportLogsServiceRequest / ExportMetricsServiceRequest received by the RecordingSink carries the application's service.name on its Resource
    And no record carries service.name="spark" or any other Spark-internal identifier

  @property
  Scenario: House-attribute completeness — every emitted signal carries every set house attribute
    Given a SparkConfig with all four house attributes set
    When the application emits N spans, M log records, and K metric data points across the application's lifetime
    Then every emitted ExportTraceServiceRequest, ExportLogsServiceRequest, and ExportMetricsServiceRequest carries all four house attributes on its Resource
    And no signal exists with a partial Resource
