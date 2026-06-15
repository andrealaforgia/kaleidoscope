// Kaleidoscope telemetry generator — library seam (DISTILL scaffold)
// Copyright (C) 2026 The Kaleidoscope authors
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as
// published by the Free Software Foundation, either version 3 of the
// License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU
// Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public
// License along with this program. If not, see <https://www.gnu.org/licenses/>.

//! # `kaleidoscope-telemetrygen` — the "send" half of the run story.
//!
//! A first-party OTLP client built on `spark` that pushes ONE sample metric
//! (`request_count`), ONE sample log (`checkout failed: card declined`), and
//! ONE sample span (`GET /api/v1/query_range` under trace id
//! `4bf92f3577b34da6a3ce929d0e0e4736`) — the C1 sample vocabulary reused
//! verbatim — to a RUNNING consolidated runtime's OTLP/gRPC ingest, for the
//! local tenant. After it runs, all three signals are queryable: the
//! send-to-see loop closes end to end through the real OTLP wire and the live
//! shared store (ADR-0077 F3, experimentable-stack-v0 C3).
//!
//! ## Earned Trust — the generator must probe, not assume (US-04)
//!
//! `spark::init` validates only that the endpoint URL parses; it does NOT
//! probe connectivity, and the OTLP batch exporter is fire-and-forget. Against
//! a DOWN stack a naive generator would export into the void and exit 0,
//! contradicting US-04 ("fail clearly, do not hang or exit silently").
//! [`probe_reachable`] is therefore a MANDATORY pre-flight step: it TCP-probes
//! the ingest endpoint before any push and returns a clear [`GenError`] naming
//! the unreachable endpoint, so a down stack is a legible non-zero failure, not
//! a silent success.
//!
//! ## Pinning the verbatim sample trace id
//!
//! `spark`'s public surface is locked to four items (ADR-0011) and exposes no
//! custom id-generator seam, so the verbatim trace id
//! `4bf92f3577b34da6a3ce929d0e0e4736` is pinned through the standard OTel API:
//! the sample span is started with a SAMPLED **remote parent**
//! `SpanContext` carrying that trace id, and the child span inherits the
//! parent's trace id (the default `ParentBased(AlwaysOn)` sampler records a
//! sampled remote parent). The by-id query in scenario 1 then finds the span.

#![forbid(unsafe_code)]

use std::time::Duration;

/// The default OTLP/gRPC ingest endpoint (the consolidated runtime's gRPC
/// ingest). Overridden by `OTEL_EXPORTER_OTLP_ENDPOINT` in the bin.
pub const DEFAULT_ENDPOINT: &str = "http://localhost:4317";
/// The single local-experiment tenant (W3, ADR-0077). Overridden by
/// `KALEIDOSCOPE_TENANT` in the bin.
pub const DEFAULT_TENANT: &str = "acme";
/// The `service.name` the sample telemetry is filed under. The traces query
/// `service` parameter must match this to find the sample span.
pub const DEFAULT_SERVICE_NAME: &str = "kaleidoscope-demo";

/// The OTel instrumentation-scope name the sample meter and tracer are
/// obtained under. Distinct from `service.name` (which is the resource
/// attribute the queries key on); the scope name is the emitting library's
/// own identity and does not affect the query results.
const DEMO_INSTRUMENTATION_SCOPE: &str = "kaleidoscope-telemetrygen";

/// The sample metric name (the C1 vocabulary, reused verbatim). The metrics
/// `query_range` looks this up.
const METRIC_NAME: &str = "request_count";

/// The sample log body (the C1 vocabulary, reused verbatim). The logs query
/// returns records whose body contains this.
const LOG_BODY: &str = "checkout failed: card declined";

/// The sample span name (the C1 vocabulary, reused verbatim).
const SPAN_NAME: &str = "GET /api/v1/query_range";

/// The verbatim sample trace id (ADR-0077 F3 "reused verbatim"). The by-id
/// traces query looks this up; the sample span is pinned to it via a remote
/// parent span context.
const DEMO_TRACE_ID_HEX: &str = "4bf92f3577b34da6a3ce929d0e0e4736";

/// A fixed, non-zero parent span id for the pinned sample trace. The id must
/// be non-zero so the parent `SpanContext` is valid and the child span
/// actually inherits the pinned trace id (a zero span id yields an invalid
/// parent the sampler would ignore). The value is the W3C trace-context
/// example span id; its only role is to make the parent context valid.
const DEMO_PARENT_SPAN_ID_HEX: &str = "00f067aa0ba902b7";

/// The bound on the pre-flight TCP connect. A closed loopback port refuses
/// immediately; the timeout only guards a host that accepts the SYN but never
/// completes the handshake, so the generator fails clearly instead of hanging
/// (US-04 "do not hang or exit silently").
const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

/// What the generator pushes, and where. DELIVER feeds `endpoint` to both the
/// reachability probe and `spark::init`, and `tenant` to
/// `SparkConfig::with_tenant_id` so every sample record carries the
/// `tenant.id` resource attribute.
#[derive(Debug, Clone)]
pub struct GenConfig {
    /// The OTLP/gRPC ingest endpoint, e.g. `http://127.0.0.1:4317`.
    pub endpoint: String,
    /// The `tenant.id` resource attribute every sample record carries.
    pub tenant: String,
    /// The `service.name` resource attribute (default
    /// [`DEFAULT_SERVICE_NAME`]).
    pub service_name: String,
}

impl GenConfig {
    /// A config for `endpoint` + `tenant` with the default service name.
    #[must_use]
    pub fn new(endpoint: impl Into<String>, tenant: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            tenant: tenant.into(),
            service_name: DEFAULT_SERVICE_NAME.to_string(),
        }
    }
}

/// A count of what was pushed, returned on a successful [`generate`]. The bin
/// prints it as the run summary; the acceptance suite asserts the observable
/// outcome via the query routers, not this struct.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenSummary {
    /// Number of metric data points pushed (the sample sends 1).
    pub metrics_pushed: u64,
    /// Number of log records pushed (the sample sends 1).
    pub logs_pushed: u64,
    /// Number of spans pushed (the sample sends 1).
    pub spans_pushed: u64,
}

/// A telemetry-generation failure. The load-bearing variant is
/// [`GenError::Unreachable`]: the pre-flight probe found the ingest endpoint
/// down, so the bin exits non-zero with a clear, actionable message rather than
/// firing telemetry into the void (US-04).
#[derive(Debug, thiserror::Error)]
pub enum GenError {
    /// The pre-flight reachability probe could not reach the ingest endpoint.
    /// The bin renders this to stderr and exits non-zero (US-04 down-stack AC).
    #[error("ingest endpoint {endpoint} is unreachable ({detail}); bring the stack up first (for example `make up`) before generating telemetry")]
    Unreachable {
        /// The endpoint that could not be reached.
        endpoint: String,
        /// The underlying transport detail (e.g. connection refused).
        detail: String,
    },
    /// The OTLP export itself failed after a successful probe.
    #[error("telemetry export to {endpoint} failed: {detail}")]
    ExportFailed {
        /// The endpoint the export targeted.
        endpoint: String,
        /// The underlying export/flush detail.
        detail: String,
    },
    /// The generator configuration was rejected (e.g. an unparseable endpoint
    /// or an empty tenant).
    #[error("invalid generator configuration: {detail}")]
    InvalidConfig {
        /// What was wrong with the configuration.
        detail: String,
    },
    /// DISTILL RED-not-BROKEN placeholder. DELIVER replaces every return of
    /// this variant with the real probe / push implementation. Its `Display`
    /// deliberately differs from every real variant so an acceptance assertion
    /// can tell "scaffold not implemented" apart from a genuine business
    /// outcome (the RED-for-the-right-reason discriminator).
    #[error("__SCAFFOLD__ kaleidoscope-telemetrygen::{operation} is not yet implemented (DISTILL RED placeholder; DELIVER wires spark + opentelemetry)")]
    Scaffold {
        /// The seam that is not yet implemented.
        operation: &'static str,
    },
}

/// MANDATORY pre-flight reachability probe (ADR-0077 F3 / US-04).
///
/// Parses the host:port authority out of the OTLP `endpoint` and attempts a
/// bounded TCP connect. Returns `Ok(())` when the ingest endpoint accepts a
/// connection; returns [`GenError::Unreachable`] naming the endpoint when the
/// connect is refused or times out, so a down stack fails clearly instead of
/// the fire-and-forget batch exporter swallowing the push.
///
/// The authority is everything after the URL scheme (the OTLP/gRPC endpoint
/// carries a host:port and no path), passed straight to the resolver — a value
/// without a usable port simply fails the connect, which surfaces as the same
/// clear unreachability report.
///
/// # Errors
///
/// [`GenError::Unreachable`] when the ingest endpoint refuses the connection or
/// does not complete the handshake within [`PROBE_TIMEOUT`].
pub async fn probe_reachable(endpoint: &str) -> Result<(), GenError> {
    let authority = ingest_authority(endpoint);
    let connect = tokio::net::TcpStream::connect(authority);
    match tokio::time::timeout(PROBE_TIMEOUT, connect).await {
        Ok(Ok(_stream)) => Ok(()),
        Ok(Err(error)) => Err(GenError::Unreachable {
            endpoint: endpoint.to_owned(),
            detail: error.to_string(),
        }),
        Err(_elapsed) => Err(GenError::Unreachable {
            endpoint: endpoint.to_owned(),
            detail: format!("no response within {PROBE_TIMEOUT:?}"),
        }),
    }
}

/// The host:port authority of an OTLP endpoint: everything after the `://`
/// scheme separator (the OTLP/gRPC endpoint carries no path). When no scheme
/// is present the whole string is treated as the authority.
fn ingest_authority(endpoint: &str) -> &str {
    match endpoint.split_once("://") {
        Some((_scheme, authority)) => authority,
        None => endpoint,
    }
}

/// Push the sample telemetry across all three signals to `config.endpoint` for
/// `config.tenant` (ADR-0077 F3 / US-04).
///
/// DELIVER: after a successful [`probe_reachable`], `spark::init(
/// SparkConfig::for_service(&config.service_name).with_tenant_id(&config.tenant))`
/// pointed at `config.endpoint`, then emit via the global `opentelemetry` API
/// a `request_count` counter, a `checkout failed: card declined` log, and a
/// `GET /api/v1/query_range` span under trace id
/// `4bf92f3577b34da6a3ce929d0e0e4736`; drop the guard to force-flush all three
/// signals synchronously before returning the [`GenSummary`].
///
/// # Errors
///
/// - [`GenError::Unreachable`] when the pre-flight probe finds the ingest
///   endpoint down (fail-closed: no telemetry is pushed into a down stack).
/// - [`GenError::ExportFailed`] when `spark::init` cannot build the OTLP
///   exporter pipeline for the resolved endpoint.
pub async fn generate(config: GenConfig) -> Result<GenSummary, GenError> {
    // Fail closed FIRST (ADR-0077 F3 / US-04): never push into a down stack.
    probe_reachable(&config.endpoint).await?;

    let guard = spark::init(
        spark::SparkConfig::for_service(&config.service_name)
            .with_tenant_id(&config.tenant)
            .with_endpoint(&config.endpoint),
    )
    .map_err(|error| GenError::ExportFailed {
        endpoint: config.endpoint.clone(),
        detail: error.to_string(),
    })?;

    emit_demo_dataset();

    // Drop the guard to force-flush all three signals synchronously before
    // the summary is reported, so "the run finished" means "the telemetry is
    // on the wire" (the send-to-see loop's serialisation point).
    drop(guard);

    Ok(demo_summary())
}

/// Emit the demo dataset across all three signals via the global OTel API that
/// [`spark::init`] configured: one [`METRIC_NAME`] counter point, one
/// [`LOG_BODY`] log record emitted INSIDE the demo span (so it carries the
/// pinned trace id), and one [`SPAN_NAME`] span pinned to the verbatim
/// [`DEMO_TRACE_ID_HEX`] trace id.
fn emit_demo_dataset() {
    emit_sample_metric();
    emit_sample_span_with_correlated_log();
}

/// Increment the `request_count` counter once.
fn emit_sample_metric() {
    let meter = opentelemetry::global::meter(DEMO_INSTRUMENTATION_SCOPE);
    let counter = meter.u64_counter(METRIC_NAME).build();
    counter.add(1, &[]);
}

/// Emit the sample span pinned to the verbatim demo trace id, AND emit the
/// sample failure log INSIDE that span so the log is correlated to the trace.
///
/// The span is started under a SAMPLED remote-parent [`SpanContext`] carrying
/// [`DEMO_TRACE_ID_HEX`]; the child span inherits the parent's trace id, so the
/// by-id traces query finds it. The span is then ATTACHED as the current OTel
/// context (mirroring the PG-1 external demo's "log inside active span"
/// pattern, `with trace.use_span(child): app_logger.info(...)`), and the
/// failure log is emitted within that scope. The `tracing` event flows to OTLP
/// through the `opentelemetry-appender-tracing` bridge `spark::init` installs as
/// the process global subscriber; its non-`spark` target passes the bridge
/// filter, and the bridge stamps the trace id / span id from
/// `opentelemetry::Context::current()` at emit time — which is the attached demo
/// span. So the failure log lands carrying [`DEMO_TRACE_ID_HEX`], and a
/// by-trace_id logs query finds it. `Context::attach` moves the span-carrying
/// context into the thread-local current slot; dropping the returned guard
/// restores the prior context, which drops the demo span — ending it and
/// enqueueing it for export.
fn emit_sample_span_with_correlated_log() {
    use opentelemetry::trace::{TraceContextExt, Tracer};

    let parent = opentelemetry::trace::SpanContext::new(
        opentelemetry::trace::TraceId::from_hex(DEMO_TRACE_ID_HEX)
            .expect("the pinned demo trace id is valid 32-char hex"),
        opentelemetry::trace::SpanId::from_hex(DEMO_PARENT_SPAN_ID_HEX)
            .expect("the pinned demo parent span id is valid 16-char hex"),
        opentelemetry::trace::TraceFlags::SAMPLED,
        true,
        opentelemetry::trace::TraceState::default(),
    );
    let context = opentelemetry::Context::new().with_remote_span_context(parent);
    let tracer = opentelemetry::global::tracer(DEMO_INSTRUMENTATION_SCOPE);
    let span = tracer.start_with_context(SPAN_NAME, &context);

    // Make the demo span the CURRENT OTel context, emit the failure log within
    // that scope so the appender bridge stamps the pinned trace id, then drop
    // the guard. `Context::attach` MOVES the span-carrying context into the
    // thread-local current slot; dropping the guard restores the prior context,
    // which drops the demo span — ending it and enqueueing it for export (the
    // log is already on its way carrying the trace id). The explicit
    // `drop(guard)` makes that end-of-span point deterministic before the
    // force-flush in `generate`.
    let guard = context.with_span(span).attach();
    tracing::error!("{}", LOG_BODY);
    drop(guard);
}

/// The cardinality of the demo dataset: exactly one of each signal
/// ([`emit_demo_dataset`] pushes one metric point, one log, one span).
fn demo_summary() -> GenSummary {
    GenSummary {
        metrics_pushed: 1,
        logs_pushed: 1,
        spans_pushed: 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bind a real loopback listener and return it with its address. A bound
    /// `TcpListener` accepts connections at the OS level (the backlog) without
    /// an explicit `accept`, so it is a "reachable" endpoint for the probe.
    /// The caller MUST keep the returned listener alive for the connect to
    /// succeed.
    async fn reachable_listener() -> (tokio::net::TcpListener, std::net::SocketAddr) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind ephemeral loopback listener");
        let addr = listener.local_addr().expect("read back the bound address");
        (listener, addr)
    }

    /// An address nothing is listening on: bind then drop the listener.
    async fn closed_addr() -> std::net::SocketAddr {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind ephemeral loopback listener");
        let addr = listener.local_addr().expect("read back the bound address");
        drop(listener);
        addr
    }

    /// `generate` is fail-closed: against a down ingest endpoint it reports
    /// unreachability and NEVER reaches `spark::init` (the probe-first
    /// ordering, US-04 / ADR-0077 F3). Pins the ordering the bin relies on.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn generate_fails_closed_when_the_ingest_endpoint_is_down() {
        let addr = closed_addr().await;
        let config = GenConfig::new(format!("http://{addr}"), "acme");

        let error = generate(config)
            .await
            .expect_err("generate must fail closed against a down ingest endpoint");

        assert!(
            matches!(error, GenError::Unreachable { .. }),
            "the down-stack failure must be an unreachability report; got: {error:?}"
        );
    }

    /// When the probe passes (a reachable port) but the endpoint is not a
    /// valid OTLP target for `spark::init` (a non-http scheme), `generate`
    /// surfaces the init failure as an error rather than reporting a phantom
    /// success. Exercises the probe's reachable branch AND the `spark::init`
    /// error mapping the live happy path never hits.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn generate_surfaces_init_failure_when_the_endpoint_is_reachable_but_invalid() {
        let (_listener, addr) = reachable_listener().await;
        // Reachable for the TCP probe, but `spark::init` rejects a non-http(s)
        // scheme with InvalidEndpoint -> mapped to ExportFailed.
        let config = GenConfig::new(format!("tcp://{addr}"), "acme");

        let error = generate(config)
            .await
            .expect_err("a reachable-but-invalid endpoint must surface an init failure");

        assert!(
            matches!(error, GenError::ExportFailed { .. }),
            "a spark::init failure must surface as ExportFailed; got: {error:?}"
        );
    }

    /// The demo dataset is exactly one of each signal. Pins the summary
    /// cardinality the bin reports against the documented "one metric, one
    /// log, one span" demo contract.
    #[test]
    fn demo_summary_reports_one_of_each_signal() {
        assert_eq!(
            demo_summary(),
            GenSummary {
                metrics_pushed: 1,
                logs_pushed: 1,
                spans_pushed: 1,
            }
        );
    }

    /// The authority is the host:port after the scheme; the OTLP endpoint
    /// carries no path. Pins the parse the probe connects against.
    #[test]
    fn ingest_authority_is_the_host_port_after_the_scheme() {
        assert_eq!(ingest_authority("http://127.0.0.1:4317"), "127.0.0.1:4317");
        assert_eq!(
            ingest_authority("https://example.test:443"),
            "example.test:443"
        );
        assert_eq!(ingest_authority("127.0.0.1:4317"), "127.0.0.1:4317");
    }
}
