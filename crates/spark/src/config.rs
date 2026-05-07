//! `SparkConfig` — the value-consuming builder.
//!
//! Per ADR-0011 §"SparkConfig API shape": one constructor
//! [`SparkConfig::for_service`], six builder methods that take and
//! return `Self`. Every builder method is `#[must_use]`. The struct
//! itself is `#[non_exhaustive]` so future fields are non-breaking
//! additions.
//!
//! ## DISTILL state
//!
//! The builder methods at DISTILL are real (not `unimplemented!()`):
//! they construct and return the config so the integration tests under
//! `tests/` can build configs the same way DELIVER will. The
//! `unimplemented!()` panic lives inside `init.rs` — once `init` runs,
//! every config field is read for the first time and the day-one stub
//! panics.
//!
//! Fields are private; the resolved values reach `init` through this
//! module's `pub(crate)` accessors.

use std::time::Duration;

/// Configuration for Spark's initialisation.
///
/// Constructed via [`SparkConfig::for_service`] which forces
/// `service.name` at construction time. Subsequent builder methods
/// add optional or opt-in-required attributes and operator-tunable
/// values.
///
/// All fields are private; the resolution chain
/// (`SparkConfig::with_endpoint` > `OTEL_EXPORTER_OTLP_ENDPOINT` >
/// default) lives inside `init.rs`.
#[non_exhaustive]
#[derive(Debug, Clone)]
#[allow(dead_code)] // DISTILL state: fields are wired up by init.rs in DELIVER.
pub struct SparkConfig {
    pub(crate) service_name: String,
    pub(crate) tenant_id_required: bool,
    pub(crate) tenant_id: Option<String>,
    pub(crate) feature_flags: Vec<(String, String)>,
    pub(crate) experiment_id: Option<String>,
    pub(crate) endpoint: Option<String>,
    pub(crate) flush_timeout: Option<Duration>,
    /// Slice 07 / ADR-0025: opt-in strict-mode schema lint. Default
    /// `false` (warn mode). When `true`, a Codex `LintReport` from
    /// the schema-lint pass at `init` time produces
    /// `Err(SparkError::SchemaValidation(report))` instead of a
    /// `tracing::warn!` event. Useful for CI integration tests where
    /// a misconfigured resource attribute should fail-fast.
    pub(crate) strict_schema_lint: bool,
}

impl SparkConfig {
    /// Construct a `SparkConfig` for the given service. The
    /// `service.name` is the OTel-canonical service identifier
    /// (always required); empty values are caught by the lint pass
    /// at [`crate::init`] time.
    #[must_use]
    pub fn for_service(name: impl Into<String>) -> SparkConfig {
        SparkConfig {
            service_name: name.into(),
            tenant_id_required: false,
            tenant_id: None,
            feature_flags: Vec::new(),
            experiment_id: None,
            endpoint: None,
            flush_timeout: None,
            strict_schema_lint: false,
        }
    }

    /// Mark `tenant.id` as required for this configuration. With this
    /// flag set, `init` returns
    /// [`SparkError::MissingRequiredAttribute`](crate::SparkError::MissingRequiredAttribute)
    /// if `tenant.id` is absent or empty. Defaults to off (single-
    /// tenant Spark integrations succeed without a `tenant.id`).
    #[must_use]
    pub fn require_tenant_id(mut self) -> Self {
        self.tenant_id_required = true;
        self
    }

    /// Set the `tenant.id` resource attribute. Empty strings are
    /// treated identically to absence by the lint pass.
    #[must_use]
    pub fn with_tenant_id(mut self, tenant_id: impl Into<String>) -> Self {
        self.tenant_id = Some(tenant_id.into());
        self
    }

    /// Set the `feature_flag.{key}` resource attributes. Accepts any
    /// `IntoIterator<Item = (impl Into<String>, impl Into<String>)>`,
    /// covering `HashMap<String, String>`, `BTreeMap<&str, &str>`,
    /// `Vec<(&str, &str)>`, and array literals like
    /// `[("checkout-v2", "on")]`.
    ///
    /// Empty-value entries are skipped by the Resource composer (per
    /// Slice 03's UAT "Empty-string optional attributes are skipped").
    /// Multiple calls accumulate (the pairs are appended, not
    /// replaced).
    #[must_use]
    pub fn with_feature_flags<I, K, V>(mut self, flags: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        for (k, v) in flags {
            self.feature_flags.push((k.into(), v.into()));
        }
        self
    }

    /// Set the `experiment.id` resource attribute. Empty strings are
    /// skipped by the Resource composer.
    #[must_use]
    pub fn with_experiment_id(mut self, experiment_id: impl Into<String>) -> Self {
        self.experiment_id = Some(experiment_id.into());
        self
    }

    /// Set the OTLP endpoint URL. Highest precedence in the
    /// resolution chain (`SparkConfig::with_endpoint` >
    /// `OTEL_EXPORTER_OTLP_ENDPOINT` > default `http://localhost:4317`).
    /// Parsed at [`crate::init`] time; an unparseable value or a
    /// non-http(s) scheme produces
    /// [`SparkError::InvalidEndpoint`](crate::SparkError::InvalidEndpoint).
    #[must_use]
    pub fn with_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.endpoint = Some(endpoint.into());
        self
    }

    /// Set the flush timeout for [`SparkGuard`](crate::SparkGuard) on
    /// drop. Default 5 s. The total drop time is bounded by this
    /// duration regardless of per-provider behaviour (see ADR-0014).
    #[must_use]
    pub fn with_flush_timeout(mut self, timeout: Duration) -> Self {
        self.flush_timeout = Some(timeout);
        self
    }

    /// Configure strict-mode schema lint (Slice 07 / ADR-0025).
    ///
    /// Default: `false` (warn mode). A `codex::LintReport` from the
    /// `init`-time schema lint is emitted as a single
    /// `tracing::warn!(target = "spark", ...)` event; `init` returns
    /// `Ok(SparkGuard)`. This is the operationally safe rollout
    /// posture: existing Spark deployments do not see new init
    /// failures when this slice ships.
    ///
    /// Strict mode (`true`): a `codex::LintReport` causes `init` to
    /// return `Err(SparkError::SchemaValidation(report))`. Useful for
    /// CI integration tests where a misconfigured resource attribute
    /// should fail-fast rather than scroll past in warn output.
    #[must_use]
    pub fn with_strict_schema_lint(mut self, strict: bool) -> Self {
        self.strict_schema_lint = strict;
        self
    }
}
