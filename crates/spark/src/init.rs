//! `init` — the orchestrator.
//!
//! Per ADR-0011 §"Internal layout": the full init flow lives in this
//! one module — lint pass, AtomicBool CAS (Slice 02+), Resource
//! composition, exporter construction, provider construction,
//! global-set, guard return.
//!
//! ## Slice 01 — walking skeleton
//!
//! The DELIVER pass for Slice 01 implements the happy path:
//!
//! 1. Resolve the endpoint (`SparkConfig::with_endpoint` >
//!    `OTEL_EXPORTER_OTLP_ENDPOINT` is delegated to the upstream
//!    `opentelemetry-otlp` resolver in later slices; Slice 01 honours
//!    the explicit `with_endpoint` value or falls back to the OTLP
//!    default `http://localhost:4317`).
//! 2. Compose an [`opentelemetry_sdk::Resource`] carrying
//!    `service.name` and (when set) `tenant.id`.
//! 3. Build an `opentelemetry-otlp` [`SpanExporter`] over OTLP/gRPC
//!    targeting the resolved endpoint.
//! 4. Wire the exporter into a [`opentelemetry_sdk::trace::TracerProvider`]
//!    via a batch span processor running on the Tokio runtime.
//! 5. Set the OTel global tracer provider.
//! 6. Emit the `target="spark"` `tracing::info!` event recording
//!    `spark::init succeeded` with the resolved configuration's
//!    structured fields.
//! 7. Return a [`SparkGuard`] holding the provider so its `Drop`
//!    impl can force-flush pending exports.
//!
//! Slice 02 introduces the lint pass (missing required attributes,
//! invalid endpoint) and the AtomicBool single-init flag. Slice 05
//! widens the construction to the logger and meter providers. Slice 06
//! lands the bounded sequential per-provider flush mechanism in
//! [`crate::guard::SparkGuard::drop`].

use std::time::Duration;

use opentelemetry::KeyValue;
use opentelemetry_otlp::{SpanExporter, WithExportConfig};
use opentelemetry_sdk::trace::TracerProvider as SdkTracerProvider;
use opentelemetry_sdk::{runtime, Resource};
use opentelemetry_semantic_conventions::resource::SERVICE_NAME;

use crate::config::SparkConfig;
use crate::error::SparkError;
use crate::guard::{Inner, SparkGuard};
use crate::observability;

/// The OTLP endpoint Spark uses when neither the application nor the
/// OTel-canonical environment variable provides one. Matches the
/// upstream OTLP default at `=0.27`.
const DEFAULT_ENDPOINT: &str = "http://localhost:4317";

/// The transport label Spark records on the resolved-config tracing
/// event. v0 hard-codes gRPC per ADR-0013 §1 (the v0 default
/// transport).
const PROTOCOL_GRPC: &str = "grpc";

/// The default flush deadline applied when the application did not
/// call [`SparkConfig::with_flush_timeout`]. Per DISCUSS Q4 / ADR-0014.
const DEFAULT_FLUSH_TIMEOUT: Duration = Duration::from_secs(5);

/// The `tenant.id` resource-attribute key. Kaleidoscope-house, not in
/// OTel semconv at `=0.27` (per ADR-0013 §2). Future semconv stabilisation
/// is a Codex Phase 0+ migration concern; v0 keeps the literal.
const TENANT_ID_KEY: &str = "tenant.id";

/// The pub(crate) entry the public `spark::init` delegates to.
pub(crate) fn init(config: SparkConfig) -> Result<SparkGuard, SparkError> {
    let endpoint = resolve_endpoint(&config);
    let flush_timeout = config.flush_timeout.unwrap_or(DEFAULT_FLUSH_TIMEOUT);
    let resource = build_resource(&config);

    let exporter = SpanExporter::builder()
        .with_tonic()
        .with_endpoint(endpoint.clone())
        .build()
        .map_err(|e| SparkError::ExporterInitFailed {
            reason: e.to_string(),
            source: Some(Box::new(e)),
        })?;

    let tracer_provider = SdkTracerProvider::builder()
        .with_batch_exporter(exporter, runtime::Tokio)
        .with_resource(resource)
        .build();

    let _previous = opentelemetry::global::set_tracer_provider(tracer_provider.clone());

    observability::emit_init_succeeded(
        &config.service_name,
        &endpoint,
        PROTOCOL_GRPC,
        flush_timeout,
    );

    Ok(SparkGuard {
        inner: Some(Inner {
            tracer_provider,
            flush_timeout,
        }),
    })
}

/// Compose the OTel SDK [`Resource`] carrying every set house attribute.
///
/// Slice 01 wires `service.name` (always) and `tenant.id` (when
/// `with_tenant_id` was called and the value is non-empty). Slice 03
/// extends this helper with `feature_flag.{key}` and `experiment.id`.
fn build_resource(config: &SparkConfig) -> Resource {
    let mut attributes = Vec::with_capacity(2);
    attributes.push(KeyValue::new(SERVICE_NAME, config.service_name.clone()));
    if let Some(tenant_id) = config.tenant_id.as_deref() {
        if !tenant_id.is_empty() {
            attributes.push(KeyValue::new(TENANT_ID_KEY, tenant_id.to_owned()));
        }
    }
    Resource::new(attributes)
}

/// Resolve the OTLP endpoint from the configured value, falling back
/// to the upstream default when not set.
///
/// Slice 04 introduces the full precedence chain
/// (`SparkConfig::with_endpoint` > `OTEL_EXPORTER_OTLP_ENDPOINT` >
/// default) by delegating env-var resolution to the upstream
/// `opentelemetry-otlp` resolver. Slice 01 honours the explicit
/// `with_endpoint` value or the default; the env var is not consulted.
fn resolve_endpoint(config: &SparkConfig) -> String {
    config
        .endpoint
        .clone()
        .unwrap_or_else(|| DEFAULT_ENDPOINT.to_owned())
}
