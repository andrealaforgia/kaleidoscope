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
//! 1. Resolve the endpoint via [`resolve_endpoint`]. Slice 01 honours
//!    the explicit `SparkConfig::with_endpoint` value or falls back
//!    to the OTLP default `http://localhost:4317`. Slice 04 extends
//!    the resolution chain to consult the `OTEL_EXPORTER_OTLP_ENDPOINT`
//!    env var between the explicit value and the default.
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
//! invalid endpoint) and the AtomicBool single-init flag. Slice 04
//! adds the env-var precedence path (`OTEL_EXPORTER_OTLP_ENDPOINT`)
//! between the explicit builder value and the default, with the same
//! `InvalidEndpoint` validation applied to the env-supplied URL.
//! Slice 05 widens the construction to the logger and meter providers.
//! Slice 06 lands the bounded sequential per-provider flush mechanism
//! in [`crate::guard::SparkGuard::drop`].

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use codex::SchemaCatalogue;
use opentelemetry::KeyValue;
use opentelemetry_otlp::{LogExporter, MetricExporter, SpanExporter, WithExportConfig};
use opentelemetry_sdk::logs::{BatchLogProcessor, LoggerProvider as SdkLoggerProvider};
use opentelemetry_sdk::metrics::{PeriodicReader, SdkMeterProvider};
use opentelemetry_sdk::trace::{BatchSpanProcessor, TracerProvider as SdkTracerProvider};
use opentelemetry_sdk::{runtime, Resource};
use opentelemetry_semantic_conventions::resource::SERVICE_NAME;
use tracing_subscriber::filter::FilterFn;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::Layer;

use crate::config::SparkConfig;
use crate::error::SparkError;
use crate::guard::{Inner, SparkGuard};
use crate::observability;

/// The OTLP endpoint Spark uses when neither the application nor the
/// OTel-canonical environment variable provides one. Matches the
/// upstream OTLP default at `=0.27`.
const DEFAULT_ENDPOINT: &str = "http://localhost:4317";

/// The OTel-canonical environment variable Spark consults when the
/// application did not call [`crate::SparkConfig::with_endpoint`].
/// Per US-SP-04 / Slice 04: the OTel-spec name is the only env var
/// Spark reads at v0; Spark does NOT introduce `SPARK_*` env vars
/// (System Constraint 9 in `discuss/user-stories.md`).
const ENV_OTLP_ENDPOINT: &str = "OTEL_EXPORTER_OTLP_ENDPOINT";

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

/// The `experiment.id` resource-attribute key. Kaleidoscope-house, not
/// in OTel semconv at `=0.27` (per ADR-0013 §2). Same forward-compat
/// posture as `tenant.id`: v0 keeps the literal; Codex Phase 0+ owns
/// any future semconv alignment.
const EXPERIMENT_ID_KEY: &str = "experiment.id";

/// The `service.name` semantic-conventions key, in literal form. The
/// upstream constant `SERVICE_NAME` resolves to the same string but is
/// a `Key`/`&str` constant; the lint pass needs the literal `String`.
const SERVICE_NAME_KEY: &str = "service.name";

/// Spark-internal single-init flag (per ADR-0015 §1).
///
/// Catches the common case ("application calls `spark::init` twice in
/// `main`"). The OTel SDK 0.27 `set_tracer_provider` is infallible, so
/// without this flag a second Spark init would silently replace the
/// previously-set global provider — exactly the silent-replacement
/// hazard the invariant exists to prevent.
///
/// The flag is set *after* the lint pass succeeds (so a lint failure
/// does not consume the single-init budget) and is rolled back on any
/// post-flag failure (so a retry after `ExporterInitFailed` etc. gets
/// a clean attempt).
static SPARK_INITIALISED: AtomicBool = AtomicBool::new(false);

/// Process-global slot for the `LoggerProvider` Spark constructed at
/// init time. Doc-hidden test seam — the integration tests retrieve
/// the provider after `init` returns to build their own
/// `OpenTelemetryTracingBridge` and install it into the test
/// subscriber's reload slot. Production code never calls this; the
/// slot is populated in `build_pipeline` and cleared by
/// `reset_for_testing`.
///
/// ADR-0017 specifies that Spark's `init` "configures the OTel
/// `LoggerProvider`, builds an `OpenTelemetryTracingBridge` against
/// it, and adds that bridge as a `tracing_subscriber::Layer`". Spark's
/// init does install the global subscriber via
/// `tracing::subscriber::set_global_default` when no subscriber is
/// already present (the production path); when a subscriber is already
/// installed (the test path: `tests/common/mod.rs` pre-installs a
/// Registry with a `SparkCaptureLayer` so `target="spark"` events are
/// observed for D5 invariant checks), Spark's `set_global_default`
/// fails silently and the bridge is exposed via this slot for the test
/// to wire into its own subscriber via a `reload::Handle`.
static TEST_LOGGER_PROVIDER: Mutex<Option<SdkLoggerProvider>> = Mutex::new(None);

/// Codex schema catalogue. Built lazily on the first `init` call;
/// re-used across all subsequent inits in the same process. Per
/// ADR-0025 §2: the static corpus inside `SchemaCatalogue::new` is
/// large enough that rebuilding it on every `init` would add
/// measurable boot-time overhead.
///
/// Spark's single-init invariant (ADR-0015) means a long-running
/// process performs at most one successful `init` per guard lifetime.
/// `OnceLock` matches this shape: build once, read many.
static CATALOGUE: OnceLock<SchemaCatalogue> = OnceLock::new();

/// Slice 07 / ADR-0025: accessor for the lazy schema catalogue.
fn catalogue() -> &'static SchemaCatalogue {
    CATALOGUE.get_or_init(SchemaCatalogue::new)
}

/// The pub(crate) entry the public `spark::init` delegates to.
pub(crate) fn init(config: SparkConfig) -> Result<SparkGuard, SparkError> {
    // 1. Internal lint pass — synchronous, no I/O, no OTel SDK type
    //    construction. Per ADR-0015 §1: runs before the AtomicBool
    //    flip so a lint failure does not leave Spark half-initialised.
    lint(&config)?;

    // 1b. Slice 07 / ADR-0025 — Codex schema lint pass. Runs after
    //     the internal lint (which catches missing/invalid required
    //     attributes) and before the AtomicBool flip (so a lint
    //     failure does not consume the single-init budget). The
    //     composed resource attribute pairs are checked against
    //     Codex's catalogue; violations are reported either as a
    //     single `tracing::warn!(target = "spark")` event (default)
    //     or as `Err(SparkError::SchemaValidation)` when the caller
    //     opted into strict mode via
    //     `SparkConfig::with_strict_schema_lint(true)`.
    codex_schema_lint(&config)?;

    // 2. Atomic compare-and-swap on Spark's own flag (ADR-0015 §1).
    //    `AcqRel` on success synchronises with subsequent loads; `Acquire`
    //    on failure ensures we observe the prior store that set the flag.
    if SPARK_INITIALISED
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return Err(SparkError::GlobalAlreadyInitialised);
    }

    // From here on, every Err must roll the flag back to false so a
    // retry after a transient failure (e.g. ExporterInitFailed) gets a
    // clean attempt — per ADR-0015 §"Roll-back on failure".
    build_pipeline(config).inspect_err(|_| {
        SPARK_INITIALISED.store(false, Ordering::Release);
    })
}

/// Reset Spark's per-process single-init flag.
///
/// Doc-hidden test seam. Spark's integration test suite reuses one
/// process for several `init` calls (e.g. Slice 01's seven walking-
/// skeleton tests share one binary per ADR-0015 §2's `[[test]]` rule).
/// Production code never calls this — the AtomicBool's set-once
/// semantic across the process lifetime is the contract every
/// application depends on.
///
/// The function intentionally does NOT reset OTel SDK global state
/// (`opentelemetry::global::set_tracer_provider` has no public reset
/// API at `=0.27`); only Spark's own flag is reset. Tests that need a
/// fresh OTel global provider must already serialise via
/// `serial_test::serial` and accept that the global is whatever the
/// most recent `init` set.
///
/// Slice 05 widening: also clears the doc-hidden `TEST_LOGGER_PROVIDER`
/// slot so a subsequent `init` call cannot leak the previous
/// `LoggerProvider` into a test that retrieves it before its own
/// `init` runs.
pub(crate) fn reset_for_testing() {
    SPARK_INITIALISED.store(false, Ordering::Release);
    *TEST_LOGGER_PROVIDER
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
}

/// Release Spark's single-init reservation when the `SparkGuard`
/// drops.
///
/// Per ADR-0015 §1 the AtomicBool defends the "two `init` calls while
/// a guard is alive" case (the operationally load-bearing scenario:
/// double-init in `main` would silently replace the OTel global
/// provider). Once the guard drops, the OTel pipeline is being torn
/// down and a fresh `init` is a sensible production scenario:
/// hot-reload of OTel configuration, multi-phase application
/// shutdown/restart, or integration-test binaries that exercise the
/// init→drop cycle multiple times within one process.
///
/// The single-init invariant remains intact for the live-guard case
/// because the flag is set inside the `init` body and only released
/// when the guard's `Drop::drop` runs. `invariant_single_init.rs`
/// exercises the contract: the test holds the first guard until after
/// the second `init` call, so the second call observes the flag set
/// and returns `GlobalAlreadyInitialised`.
///
/// Production code never calls this directly — the call is wired into
/// `crate::guard::SparkGuard::Drop`.
pub(crate) fn reset_after_drop() {
    SPARK_INITIALISED.store(false, Ordering::Release);
}

/// Doc-hidden test seam: retrieve the `LoggerProvider` Spark's most
/// recent successful `init` call constructed.
///
/// The integration test fixture uses this to build its own
/// `OpenTelemetryTracingBridge` and install it into the test
/// subscriber's reload slot, which is how `target != "spark"`
/// `tracing::*!` events become OTel `LogRecord`s flowing through
/// Spark's pipeline. The contract guarantees: returns `Some(...)`
/// only when an `init` call has succeeded since the last
/// `reset_for_testing`.
///
/// Production code never invokes this — Spark's `init` already
/// installs a global tracing subscriber containing the bridge layer
/// (when no subscriber is pre-installed), so applications that do not
/// install their own subscriber automatically get the bridge wiring.
pub(crate) fn test_logger_provider() -> Option<SdkLoggerProvider> {
    TEST_LOGGER_PROVIDER
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
}

/// Construct the OTel SDK pipeline once the lint pass has succeeded
/// and the single-init flag is owned by this caller.
///
/// Split out from `init` so the roll-back-on-Err of `SPARK_INITIALISED`
/// is a single `inspect_err` site at the call site, not duplicated on
/// every `?` early-return.
///
/// At Slice 05: constructs all three OTel SDK signal-type providers
/// (`TracerProvider`, `LoggerProvider`, `SdkMeterProvider`) sharing
/// one `Resource`, sets the OTel global tracer/meter providers, builds
/// the `opentelemetry-appender-tracing` bridge against the
/// `LoggerProvider` (filtered to exclude `target = "spark"` per
/// ADR-0017 §3 / D5), and tries to install the bridge as a
/// `tracing_subscriber` Layer via `set_global_default`. When a
/// subscriber is already installed (test path), the bridge is
/// retained on the `LoggerProvider` slot for the test fixture to wire
/// into its own subscriber via a reload handle.
fn build_pipeline(config: SparkConfig) -> Result<SparkGuard, SparkError> {
    let endpoint = resolve_endpoint(&config);
    let flush_timeout = config.flush_timeout.unwrap_or(DEFAULT_FLUSH_TIMEOUT);
    let resource = build_resource(&config);

    // -- Traces -----------------------------------------------------------
    let span_exporter = SpanExporter::builder()
        .with_tonic()
        .with_endpoint(endpoint.clone())
        .build()
        .map_err(|e| SparkError::ExporterInitFailed {
            reason: e.to_string(),
            source: Some(Box::new(e)),
        })?;

    // Per ADR-0014 §1: bind each batch processor's `max_export_timeout`
    // to `flush_timeout` so the SDK's own export-completion deadline
    // matches Spark's drop-time deadline. Without this, the SDK's
    // default 30 s export timeout would trump Spark's configured
    // `flush_timeout` and the "drop completes within ~deadline"
    // contract (Slice 06 Case B) would not hold for sub-second
    // deadlines.
    let span_batch_config = opentelemetry_sdk::trace::BatchConfigBuilder::default()
        .with_max_export_timeout(flush_timeout)
        .build();
    let span_processor = BatchSpanProcessor::builder(span_exporter, runtime::Tokio)
        .with_batch_config(span_batch_config)
        .build();
    let tracer_provider = SdkTracerProvider::builder()
        .with_span_processor(span_processor)
        .with_resource(resource.clone())
        .build();

    // -- Logs -------------------------------------------------------------
    // Per ADR-0017: the OTLP/gRPC log exporter feeds a batch processor
    // on the same Tokio runtime that drives the trace pipeline. The
    // `LoggerProvider` carries the same `Resource` as the tracer (KPI
    // 5: identical Resource across all three signal types).
    let log_exporter = LogExporter::builder()
        .with_tonic()
        .with_endpoint(endpoint.clone())
        .build()
        .map_err(|e| SparkError::ExporterInitFailed {
            reason: e.to_string(),
            source: Some(Box::new(e)),
        })?;

    // Same `max_export_timeout = flush_timeout` binding as the tracer
    // provider above. The logs `BatchConfigBuilder` is a separate type
    // from the trace one (`opentelemetry_sdk::logs::BatchConfigBuilder`
    // vs `opentelemetry_sdk::trace::BatchConfigBuilder`) but exposes
    // the same `with_max_export_timeout` shape.
    let log_batch_config = opentelemetry_sdk::logs::BatchConfigBuilder::default()
        .with_max_export_timeout(flush_timeout)
        .build();
    let log_processor = BatchLogProcessor::builder(log_exporter, runtime::Tokio)
        .with_batch_config(log_batch_config)
        .build();
    let logger_provider = SdkLoggerProvider::builder()
        .with_log_processor(log_processor)
        .with_resource(resource.clone())
        .build();

    // -- Metrics ----------------------------------------------------------
    // OTel SDK 0.27 has no `with_batch_exporter` for metrics; the
    // metric pipeline is driven by a `PeriodicReader` against the OTLP
    // metric exporter. The default 60 s interval is too long for the
    // Slice 05 acceptance tests, but the integration tests rely on the
    // `force_flush` at guard drop rather than the periodic interval.
    let metric_exporter = MetricExporter::builder()
        .with_tonic()
        .with_endpoint(endpoint.clone())
        .build()
        .map_err(|e| SparkError::ExporterInitFailed {
            reason: e.to_string(),
            source: Some(Box::new(e)),
        })?;

    // The metric reader's `with_timeout` is the equivalent of the
    // batch processors' `max_export_timeout`. Bound it to
    // `flush_timeout` so the meter provider's `force_flush` cannot
    // outlive Spark's drop-time deadline.
    let metric_reader = PeriodicReader::builder(metric_exporter, runtime::Tokio)
        .with_timeout(flush_timeout)
        .build();

    let meter_provider = SdkMeterProvider::builder()
        .with_reader(metric_reader)
        .with_resource(resource)
        .build();

    // -- Global provider registration ------------------------------------
    let _previous_tracer = opentelemetry::global::set_tracer_provider(tracer_provider.clone());
    opentelemetry::global::set_meter_provider(meter_provider.clone());

    // -- Bridge installation (logs path) ---------------------------------
    // ADR-0017 §3: the bridge MUST exclude `target = "spark"` events
    // so Spark's own diagnostics (init success, shutdown initiated,
    // flush deadline, etc.) do NOT feed back into the OTel pipeline
    // Spark configured (D5 / no-telemetry-on-telemetry).
    let bridge =
        opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge::new(&logger_provider)
            .with_filter(FilterFn::new(non_spark_target));
    let registry = tracing_subscriber::registry().with(bridge);
    let _ = tracing::subscriber::set_global_default(registry);
    // If `set_global_default` returned `Err`, a subscriber was already
    // installed by the application (or, in the integration tests, by
    // `tests/common/mod.rs`'s `SparkCaptureLayer` install). The test
    // path retrieves the `LoggerProvider` via the doc-hidden seam
    // below to build its own bridge and inject it through a reload
    // handle on the test's pre-installed Registry. Production
    // applications that have their own subscriber must compose the
    // bridge themselves; the v0 contract documents this in ADR-0017.
    *TEST_LOGGER_PROVIDER
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(logger_provider.clone());

    observability::emit_init_succeeded(
        &config.service_name,
        &endpoint,
        PROTOCOL_GRPC,
        flush_timeout,
    );

    Ok(SparkGuard {
        inner: Some(Inner {
            tracer_provider,
            logger_provider,
            meter_provider,
            flush_timeout,
        }),
    })
}

/// Filter callback for the `opentelemetry-appender-tracing` bridge.
///
/// Returns `true` for events Spark wants the bridge to forward (i.e.
/// every `tracing` event whose target is NOT `"spark"`); returns
/// `false` for Spark's own diagnostic events. This is the load-bearing
/// implementation detail that defends D5 / no-telemetry-on-telemetry
/// (ADR-0017 §3): Spark's `tracing::info!(target: "spark", ...)`
/// events flow to the application's tracing facade, never through the
/// OTel pipeline Spark configured.
///
/// Pulled out as a free function so it can be tested in isolation and
/// so mutation testing can exercise the boundary condition (`==`
/// flipped to `!=` flips the invariant).
fn non_spark_target(metadata: &tracing::Metadata<'_>) -> bool {
    metadata.target() != observability::TARGET
}

/// Synchronous configuration-only lint. Per ADR-0015 §1: runs before
/// any OTel SDK type is constructed and before the AtomicBool single-
/// init flag is touched, so a lint failure never leaves Spark in a
/// half-initialised state.
///
/// Lint order matters per the dispatch brief: required-attribute checks
/// precede endpoint parsing, so a missing `service.name` is reported
/// even if the endpoint is also malformed.
///
/// Endpoint validation runs against whatever the resolution chain
/// returns when the value is operator-supplied (i.e. config builder or
/// env var). The default fallback `http://localhost:4317` is a Spark-
/// owned literal that always parses; we skip re-validating it on every
/// init call. Per US-SP-04 / ADR-0011 §"InvalidEndpoint": an env var
/// carrying a malformed URL produces the same `InvalidEndpoint` variant
/// the explicit-builder typo case produces.
fn lint(config: &SparkConfig) -> Result<(), SparkError> {
    if config.service_name.is_empty() {
        return Err(SparkError::MissingRequiredAttribute {
            name: SERVICE_NAME_KEY.to_owned(),
        });
    }

    if config.tenant_id_required {
        let tenant_id = config.tenant_id.as_deref().unwrap_or("");
        if tenant_id.is_empty() {
            return Err(SparkError::MissingRequiredAttribute {
                name: TENANT_ID_KEY.to_owned(),
            });
        }
    }

    if let Some(operator_endpoint) = operator_supplied_endpoint(config) {
        validate_endpoint(&operator_endpoint)?;
    }

    Ok(())
}

/// Validate an explicit endpoint literal: must parse as a URL and the
/// scheme must be `http` or `https`. Per US-SP-02 example 3 and
/// `slice-02-init-error-paths.md`'s "InvalidEndpoint fires on URI parse
/// failure and on scheme-not-http-or-https".
fn validate_endpoint(endpoint: &str) -> Result<(), SparkError> {
    let parsed = url::Url::parse(endpoint).map_err(|e| SparkError::InvalidEndpoint {
        endpoint: endpoint.to_owned(),
        reason: e.to_string(),
    })?;
    let scheme = parsed.scheme();
    if scheme != "http" && scheme != "https" {
        return Err(SparkError::InvalidEndpoint {
            endpoint: endpoint.to_owned(),
            reason: format!("scheme {scheme:?} is not http or https"),
        });
    }
    Ok(())
}

/// Compose the resource attribute (key, value) pairs that a fresh
/// `init` call would set on the OTel SDK Resource. Owned strings so
/// the result outlives the source `SparkConfig` borrow.
///
/// The skip policy mirrors `build_resource`: empty optional values
/// are dropped (per US-SP-03 UAT). Empty `feature_flag.` keys are
/// NOT skipped — they survive to the Codex schema-lint pass at Slice
/// 07 / ADR-0025, which catches the malformed `feature_flag.`
/// (no-suffix) attribute as a Prefix violation.
///
/// The function is used by `build_resource` (for the OTel SDK type)
/// and by `codex_schema_lint` (for the Codex validate call). The
/// single composer ensures both paths agree on which attribute keys
/// the OTel SDK would have seen, so a Codex violation reported here
/// is on a key the SDK would actually have carried.
fn compose_resource_pairs(config: &SparkConfig) -> Vec<(String, String)> {
    let mut pairs: Vec<(String, String)> = Vec::with_capacity(2 + config.feature_flags.len() + 1);
    pairs.push((SERVICE_NAME_KEY.to_owned(), config.service_name.clone()));
    if let Some(tenant_id) = config.tenant_id.as_deref() {
        if !tenant_id.is_empty() {
            pairs.push((TENANT_ID_KEY.to_owned(), tenant_id.to_owned()));
        }
    }
    for (key, value) in &config.feature_flags {
        if value.is_empty() {
            continue;
        }
        let attribute_key = format!("{}{}", observability::FEATURE_FLAG_PREFIX, key);
        pairs.push((attribute_key, value.clone()));
    }
    if let Some(experiment_id) = config.experiment_id.as_deref() {
        if !experiment_id.is_empty() {
            pairs.push((EXPERIMENT_ID_KEY.to_owned(), experiment_id.to_owned()));
        }
    }
    pairs
}

/// Compose the OTel SDK [`Resource`] carrying every set house attribute.
///
/// Slice 01 wires `service.name` (always) and `tenant.id` (when
/// `with_tenant_id` was called and the value is non-empty). Slice 03
/// extends this helper with `feature_flag.{key}` (one attribute per
/// non-empty pair, namespace-prefixed per `feature_flag_namespace` in
/// `shared-artifacts-registry.md`) and `experiment.id` (when set and
/// non-empty). Empty-string values are skipped throughout (per
/// US-SP-03 UAT "Empty-string optional attributes are skipped, not
/// emitted").
///
/// Slice 07 refactors the body to share its (key, value) pair logic
/// with [`compose_resource_pairs`] so the Codex schema lint at
/// `codex_schema_lint` operates on the exact set of keys the SDK
/// Resource will carry.
fn build_resource(config: &SparkConfig) -> Resource {
    let pairs = compose_resource_pairs(config);
    let mut attributes = Vec::with_capacity(pairs.len());
    for (key, value) in pairs {
        // Special-case service.name to use the upstream typed key
        // constant (preserves Slice 01's existing observable shape
        // for the OTel SDK Resource).
        if key == SERVICE_NAME_KEY {
            attributes.push(KeyValue::new(SERVICE_NAME, value));
        } else {
            attributes.push(KeyValue::new(key, value));
        }
    }
    Resource::new(attributes)
}

/// Slice 07 / ADR-0025 — Codex schema-lint pass.
///
/// Validates the composed resource attributes against Codex's
/// `SchemaCatalogue`. On `Err(report)`:
///
/// - **strict mode** (`config.strict_schema_lint == true`): returns
///   `Err(SparkError::SchemaValidation(report))` so CI integration
///   tests can fail-fast on misconfigured resource attributes.
/// - **default (warn) mode**: emits a single
///   `tracing::warn!(target = "spark", ...)` event whose body
///   contains the full `LintReport` `Display` rendering, then
///   returns `Ok(())` so init continues. This is the operationally
///   safe rollout posture per ADR-0025 §3.
///
/// The lint runs **before** the AtomicBool single-init flip and
/// **before** any OTel SDK type construction, so a violation in
/// strict mode never half-initialises Spark.
fn codex_schema_lint(config: &SparkConfig) -> Result<(), SparkError> {
    let pairs = compose_resource_pairs(config);
    let pair_refs: Vec<(&str, &str)> = pairs
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    if let Err(report) = catalogue().validate(&pair_refs) {
        if config.strict_schema_lint {
            return Err(SparkError::SchemaValidation(report));
        }
        tracing::warn!(target: "spark", "{report}");
    }
    Ok(())
}

/// Resolve the OTLP endpoint along the documented precedence chain:
/// `SparkConfig::with_endpoint` > `OTEL_EXPORTER_OTLP_ENDPOINT` env var
/// > default `http://localhost:4317`.
///
/// Per US-SP-04 / Slice 04 / `shared-artifacts-registry.md > otlp_endpoint`:
/// the explicit builder value wins outright; the env var is consulted
/// only when the application did not call `with_endpoint`; the default
/// is the lowest-precedence fallback. Empty env-var values are treated
/// as absent (an empty `OTEL_EXPORTER_OTLP_ENDPOINT=""` falls through
/// to the default rather than producing an invalid endpoint).
fn resolve_endpoint(config: &SparkConfig) -> String {
    operator_supplied_endpoint(config).unwrap_or_else(|| DEFAULT_ENDPOINT.to_owned())
}

/// Return the operator-supplied endpoint (highest two precedence
/// levels): the `SparkConfig::with_endpoint` value if present,
/// otherwise the `OTEL_EXPORTER_OTLP_ENDPOINT` env-var value if
/// present and non-empty. `None` indicates the resolution chain
/// would fall through to Spark's default literal.
///
/// Centralised so the lint pass and the resolution path share one
/// definition of "did the operator supply something?". Per
/// US-SP-04 / ADR-0011 §"InvalidEndpoint": only operator-supplied
/// endpoints are URL-validated; the Spark default is a known-good
/// literal.
fn operator_supplied_endpoint(config: &SparkConfig) -> Option<String> {
    if let Some(explicit) = config.endpoint.as_deref() {
        return Some(explicit.to_owned());
    }
    let env_value = std::env::var(ENV_OTLP_ENDPOINT).ok()?;
    if env_value.is_empty() {
        return None;
    }
    Some(env_value)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Slice 07 / ADR-0025 §2: pin the OnceLock invariant. Two
    /// successive `catalogue()` calls must return the exact same
    /// reference (same memory address), not two equivalent
    /// `SchemaCatalogue` instances. A `Box::leak(Box::new(default()))`
    /// implementation would observationally agree on `validate(...)`
    /// output but allocate a fresh catalogue on every call — pointer
    /// identity is the deterministic mutation-evidence anchor for the
    /// `get_or_init` implementation choice.
    #[test]
    fn catalogue_returns_the_same_instance_across_calls() {
        let first = catalogue() as *const SchemaCatalogue;
        let second = catalogue() as *const SchemaCatalogue;
        assert!(std::ptr::eq(first, second));
    }
}
