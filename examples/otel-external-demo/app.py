"""A tiny, readable demo that emits OpenTelemetry using ONLY the official SDK.

This app depends on NOTHING first-party. It speaks plain OTLP/HTTP to whatever
endpoint OTEL_EXPORTER_OTLP_ENDPOINT points at, using the official
`opentelemetry`, `opentelemetry.sdk` and `opentelemetry.exporter.otlp.*`
packages. Point it at any OTLP/HTTP collector and it just works.

What it does:
  * Emits ONE trace: a parent span and a child span created in the parent's
    context (so they share a trace id and the child's parent is the parent).
  * Stamps a SPAN-level attribute customer.id="bea-test" on the CHILD span.
  * Emits ONE log record from inside the child's active span context, so the
    SDK stamps the trace/span context onto the log (the PG-2 seed).
  * Prints ONE JSON handshake line on stdout carrying the ids and the exact
    nanosecond timestamps the SDK recorded, so an automated test can assert an
    exact round-trip. All human-readable notes go to stderr.

Run it:
  OTEL_EXPORTER_OTLP_ENDPOINT=http://127.0.0.1:4318 python app.py

Environment:
  OTEL_EXPORTER_OTLP_ENDPOINT  base URL of the OTLP/HTTP collector (required)
  KALEIDOSCOPE_TENANT          tenant filed as resource attr tenant.id (acme)
  OTEL_SERVICE_NAME            service.name (default otel-external-demo)
"""

import json
import logging
import os
import sys

from opentelemetry import trace
from opentelemetry.sdk.resources import Resource
from opentelemetry.sdk.trace import TracerProvider
from opentelemetry.sdk.trace.export import SimpleSpanProcessor
from opentelemetry.exporter.otlp.proto.http.trace_exporter import OTLPSpanExporter

# The log signal (the PG-2 seed). The official logs SDK is still under the
# `_logs` module name in 1.27.0; that is the supported import path for the
# stable OTLP/HTTP log exporter.
from opentelemetry.sdk._logs import LoggerProvider, LoggingHandler
from opentelemetry.sdk._logs.export import SimpleLogRecordProcessor
from opentelemetry.exporter.otlp.proto.http._log_exporter import OTLPLogExporter


def main() -> int:
    base = os.environ.get("OTEL_EXPORTER_OTLP_ENDPOINT")
    if not base:
        print("OTEL_EXPORTER_OTLP_ENDPOINT must be set", file=sys.stderr)
        return 2
    base = base.rstrip("/")

    tenant = os.environ.get("KALEIDOSCOPE_TENANT", "acme")
    service_name = os.environ.get("OTEL_SERVICE_NAME", "otel-external-demo")

    # Resource attributes: tenant.id lets the gateway file the telemetry under
    # the right tenant; service.name names the producer.
    resource = Resource.create(
        {
            "service.name": service_name,
            "tenant.id": tenant,
        }
    )

    # --- Traces: explicit per-signal path, immediate export. ---------------
    span_exporter = OTLPSpanExporter(endpoint=f"{base}/v1/traces")
    tracer_provider = TracerProvider(resource=resource)
    tracer_provider.add_span_processor(SimpleSpanProcessor(span_exporter))
    tracer = tracer_provider.get_tracer("otel-external-demo")

    # --- Logs (the PG-2 seed): explicit per-signal path, immediate export. --
    log_exporter = OTLPLogExporter(endpoint=f"{base}/v1/logs")
    logger_provider = LoggerProvider(resource=resource)
    logger_provider.add_log_record_processor(SimpleLogRecordProcessor(log_exporter))
    handler = LoggingHandler(level=logging.INFO, logger_provider=logger_provider)
    app_logger = logging.getLogger("otel-external-demo")
    app_logger.setLevel(logging.INFO)
    app_logger.addHandler(handler)

    # One trace: a parent span and a child created in the parent's context, so
    # they share a trace id and the child's parent is the parent.
    parent = tracer.start_span("parent-operation")
    ctx = trace.set_span_in_context(parent)
    child = tracer.start_span("child-operation", context=ctx)
    # SPAN-level attribute on the CHILD (not a resource attribute).
    child.set_attribute("customer.id", "bea-test")

    # Emit one log INSIDE the child's active span context, so the SDK stamps
    # the trace/span ids onto the log record.
    with trace.use_span(child, end_on_exit=False):
        app_logger.info("pg1 external sdk log inside span")

    child.end()
    parent.end()

    # Read back the EXACT nanos and ids the SDK recorded (these are what it
    # exports, so they round-trip).
    parent_ctx = parent.get_span_context()
    child_ctx = child.get_span_context()
    handshake = {
        "trace_id": f"{parent_ctx.trace_id:032x}",
        "parent": {
            "span_id": f"{parent_ctx.span_id:016x}",
            "start_unix_nano": parent.start_time,
            "end_unix_nano": parent.end_time,
        },
        "child": {
            "span_id": f"{child_ctx.span_id:016x}",
            "start_unix_nano": child.start_time,
            "end_unix_nano": child.end_time,
        },
    }

    # Flush + shut down both providers so everything is delivered before exit.
    logger_provider.force_flush()
    logger_provider.shutdown()
    tracer_provider.force_flush()
    tracer_provider.shutdown()

    # Exactly one parseable JSON line on stdout; notes go to stderr.
    print(f"emitted trace {handshake['trace_id']} for tenant {tenant}", file=sys.stderr)
    print(json.dumps(handshake))
    return 0


if __name__ == "__main__":
    sys.exit(main())
