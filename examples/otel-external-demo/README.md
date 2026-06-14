# otel-external-demo

A tiny demo that emits OpenTelemetry using ONLY the official OpenTelemetry SDK.
It has no dependency on Kaleidoscope: it speaks plain OTLP/HTTP to whatever
collector you point it at, exactly like any third-party app would.

It emits one trace (a parent span and a child span) with a span-level
attribute `customer.id="bea-test"` on the child, and one log record from inside
the child's span context. It prints one JSON line on stdout with the ids and
the exact nanosecond timestamps the SDK recorded.

## Run

```sh
python3 -m venv .venv
.venv/bin/python -m pip install -r requirements.txt
OTEL_EXPORTER_OTLP_ENDPOINT=http://127.0.0.1:4318 .venv/bin/python app.py
```

## Environment

- `OTEL_EXPORTER_OTLP_ENDPOINT` (required) -- base URL of the OTLP/HTTP
  collector, e.g. `http://127.0.0.1:4318`. The per-signal paths `/v1/traces`
  and `/v1/logs` are appended explicitly.
- `KALEIDOSCOPE_TENANT` (default `acme`) -- filed as the resource attribute
  `tenant.id` so the collector can scope the telemetry to a tenant.
- `OTEL_SERVICE_NAME` (default `otel-external-demo`) -- the `service.name`.
