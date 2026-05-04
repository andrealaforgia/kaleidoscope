# Component Design — `aperture` v0 (DESIGN)

> **Wave**: DESIGN (`nw-solution-architect` / Morgan).
> **Date**: 2026-05-04.
> **Author**: Morgan.
> **Companion documents**: `architecture-overview.md`, `wave-decisions.md`, `aperture-port-and-adapter-diagram.md`, `workspace-layout.md`, ADR-0006 through ADR-0010 in [`../../../product/architecture/`](../../../product/architecture/).

This document is the binding contract DELIVER consumes. Every type signature, error variant, module path, and configuration key declared here is load-bearing. Renames are version-bump-able; additions are non-breaking.

---

## Module structure

```
crates/aperture/
├── Cargo.toml
├── README.md
├── examples/
│   ├── send_one_log_record_grpc.rs        # Slice 01 demo client
│   └── config-stub.toml                   # Slice 01 demo config
└── src/
    ├── lib.rs                             # Public API: testing module + re-exports for integration tests
    ├── main.rs                            # Binary entry: parses args, calls run().await
    ├── ports/
    │   └── mod.rs                         # OtlpSink trait + SinkRecord enum + SinkError enum + Probe trait
    ├── app/
    │   ├── mod.rs                         # Application core: ingest_logs/traces/metrics, framing_for_transport
    │   ├── readiness.rs                   # ReadinessState (Starting / Ready / Draining)
    │   ├── responses.rs                   # violation_to_response, sink_error_to_response (per transport)
    │   └── summary.rs                     # summarise_record (resource.service.name, counts)
    ├── transport/
    │   ├── mod.rs
    │   ├── grpc.rs                        # tonic Server + LogsService/TracesService/MetricsService impls
    │   └── http.rs                        # axum Router for /v1/{logs,traces,metrics} + /healthz + /readyz
    ├── sinks/
    │   ├── mod.rs
    │   ├── stub.rs                        # StubSink
    │   └── forwarding.rs                  # ForwardingSink + reqwest client + Probe impl
    ├── config/
    │   ├── mod.rs                         # Config struct + load_config(path) + validate_config
    │   └── schema.rs                      # ApertureConfig and nested structs (TOML schema)
    ├── observability/
    │   ├── mod.rs                         # tracing-subscriber init; closed event-name constants
    │   └── events.rs                      # event-name constants (closed v0 set, see DISCUSS D1)
    ├── shutdown/
    │   └── mod.rs                         # orchestrate_shutdown; signal handler; drain logic
    ├── error.rs                           # ApertureError (top-level; thiserror)
    └── compose.rs                         # Composition root: wire_then_probe_then_use; build_app
└── tests/
    ├── slice_01_walking_skeleton.rs       # Walking skeleton acceptance, real OTel SDK 0.27 client
    ├── slice_02_http_and_readiness.rs     # HTTP listener + /healthz + /readyz
    ├── slice_03_traces.rs                 # Traces signal end-to-end
    ├── slice_04_metrics.rs                # Metrics signal end-to-end
    ├── slice_05_backpressure.rs           # Concurrency cap UATs
    ├── slice_06_forwarding_sink.rs        # ForwardingSink against a fixture downstream
    ├── slice_07_tls_schema_knob.rs        # tls.enabled=true emits warn, plaintext continues
    ├── slice_08_graceful_shutdown.rs      # Drain UATs (clean drain + deadline-exceeded)
    ├── no_telemetry_on_telemetry.rs       # Network-namespace invariant test (DEVOPS-provided fixture)
    └── probe_gold_runner.rs               # Earned-Trust probe behavioural-layer test
```

The split is mechanical: file boundaries match the conceptual layers (`ports`, `app`, `transport`, `sinks`, `config`, `observability`, `shutdown`, `error`, `compose`). The same shape the harness used (modules-from-day-one); the same shape every later Kaleidoscope service will follow.

`lib.rs` exposes only a `testing` sub-module (with the `RecordingSink` test double — see "Test doubles" below) and re-exports the bare minimum the integration tests need. The crate's primary deliverable is the `aperture` binary in `main.rs`.

---

## Public types and traits — full signatures

### `ports::OtlpSink` and friends

```rust
// crates/aperture/src/ports/mod.rs
//! Output ports — the seams Aperture's application core writes through.
//! Concrete implementations are in `crate::sinks`.

use async_trait::async_trait;
use opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest;
use opentelemetry_proto::tonic::collector::metrics::v1::ExportMetricsServiceRequest;
use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest;

/// Aperture's hand-off boundary with the next pipeline stage. v0 ships
/// `StubSink` and `ForwardingSink`; Phase 1 adds Sieve as a third
/// implementation.
///
/// Implementations MUST be cheap to clone (typically an `Arc<Inner>`).
/// Aperture wraps every sink in `Arc<dyn OtlpSink>` at the composition
/// root and clones the Arc into each transport adapter.
#[async_trait]
pub trait OtlpSink: Send + Sync + 'static {
    /// Hand the typed record to the next stage. Returns when the next
    /// stage has acknowledged (Ok) or refused (Err). Aperture awaits this
    /// before responding to the upstream SDK; "the sink has acknowledged"
    /// is the contract Andrea's locked Q3 names.
    ///
    /// On `Ok(())`: SDK receives gRPC OK / HTTP 200.
    /// On `Err(e)`: SDK receives gRPC UNAVAILABLE / HTTP 503 with `e`'s
    /// Display included in the upstream message body.
    ///
    /// Implementations MUST NOT panic on user input; user-input failure
    /// is a `Err(SinkError)`, never a panic.
    async fn accept(&self, record: SinkRecord) -> Result<(), SinkError>;
}

/// The three OTLP-stable signals at v0. Carries the upstream
/// `opentelemetry_proto` type unwrapped — no harness-local wrapper, no
/// Aperture-local wrapper (DISCUSS D2).
#[derive(Debug)]
#[non_exhaustive]
pub enum SinkRecord {
    Logs(ExportLogsServiceRequest),
    Traces(ExportTraceServiceRequest),
    Metrics(ExportMetricsServiceRequest),
}

/// Reasons a sink can refuse a record. The variants are the failure
/// shapes downstream actually surface; future-additive under
/// `#[non_exhaustive]`.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SinkError {
    /// Downstream returned a non-2xx status, or refused the connection,
    /// or DNS resolution failed. Maps to gRPC UNAVAILABLE / HTTP 503.
    #[error("downstream unavailable: {reason}")]
    DownstreamUnavailable {
        /// Free-text reason, surfaced verbatim in the upstream message body.
        /// Examples: "503 Service Unavailable", "connection refused",
        /// "dns resolve failed: no record for otelbackend".
        reason: String,
    },

    /// Downstream did not respond within the configured timeout. Maps to
    /// gRPC UNAVAILABLE / HTTP 503.
    #[error("downstream timeout after {elapsed_ms} ms")]
    DownstreamTimeout {
        /// Elapsed wall-clock time in milliseconds before the deadline hit.
        elapsed_ms: u64,
    },

    /// The sink itself crashed in a way the application core couldn't
    /// classify. Maps to gRPC INTERNAL / HTTP 500. SHOULD NEVER occur in
    /// v0; defensive variant only.
    #[error("sink internal error: {message}")]
    Internal {
        message: String,
    },
}
```

The trait is **async** because `ForwardingSink` does network I/O; a synchronous trait would force every async I/O call to block a Tokio runtime thread, defeating DISCUSS Q2's locked Tokio choice (rejected alternative recorded in DISCUSS D2).

The trait uses the `async-trait` crate (not nightly's stabilised `async fn in trait`) because:
1. `async-trait` allows storing `Arc<dyn OtlpSink>` in the runtime registry — `async fn in trait` does not yet support `dyn` dispatch in stable Rust 1.85.
2. `async-trait`'s desugaring is well-understood, debuggable in stack traces, and adds minimal compile-time cost.
3. ADR-0007 records this trade-off and the Phase-1 revisit gate.

### `ports::Probe`

```rust
// crates/aperture/src/ports/mod.rs (continued)

/// Earned-Trust probe contract. Every `OtlpSink` implementation MUST
/// also implement `Probe`; the composition root invokes
/// `wire_then_probe_then_use` which refuses to start if any probe
/// returns `Err`.
///
/// The structural-layer enforcement (xtask AST walk) verifies every
/// type implementing `OtlpSink` also implements `Probe`. The behavioural
/// layer (`tests/probe_gold_runner.rs`) verifies probes actually exercise
/// their dependency.
#[async_trait]
pub trait Probe: Send + Sync + 'static {
    /// Demonstrate empirically that this component can honour its
    /// contract in the real environment where it will run. For
    /// `StubSink`: trivially `Ok(())` (no external dependency). For
    /// `ForwardingSink`: issues a probe request to the configured
    /// downstream endpoint and asserts a 2xx response.
    ///
    /// Returns `Err(ProbeError)` if the dependency does not honour the
    /// contract; the composition root translates this to a
    /// `health.startup.refused` event and exits 1.
    async fn probe(&self) -> Result<(), ProbeError>;
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ProbeError {
    #[error("downstream unreachable at {endpoint}: {reason}")]
    Unreachable { endpoint: String, reason: String },

    #[error("downstream rejected probe at {endpoint}: status {status}")]
    Refused { endpoint: String, status: u16 },

    #[error("probe timed out after {elapsed_ms} ms against {endpoint}")]
    Timeout { endpoint: String, elapsed_ms: u64 },
}
```

### `app::ingest_logs`, `ingest_traces`, `ingest_metrics`

```rust
// crates/aperture/src/app/mod.rs

use std::sync::Arc;
use crate::ports::{OtlpSink, SinkRecord, SinkError};
use otlp_conformance_harness::{validate_logs, validate_traces, validate_metrics, Framing, OtlpViolation};

/// Outcome of one validate-and-route call. The transport adapters
/// translate this to gRPC / HTTP responses via `app::responses`.
#[derive(Debug)]
pub enum IngestOutcome {
    Accepted,
    Rejected(OtlpViolation),
    SinkRefused(SinkError),
    /// Concurrency cap was hit before the call ran. Carries the cap value
    /// so the response can name it.
    ConcurrencyCapHit { cap: u32, transport: Transport },
}

#[derive(Debug, Clone, Copy)]
pub enum Transport {
    Grpc,
    HttpProtobuf,
}

#[inline]
pub fn framing_for_transport(t: Transport) -> Framing {
    match t {
        Transport::Grpc => Framing::GrpcProtobuf,
        Transport::HttpProtobuf => Framing::HttpProtobuf,
    }
}

/// Validate a logs body and route it to the sink. The single call site
/// for `validate_logs` in Aperture (CI invariant
/// `single_validator_per_signal` enforces).
pub async fn ingest_logs(
    body: &[u8],
    transport: Transport,
    sink: &Arc<dyn OtlpSink>,
) -> IngestOutcome {
    let framing = framing_for_transport(transport);
    match validate_logs(body, framing) {
        Ok(record) => match sink.accept(SinkRecord::Logs(record)).await {
            Ok(()) => IngestOutcome::Accepted,
            Err(e) => IngestOutcome::SinkRefused(e),
        },
        Err(violation) => IngestOutcome::Rejected(violation),
    }
}

// ingest_traces and ingest_metrics are mechanically symmetric; same shape.
```

**Critical invariant.** These three functions are the ONLY call sites of `validate_logs/traces/metrics` in the entire `aperture` crate. The CI invariant `single_validator_per_signal` (DEVOPS-owned, AST-walking xtask check) enforces this. Any future code adding a second call site fails CI.

### `app::responses` — transport-specific response mapping

```rust
// crates/aperture/src/app/responses.rs
//! Map IngestOutcome to transport-specific response shapes. The harness's
//! OtlpViolation::Display is used VERBATIM (DISCUSS D6); this module never
//! reformats, truncates, or replaces the string.

use otlp_conformance_harness::OtlpViolation;
use crate::ports::SinkError;
use crate::app::Transport;

pub struct GrpcResponse {
    pub status: tonic::Code,
    pub message: String,
}

pub struct HttpResponse {
    pub status: u16,
    pub body: String,
    pub content_type: &'static str,
    pub retry_after_seconds: Option<u32>,
}

pub fn violation_to_grpc(v: &OtlpViolation) -> GrpcResponse {
    GrpcResponse {
        status: tonic::Code::InvalidArgument,
        message: v.to_string(), // OtlpViolation::Display, verbatim
    }
}

pub fn violation_to_http(v: &OtlpViolation) -> HttpResponse {
    HttpResponse {
        status: 400,
        body: v.to_string(), // OtlpViolation::Display, verbatim
        content_type: "text/plain; charset=utf-8",
        retry_after_seconds: None,
    }
}

pub fn sink_error_to_grpc(e: &SinkError) -> GrpcResponse {
    GrpcResponse {
        status: match e {
            SinkError::Internal { .. } => tonic::Code::Internal,
            _ => tonic::Code::Unavailable,
        },
        message: e.to_string(),
    }
}

pub fn sink_error_to_http(e: &SinkError) -> HttpResponse {
    HttpResponse {
        status: match e {
            SinkError::Internal { .. } => 500,
            _ => 503,
        },
        body: e.to_string(),
        content_type: "text/plain; charset=utf-8",
        retry_after_seconds: None,
    }
}

pub fn cap_hit_to_grpc(cap: u32, transport: Transport) -> GrpcResponse {
    GrpcResponse {
        status: tonic::Code::ResourceExhausted,
        message: format!(
            "aperture: gRPC concurrency cap of {cap} reached on transport={}",
            match transport { Transport::Grpc => "grpc", Transport::HttpProtobuf => "http_protobuf" }
        ),
    }
}

pub fn cap_hit_to_http(cap: u32, transport: Transport) -> HttpResponse {
    HttpResponse {
        status: 503,
        body: format!(
            "aperture: HTTP concurrency cap of {cap} reached on transport={}",
            match transport { Transport::Grpc => "grpc", Transport::HttpProtobuf => "http_protobuf" }
        ),
        content_type: "text/plain; charset=utf-8",
        retry_after_seconds: Some(1),
    }
}
```

### `app::summary::summarise_record`

```rust
// crates/aperture/src/app/summary.rs

use crate::ports::SinkRecord;

pub struct RecordSummary<'a> {
    pub signal: &'static str,    // "logs" | "traces" | "metrics"
    pub resource_service_name: Option<&'a str>,
    pub count: usize,            // record_count | span_count | data_point_count
    pub count_field_name: &'static str, // "record_count" | "span_count" | "data_point_count"
}

pub fn summarise_record(record: &SinkRecord) -> RecordSummary<'_> {
    match record {
        SinkRecord::Logs(req) => RecordSummary {
            signal: "logs",
            resource_service_name: extract_service_name_from_logs(req),
            count: count_log_records(req),
            count_field_name: "record_count",
        },
        SinkRecord::Traces(req) => RecordSummary {
            signal: "traces",
            resource_service_name: extract_service_name_from_traces(req),
            count: count_spans(req),
            count_field_name: "span_count",
        },
        SinkRecord::Metrics(req) => RecordSummary {
            signal: "metrics",
            resource_service_name: extract_service_name_from_metrics(req),
            count: count_data_points(req),
            count_field_name: "data_point_count",
        },
    }
}

// Counting helpers walk:
//   logs:    ResourceLogs -> ScopeLogs -> LogRecord
//   traces:  ResourceSpans -> ScopeSpans -> Span
//   metrics: ResourceMetrics -> ScopeMetrics -> Metric (one per Metric, not per data point)
//
// DISCUSS US-AP-06 Domain Examples #2 explicitly says histograms count as
// ONE data point each, not per bucket — the unit downstream backends count.
// The DISCUSS contract is "one per Metric", which is the value DELIVER must
// implement.
//
// resource.service.name extraction walks ResourceFooBars[0].Resource.attributes
// looking for the key "service.name" (OpenTelemetry semantic convention).
// Returns None if the attribute is absent; sink_accepted line omits the field.
```

### `app::readiness::ReadinessState`

```rust
// crates/aperture/src/app/readiness.rs

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ReadinessPhase {
    Starting = 0,
    Ready = 1,
    Draining = 2,
}

pub struct ReadinessState {
    inner: AtomicU8,
    grpc_bound: std::sync::atomic::AtomicBool,
    http_bound: std::sync::atomic::AtomicBool,
}

pub type SharedReadinessState = Arc<ReadinessState>;

impl ReadinessState {
    pub fn new() -> SharedReadinessState {
        Arc::new(Self {
            inner: AtomicU8::new(ReadinessPhase::Starting as u8),
            grpc_bound: false.into(),
            http_bound: false.into(),
        })
    }

    pub fn current(&self) -> ReadinessPhase { /* read inner with Ordering::Acquire */ }

    pub fn mark_grpc_bound(&self) {
        self.grpc_bound.store(true, Ordering::Release);
        self.recompute_ready();
    }
    pub fn mark_http_bound(&self) {
        self.http_bound.store(true, Ordering::Release);
        self.recompute_ready();
    }

    pub fn flip_to_draining(&self) {
        self.inner.store(ReadinessPhase::Draining as u8, Ordering::Release);
    }

    fn recompute_ready(&self) {
        if self.current() == ReadinessPhase::Draining { return; } // never go back to Ready
        if self.grpc_bound.load(Ordering::Acquire) && self.http_bound.load(Ordering::Acquire) {
            self.inner.store(ReadinessPhase::Ready as u8, Ordering::Release);
            // emit event=readiness_changed ready=true reason=listeners_bound
        }
    }
}
```

The state machine has one direction only: `Starting → Ready → Draining`. There is no path from `Draining` back to `Ready` (a draining process never recovers; it exits).

`/readyz` reads `ReadinessState::current()` and returns 200 only when the phase is `Ready`.

### `error::ApertureError` (top-level)

```rust
// crates/aperture/src/error.rs

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ApertureError {
    #[error("config error: {message}")]
    ConfigInvalid { message: String },

    #[error("config file unreadable at {path}: {source}")]
    ConfigUnreadable { path: String, #[source] source: std::io::Error },

    #[error("listener bind failed for {transport} on {addr}: {source}")]
    ListenerBindFailed { transport: String, addr: String, #[source] source: std::io::Error },

    #[error("sink probe failed: {0}")]
    SinkProbeFailed(#[from] crate::ports::ProbeError),

    #[error("shutdown deadline exceeded; {dropped_count} requests still in-flight")]
    DrainDeadlineExceeded { dropped_count: usize },

    #[error("internal invariant violation: {message}")]
    Internal { message: String },
}

pub type Result<T> = std::result::Result<T, ApertureError>;
```

The `main()` entry maps `ApertureError` variants to specific exit codes:

| Variant | Exit code | Stderr `event=` |
|---|---|---|
| `ConfigInvalid` | 2 | `config_validation_failed` |
| `ConfigUnreadable` | 2 | `config_validation_failed` |
| `ListenerBindFailed` | 1 | `listener_bind_failed` |
| `SinkProbeFailed` | 1 | `health.startup.refused` |
| `DrainDeadlineExceeded` | 1 | `drain_deadline_exceeded` then `shutdown_complete exit_code=1` |
| `Internal` | 70 (EX_SOFTWARE) | `internal_invariant_violation` |
| Clean drain (`Ok(())`) | 0 | `shutdown_complete exit_code=0` |

---

## Configuration schema

TOML schema, loaded once at startup via `figment`. Environment-variable overrides use the prefix `APERTURE__` with `__` as the path separator (figment's standard convention).

### `config-default.toml` — the schema in concrete form

```toml
# aperture v0 configuration. Every key is optional; defaults shown.
# Top-level table is the reserved name "aperture".

[aperture]
# Ops-friendly version label echoed in the startup event. Normally left at default.
# (Defaults to env!("CARGO_PKG_VERSION") — operators do not set this.)

[aperture.transport.grpc]
# Address and port the gRPC listener binds to. Both halves are validated;
# 0.0.0.0 means all interfaces; any non-loopback bind on a privileged port
# (<1024) requires CAP_NET_BIND_SERVICE.
bind_addr = "0.0.0.0:4317"
# Maximum receive message size in bytes. Bodies above this return
# RESOURCE_EXHAUSTED (gRPC) or 413 (HTTP). 4 MiB is the OTel default.
max_recv_msg_size = 4194304        # 4 MiB
# Maximum simultaneous in-flight requests on this transport. Beyond this,
# refusal is RESOURCE_EXHAUSTED. See ADR-0010 for the rationale.
max_concurrent_requests = 1024

[aperture.transport.http]
bind_addr = "0.0.0.0:4318"
max_recv_msg_size = 4194304
max_concurrent_requests = 1024

[aperture.sink]
# "stub" -> StubSink (logs to stderr; useful for smoke fixtures and CI).
# "forwarding" -> ForwardingSink (POSTs to forwarding.endpoint).
kind = "stub"

[aperture.sink.forwarding]
# Operator-supplied OTel-compatible backend URL. Required when kind="forwarding".
# Schema: "http://host:port" (no trailing slash, no path; Aperture appends /v1/{signal}).
endpoint = ""
# Outbound request timeout; ForwardingSink fails fast on timeout.
timeout_ms = 5000

[aperture.shutdown]
# Maximum time the drain phase will wait for in-flight requests after the
# readiness flip. Default matches Kubernetes' terminationGracePeriodSeconds.
drain_deadline_ms = 30000

[aperture.security.tls]
# Forward-compatibility knob for Aegis (Phase 2). At v0 setting this true emits
# exactly one warn stderr line (event=tls_not_supported_in_v0) and continues plaintext.
enabled = false
# Path to the PEM-encoded server certificate. Read at startup if enabled.
cert_path = ""
# Path to the PEM-encoded server private key. Read at startup if enabled.
key_path = ""

[aperture.security.auth.spiffe]
# Forward-compatibility knob for Aegis (Phase 2). Same behaviour as TLS at v0:
# one warn stderr line, continue plaintext.
enabled = false
# SPIFFE Workload API socket. Honoured only at Phase 2.
workload_api_socket = ""
# Trust domain. Honoured only at Phase 2.
trust_domain = ""
```

### Validation (post-deserialise checks)

Beyond serde's structural decode, the following invariants are checked in `config::validate_config`:

1. `transport.grpc.bind_addr != transport.http.bind_addr` — UAT in US-AP-01.
2. If `sink.kind == "forwarding"`, `sink.forwarding.endpoint` MUST be non-empty AND parse as a `http://` or `https://` URL.
3. `transport.{grpc,http}.max_concurrent_requests >= 1`.
4. `transport.{grpc,http}.max_recv_msg_size >= 1024` (1 KiB minimum, sanity check).
5. `shutdown.drain_deadline_ms >= 100` (100 ms minimum, prevents zero-deadline race).

Failure of any check returns `ApertureError::ConfigInvalid` with a specific message; exit code 2; stderr `event=config_validation_failed reason="..."`.

### Environment-variable override convention

`figment` converts env vars with the prefix `APERTURE__` and `__` separator into paths:

| Env var | Overrides |
|---|---|
| `APERTURE__TRANSPORT__GRPC__BIND_ADDR` | `aperture.transport.grpc.bind_addr` |
| `APERTURE__SINK__KIND` | `aperture.sink.kind` |
| `APERTURE__SINK__FORWARDING__ENDPOINT` | `aperture.sink.forwarding.endpoint` |
| `APERTURE__SHUTDOWN__DRAIN_DEADLINE_MS` | `aperture.shutdown.drain_deadline_ms` |
| `APERTURE__SECURITY__TLS__ENABLED` | `aperture.security.tls.enabled` |

CLI args at v0: `aperture --config /path/to/aperture.toml`. No other CLI args.

### Why `figment`, not plain `serde + toml`

`figment` provides layered configuration (file → env → defaults) with one builder; plain `serde + toml` requires hand-written merging logic for the env-var overlay. The dependency cost is small (figment is ~200 LOC of code, MIT-licensed, mature). ADR-0008 records the trade-off and considers `config-rs` as a rejected alternative.

---

## Observability design

### Logger initialisation

```rust
// crates/aperture/src/observability/mod.rs

use tracing_subscriber::{fmt, EnvFilter};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

pub fn init_logging() {
    let filter = EnvFilter::try_from_env("APERTURE_LOG")
        .unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(filter)
        .with(
            fmt::layer()
                .json()
                .with_writer(std::io::stderr)
                .flatten_event(true)
                .with_current_span(false)
                .with_span_list(false)
                .with_target(false)  // event names are explicit, not module-prefixed
        )
        .init();
}
```

Output shape per event (one line per event, JSON):

```json
{"timestamp":"2026-05-04T09:12:01.022Z","level":"info","event":"listener_bound","transport":"grpc","addr":"0.0.0.0:4317"}
```

Workspace `[lints]` denies `clippy::print_stdout` and `clippy::print_stderr`; `tracing` is the only stderr-writing path.

### Closed v0 event-name set (DISCUSS D1, verbatim)

```rust
// crates/aperture/src/observability/events.rs

pub mod event {
    pub const STARTUP: &str = "startup";
    pub const LISTENER_BOUND: &str = "listener_bound";
    pub const LISTENER_CLOSING: &str = "listener_closing";
    pub const LISTENER_BIND_FAILED: &str = "listener_bind_failed";
    pub const READY: &str = "ready";
    pub const READINESS_CHANGED: &str = "readiness_changed";
    pub const REQUEST_RECEIVED: &str = "request_received";
    pub const SINK_ACCEPTED: &str = "sink_accepted";
    pub const SINK_FAILED: &str = "sink_failed";
    pub const SHUTDOWN_INITIATED: &str = "shutdown_initiated";
    pub const SHUTDOWN_COMPLETE: &str = "shutdown_complete";
    pub const IN_FLIGHT_DRAINED: &str = "in_flight_drained";
    pub const DRAIN_DEADLINE_EXCEEDED: &str = "drain_deadline_exceeded";
    pub const UNSUPPORTED_MEDIA_TYPE: &str = "unsupported_media_type";
    pub const BODY_TOO_LARGE: &str = "body_too_large";
    pub const CONCURRENCY_CAP_HIT: &str = "concurrency_cap_hit";
    pub const TLS_NOT_SUPPORTED_IN_V0: &str = "tls_not_supported_in_v0";
    pub const HEALTH_STARTUP_REFUSED: &str = "health.startup.refused";
    pub const CONFIG_VALIDATION_FAILED: &str = "config_validation_failed";
    pub const INTERNAL_INVARIANT_VIOLATION: &str = "internal_invariant_violation";
}
```

20 events total. The DISCUSS D1 closed set has 16; DESIGN adds four (`health.startup.refused` for the Earned-Trust probe; `config_validation_failed` for config-load errors; `internal_invariant_violation` for the defensive `ApertureError::Internal` exit; `request_received` was in DISCUSS D1 — kept). Adding events is non-breaking under DISCUSS D1's evolution rules; adding these four is justified by Earned-Trust probe contract (D1's set was written before the probe contract was crystallised in DESIGN).

### Health and readiness handlers

`/healthz` and `/readyz` are routes on the **same** axum HTTP listener as `/v1/{logs,traces,metrics}` — they share port `:4318`. This is deliberate (DISCUSS US-AP-02 Solution): operators expect to probe one place. If a future security review demands separation onto an admin port, that is a Phase-1+ change.

```rust
// crates/aperture/src/transport/http.rs (excerpts)

async fn healthz_handler() -> impl axum::response::IntoResponse {
    (axum::http::StatusCode::OK, [("Content-Type", "text/plain; charset=utf-8")], "ok\n")
}

async fn readyz_handler(
    State(readiness): State<SharedReadinessState>,
) -> impl axum::response::IntoResponse {
    use ReadinessPhase::*;
    let (status, body) = match readiness.current() {
        Starting => (axum::http::StatusCode::SERVICE_UNAVAILABLE, "starting\n"),
        Ready    => (axum::http::StatusCode::OK,                  "ready\n"),
        Draining => (axum::http::StatusCode::SERVICE_UNAVAILABLE, "draining\n"),
    };
    (status, [("Content-Type", "text/plain; charset=utf-8")], body)
}
```

Health and readiness handlers MUST NOT panic. If they do, that is an internal invariant violation (DISCUSS US-AP-05 failure_modes); the panic-handler set in `main()` exits the process non-zero.

---

## Sinks — concrete designs

### `StubSink`

```rust
// crates/aperture/src/sinks/stub.rs

use async_trait::async_trait;
use crate::ports::{OtlpSink, Probe, SinkRecord, SinkError, ProbeError};
use crate::app::summary::summarise_record;

#[derive(Debug, Default)]
pub struct StubSink;

#[async_trait]
impl OtlpSink for StubSink {
    async fn accept(&self, record: SinkRecord) -> Result<(), SinkError> {
        let s = summarise_record(&record);
        tracing::info!(
            event = crate::observability::events::event::SINK_ACCEPTED,
            sink = "stub",
            signal = s.signal,
            { s.count_field_name } = s.count,
            "resource.service.name" = s.resource_service_name.unwrap_or(""),
        );
        Ok(())
    }
}

#[async_trait]
impl Probe for StubSink {
    async fn probe(&self) -> Result<(), ProbeError> {
        // No external dependency. Probe is trivially Ok. Documented here so
        // a maintainer doesn't think "no probe" is the answer.
        Ok(())
    }
}
```

### `ForwardingSink`

```rust
// crates/aperture/src/sinks/forwarding.rs

use async_trait::async_trait;
use std::sync::Arc;
use std::time::{Duration, Instant};
use reqwest::Client;
use prost::Message;
use crate::ports::{OtlpSink, Probe, SinkRecord, SinkError, ProbeError};

#[derive(Debug)]
pub struct ForwardingSink {
    endpoint: String,                // e.g. "http://otel-backend:4318"
    timeout: Duration,
    client: Client,
}

impl ForwardingSink {
    pub fn new(endpoint: String, timeout: Duration) -> Self {
        let client = Client::builder()
            .timeout(timeout)
            .user_agent(format!("aperture/{}", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("reqwest::Client::build is infallible with these options");
        Self { endpoint, timeout, client }
    }

    fn url_for(&self, signal: &'static str) -> String {
        format!("{}/v1/{signal}", self.endpoint.trim_end_matches('/'))
    }
}

#[async_trait]
impl OtlpSink for ForwardingSink {
    async fn accept(&self, record: SinkRecord) -> Result<(), SinkError> {
        let started = Instant::now();
        let (signal, body) = encode_for_forwarding(&record); // signal: "logs"|"traces"|"metrics"
        let url = self.url_for(signal);

        let result = self.client
            .post(&url)
            .header("Content-Type", "application/x-protobuf")
            .body(body)
            .send()
            .await;

        let elapsed_ms = started.elapsed().as_millis() as u64;

        match result {
            Ok(resp) if resp.status().is_success() => {
                tracing::info!(
                    event = crate::observability::events::event::SINK_ACCEPTED,
                    sink = "forwarding",
                    downstream = %self.endpoint,
                    signal,
                    downstream_latency_ms = elapsed_ms,
                );
                Ok(())
            }
            Ok(resp) => {
                let status = resp.status().as_u16();
                tracing::error!(
                    event = crate::observability::events::event::SINK_FAILED,
                    sink = "forwarding",
                    downstream = %self.endpoint,
                    reason = %format!("{status}"),
                );
                Err(SinkError::DownstreamUnavailable {
                    reason: format!("downstream returned {status}"),
                })
            }
            Err(e) if e.is_timeout() => {
                tracing::error!(
                    event = crate::observability::events::event::SINK_FAILED,
                    sink = "forwarding",
                    downstream = %self.endpoint,
                    reason = "timeout",
                );
                Err(SinkError::DownstreamTimeout { elapsed_ms })
            }
            Err(e) => {
                let reason = if e.is_connect() { "connection refused" }
                    else if e.is_request() && e.to_string().contains("dns") { "dns_resolve_failed" }
                    else { "request error" };
                tracing::error!(
                    event = crate::observability::events::event::SINK_FAILED,
                    sink = "forwarding",
                    downstream = %self.endpoint,
                    reason = %reason,
                );
                Err(SinkError::DownstreamUnavailable {
                    reason: format!("{reason}: {e}"),
                })
            }
        }
    }
}

#[async_trait]
impl Probe for ForwardingSink {
    async fn probe(&self) -> Result<(), ProbeError> {
        // Two-stage probe: OPTIONS first, fall back to a known-empty
        // ExportLogsServiceRequest POST if OPTIONS is not supported.

        let opts = self.client
            .request(reqwest::Method::OPTIONS, self.url_for("logs"))
            .timeout(Duration::from_secs(2))
            .send().await;

        match opts {
            Ok(r) if r.status().is_success() || r.status() == 204 => return Ok(()),
            Ok(r) if matches!(r.status().as_u16(), 404 | 405) => {
                // Downstream may be OTel-compatible without OPTIONS support.
                // Fall through to the degraded probe below.
            }
            Ok(r) => return Err(ProbeError::Refused {
                endpoint: self.endpoint.clone(),
                status: r.status().as_u16(),
            }),
            Err(e) if e.is_timeout() => return Err(ProbeError::Timeout {
                endpoint: self.endpoint.clone(),
                elapsed_ms: 2000,
            }),
            Err(e) => return Err(ProbeError::Unreachable {
                endpoint: self.endpoint.clone(),
                reason: e.to_string(),
            }),
        }

        // Degraded probe: zero-records POST. Catalogued substrate lie:
        // a downstream that returns 200 to OPTIONS but 503 to POST.
        let degraded_body = empty_export_logs_service_request_bytes();
        let r = self.client
            .post(self.url_for("logs"))
            .header("Content-Type", "application/x-protobuf")
            .body(degraded_body)
            .timeout(Duration::from_secs(2))
            .send()
            .await
            .map_err(|e| ProbeError::Unreachable {
                endpoint: self.endpoint.clone(),
                reason: e.to_string(),
            })?;

        if r.status().is_success() {
            Ok(())
        } else {
            Err(ProbeError::Refused {
                endpoint: self.endpoint.clone(),
                status: r.status().as_u16(),
            })
        }
    }
}
```

The `Probe` impl is **not** optional. It is the structural-layer enforcement target: an `xtask` AST-walking check verifies every type implementing `OtlpSink` ALSO implements `Probe`. The behavioural-layer enforcement is `tests/probe_gold_runner.rs`, which starts Aperture against a fixture downstream that returns 200 to OPTIONS but 503 to POST and asserts Aperture refuses to start with `event=health.startup.refused`. This is the catalogued substrate lie for Aperture v0; future external dependencies grow this catalogue.

### Test doubles (in `lib.rs::testing`)

```rust
// crates/aperture/src/lib.rs

pub mod testing {
    //! Test doubles for integration tests. NOT part of the binary's public
    //! surface; gated under cfg(any(test, feature = "testing")) at the
    //! consumer end.

    use std::sync::Mutex;
    use async_trait::async_trait;
    use crate::ports::{OtlpSink, Probe, SinkRecord, SinkError, ProbeError};

    /// In-memory sink: every accepted record is appended to a vector.
    /// Used by US-AP-03's "custom OtlpSink plugs in" UAT.
    pub struct RecordingSink {
        pub records: Mutex<Vec<SinkRecord>>,
    }

    impl RecordingSink {
        pub fn new() -> Self { Self { records: Mutex::new(Vec::new()) } }
    }

    #[async_trait]
    impl OtlpSink for RecordingSink {
        async fn accept(&self, record: SinkRecord) -> Result<(), SinkError> {
            self.records.lock().expect("poisoned").push(record);
            Ok(())
        }
    }

    #[async_trait]
    impl Probe for RecordingSink {
        async fn probe(&self) -> Result<(), ProbeError> { Ok(()) }
    }
}
```

`RecordingSink` is the seam US-AP-03's "custom OtlpSink plugs in without crate-level changes" UAT writes against. It is the smallest possible witness that the trait IS the integration surface.

---

## Composition root (`compose.rs`)

```rust
// crates/aperture/src/compose.rs

pub async fn run(config: ApertureConfig) -> crate::Result<()> {
    crate::observability::init_logging();
    tracing::info!(event = event::STARTUP, version = env!("CARGO_PKG_VERSION"));

    crate::config::warn_on_v0_security_knobs(&config);

    let readiness = ReadinessState::new();

    // Wire the sink. Wire then probe then use.
    let sink: Arc<dyn OtlpSink> = match &config.sink.kind {
        SinkKind::Stub => Arc::new(StubSink),
        SinkKind::Forwarding => Arc::new(ForwardingSink::new(
            config.sink.forwarding.endpoint.clone(),
            Duration::from_millis(config.sink.forwarding.timeout_ms),
        )),
    };

    // Probe — refuses to start if the dependency isn't honourable.
    if let Err(e) = sink.probe().await {
        tracing::error!(event = event::HEALTH_STARTUP_REFUSED, reason = %e);
        return Err(ApertureError::SinkProbeFailed(e));
    }

    // Bind listeners. Each bind() flips the relevant readiness flag on success.
    let grpc_handle = transport::grpc::spawn(config.transport.grpc.clone(), Arc::clone(&sink), Arc::clone(&readiness)).await?;
    let http_handle = transport::http::spawn(config.transport.http.clone(), Arc::clone(&sink), Arc::clone(&readiness)).await?;
    tracing::info!(event = event::READY);

    // Wait for shutdown signal.
    shutdown::orchestrate(
        config.shutdown.drain_deadline_ms,
        Arc::clone(&readiness),
        vec![grpc_handle, http_handle],
    ).await
}
```

The sequence is **wire → probe → use**. A probe failure exits the process before any listener binds; the operator sees `event=health.startup.refused` on stderr and the runbook entry directs them to the misconfiguration.

---

## Dependency manifest

| Crate | Version | Purpose | License | Notes |
|---|---|---|---|---|
| `tokio` | `^1.40` | Async runtime | MIT | features = ["full"] in main; "rt-multi-thread", "macros", "signal", "sync" elsewhere |
| `tonic` | `^0.12` | gRPC server | MIT | features = ["transport"]; transitive `prost` aligned with `opentelemetry-proto`'s |
| `axum` | `^0.7` | HTTP server | MIT | features default; `axum::Router` for /v1/* + /healthz + /readyz |
| `hyper` | `^1.4` | HTTP foundation | MIT | transitive via axum and tonic |
| `tower` | `^0.5` | HTTP middleware | MIT | transitive via axum |
| `tracing` | `^0.1` | Logging facade | MIT | macros only |
| `tracing-subscriber` | `^0.3` | JSON stderr layer | MIT | features = ["json", "env-filter", "fmt"] |
| `serde` | `^1` | Config deserialise | MIT | workspace dep |
| `figment` | `^0.10` | Layered config | MIT | features = ["toml", "env"]; rejects undeclared keys |
| `async-trait` | `^0.1` | async fn in trait dyn-dispatch | MIT | ADR-0007 trade-off; revisit at Phase 1 |
| `thiserror` | `^1` | Error derive | MIT/Apache-2.0 | for ApertureError + SinkError + ProbeError |
| `reqwest` | `^0.12` | Outbound HTTP for ForwardingSink | MIT/Apache-2.0 | features = ["rustls-tls"]; default-features = false |
| `prost` | `^0.13` | Protobuf encoding for ForwardingSink | Apache-2.0 | workspace dep; aligned with opentelemetry-proto |
| `opentelemetry-proto` | `=0.27.0` | OTLP types | Apache-2.0 | workspace dep, exact pin per ADR-0003 |
| `otlp-conformance-harness` | `path = "../otlp-conformance-harness"` | Validation gate | CC0-1.0 | sibling crate |
| `[dev-dependencies] opentelemetry-otlp` | `^0.27` | Real OTel SDK for integration tests | Apache-2.0 | features for gRPC + HTTP exporters |
| `[dev-dependencies] tokio-test` | `^0.4` | Test utilities | MIT | |
| `[dev-dependencies] http` | `^1` | Status code constants | MIT | |
| `[dev-dependencies] wiremock` | `^0.6` | HTTP fixture for ForwardingSink tests | MIT | replaces a hand-rolled Hyper fixture |

All open-source, all MIT or Apache-2.0 (or CC0 for the in-tree harness). No proprietary dependencies. License compliance is enforced by `cargo deny check` at workspace level (the harness's ADR-0005 already mandates the gate; Aperture inherits it).

`[lints]` workspace section:

```toml
[workspace.lints.rust]
unsafe_code = "forbid"

[workspace.lints.clippy]
print_stdout = "deny"
print_stderr = "deny"
unwrap_used = "warn"
expect_used = "warn"
```

---

## Test surface (DELIVER will write the bodies)

Slice → integration test mapping:

| Slice | Integration test | Real client used | Asserts |
|---|---|---|---|
| 01 | `tests/slice_01_walking_skeleton.rs` | `opentelemetry-otlp` gRPC | gRPC OK; stderr listener_bound + request_received + sink_accepted lines |
| 02 | `tests/slice_02_http_and_readiness.rs` | `reqwest` + `opentelemetry-otlp` HTTP | /healthz=200; /readyz state machine; HTTP 200 on valid POST; 415 on JSON; 404 on unknown path |
| 03 | `tests/slice_03_traces.rs` | `opentelemetry-otlp` traces | gRPC OK + HTTP 200; signal=traces; span_count |
| 04 | `tests/slice_04_metrics.rs` | `opentelemetry-otlp` metrics | gRPC OK + HTTP 200; signal=metrics; data_point_count |
| 05 | `tests/slice_05_backpressure.rs` | `tonic` raw client + `reqwest` | RESOURCE_EXHAUSTED at cap+1; 503 + Retry-After; caps independent per transport |
| 06 | `tests/slice_06_forwarding_sink.rs` | `wiremock` downstream | sink_accepted with downstream + downstream_latency_ms; UNAVAILABLE on 503; UNAVAILABLE on connection refused |
| 07 | `tests/slice_07_tls_schema_knob.rs` | n/a | one warn line event=tls_not_supported_in_v0; listeners still bind plaintext |
| 08 | `tests/slice_08_graceful_shutdown.rs` | `tonic` raw client | /readyz flips to 503 within 100 ms; in_flight_drained on clean drain; drain_deadline_exceeded on slow sink |
| invariant | `tests/no_telemetry_on_telemetry.rs` | network namespace | zero outbound packets except listener acks and forwarding-to-downstream |
| invariant | `tests/probe_gold_runner.rs` | fixture downstream that lies | startup refused with health.startup.refused when downstream returns 200 to OPTIONS but 503 to POST |

---

## Behaviour mapping — DISCUSS event → DESIGN call site

| DISCUSS event | DESIGN call site | Tracing macro path |
|---|---|---|
| `startup` | `compose::run` (first line after init_logging) | `tracing::info!(event=event::STARTUP, ...)` |
| `listener_bound` | `transport::grpc::spawn` after `bind()`; `transport::http::spawn` after `bind()` | `tracing::info!(event=event::LISTENER_BOUND, transport, addr)` |
| `listener_bind_failed` | error branch of the same `bind()` calls | `tracing::error!(event=event::LISTENER_BIND_FAILED, transport, addr, reason)` |
| `ready` | `compose::run` after both listeners bound | `tracing::info!(event=event::READY, listeners=...)` |
| `readiness_changed` | `ReadinessState::recompute_ready`; `flip_to_draining` | `tracing::info!(event=event::READINESS_CHANGED, ready, reason)` |
| `request_received` | `transport::grpc::*Service::export_*`; `transport::http::ingest_handler` (before validation) | `tracing::info!(event=event::REQUEST_RECEIVED, transport, signal, bytes, peer)` |
| `sink_accepted` | `StubSink::accept`; `ForwardingSink::accept` (success path) | inside the sink impls |
| `sink_failed` | `ForwardingSink::accept` (error paths) | inside the sink impl |
| `shutdown_initiated` | `shutdown::orchestrate` (first line after signal received) | `tracing::info!(event=event::SHUTDOWN_INITIATED, signal, drain_deadline_ms)` |
| `listener_closing` | `shutdown::orchestrate` per-listener close | per listener |
| `in_flight_drained` | `shutdown::orchestrate` on clean drain | `tracing::info!(event=event::IN_FLIGHT_DRAINED, drained_count)` |
| `drain_deadline_exceeded` | `shutdown::orchestrate` on deadline elapse | `tracing::warn!(event=event::DRAIN_DEADLINE_EXCEEDED, dropped_count)` |
| `shutdown_complete` | `compose::run` final line, before exit | `tracing::info!(event=event::SHUTDOWN_COMPLETE, exit_code)` |
| `unsupported_media_type` | `transport::http::ingest_handler` Content-Type check | `tracing::warn!(event=event::UNSUPPORTED_MEDIA_TYPE, ...)` |
| `body_too_large` | tonic's `max_decoding_message_size` callback; axum's `Content-Length` check | `tracing::warn!(event=event::BODY_TOO_LARGE, ...)` |
| `concurrency_cap_hit` | `transport::grpc::*` after failed `try_acquire`; `transport::http::*` same | `tracing::warn!(event=event::CONCURRENCY_CAP_HIT, transport, cap)` |
| `tls_not_supported_in_v0` | `config::warn_on_v0_security_knobs` when `tls.enabled=true` | `tracing::warn!(event=event::TLS_NOT_SUPPORTED_IN_V0, ...)` |
| `health.startup.refused` | `compose::run` after a probe failure | `tracing::error!(event=event::HEALTH_STARTUP_REFUSED, reason)` |
| `config_validation_failed` | `config::validate_config` error mapping in `main()` | `tracing::error!(event=event::CONFIG_VALIDATION_FAILED, reason)` |
| `internal_invariant_violation` | panic handler / `ApertureError::Internal` | `tracing::error!(event=event::INTERNAL_INVARIANT_VIOLATION, message)` |

This mapping IS the contract DELIVER must implement. DISTILL writes the acceptance tests against the events listed in DISCUSS US-AP-* (already documented per-story); DELIVER drives the call sites green.

---

## What the binary actually does at startup (sequenced)

```
1. main() parses --config <path>
2. config::load_config(path)         -> ApertureConfig | ApertureError::ConfigInvalid (exit 2)
3. observability::init_logging()
4. tracing::info!(event=startup)
5. config::warn_on_v0_security_knobs() -> emits tls_not_supported_in_v0 warn line if enabled
6. compose::wire_sink(config.sink)   -> Arc<dyn OtlpSink>
7. sink.probe().await                 -> on Err: event=health.startup.refused (exit 1)
8. transport::grpc::spawn(...)       -> on Err: event=listener_bind_failed (exit 1)
9. transport::http::spawn(...)       -> on Err: event=listener_bind_failed (exit 1)
10. readiness flips to Ready (recompute_ready when both bound)
11. tracing::info!(event=ready)
12. shutdown::orchestrate(...)       -> awaits SIGTERM/SIGINT
13. on signal: event=shutdown_initiated; readiness flips to Draining; listeners close
14. wait for in-flight permits to release, bounded by drain_deadline_ms
15. on clean drain: event=in_flight_drained drained_count=N; event=shutdown_complete exit_code=0; exit 0
    on deadline:  event=drain_deadline_exceeded dropped_count=N; event=shutdown_complete exit_code=1; exit 1
```

Steps 7-9 are the load-bearing failure path: any failure here is loud (named stderr event), structured, exit-coded, non-silent. The composition root invariant is "wire → probe → use"; a probe failure prevents listener binding, so SDK clients never connect to a misconfigured Aperture and then fail at first request.

---

## Open issues for DELIVER (NOT design questions — just notes)

1. The `tonic` server's `max_concurrent_streams` setting is HTTP/2-level and orthogonal to the application-level semaphore. DELIVER should set it generously (e.g. 4× `max_concurrent_requests`) so the application semaphore is the binding constraint, not HTTP/2 frame-level limits.
2. `axum`'s default body-collection layer needs `RequestBodyLimitLayer` set to `max_recv_msg_size`; otherwise the `body_too_large` event is never emitted because hyper has already closed the connection. DELIVER references `tower_http::limit::RequestBodyLimitLayer`.
3. `figment`'s default behaviour silently ignores unknown TOML keys. DELIVER must add `Figment::deny_unknown_fields()` or use `serde(deny_unknown_fields)` on `ApertureConfig` so a misspelled key causes loud `config_validation_failed`, not silent default-value-use.
4. The `tracing-subscriber` JSON layer has a known issue where panic-during-format produces partial lines. DELIVER configures `fmt::Layer::event_format(...)` with a custom formatter that buffers a complete line and writes-then-flushes. Test asserts no partial lines on a forced panic.

These are implementation details, not architectural decisions; flagged here so DELIVER does not rediscover them.
