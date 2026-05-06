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

/// The pub(crate) entry the public `spark::init` delegates to.
pub(crate) fn init(config: SparkConfig) -> Result<SparkGuard, SparkError> {
    // 1. Lint pass — synchronous, no I/O, no OTel SDK type construction.
    //    Per ADR-0015 §1: runs before the AtomicBool flip so a lint
    //    failure does not leave Spark half-initialised.
    lint(&config)?;

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
pub(crate) fn reset_for_testing() {
    SPARK_INITIALISED.store(false, Ordering::Release);
}

/// Construct the OTel SDK pipeline once the lint pass has succeeded
/// and the single-init flag is owned by this caller.
///
/// Split out from `init` so the roll-back-on-Err of `SPARK_INITIALISED`
/// is a single `inspect_err` site at the call site, not duplicated on
/// every `?` early-return.
fn build_pipeline(config: SparkConfig) -> Result<SparkGuard, SparkError> {
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
fn build_resource(config: &SparkConfig) -> Resource {
    let mut attributes = Vec::with_capacity(2 + config.feature_flags.len() + 1);
    attributes.push(KeyValue::new(SERVICE_NAME, config.service_name.clone()));
    if let Some(tenant_id) = config.tenant_id.as_deref() {
        if !tenant_id.is_empty() {
            attributes.push(KeyValue::new(TENANT_ID_KEY, tenant_id.to_owned()));
        }
    }
    for (key, value) in &config.feature_flags {
        if value.is_empty() {
            continue;
        }
        let attribute_key = format!("{}{}", observability::FEATURE_FLAG_PREFIX, key);
        attributes.push(KeyValue::new(attribute_key, value.clone()));
    }
    if let Some(experiment_id) = config.experiment_id.as_deref() {
        if !experiment_id.is_empty() {
            attributes.push(KeyValue::new(EXPERIMENT_ID_KEY, experiment_id.to_owned()));
        }
    }
    Resource::new(attributes)
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
