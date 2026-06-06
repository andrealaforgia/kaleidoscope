//! `SparkConfig` — the value-consuming builder.
//!
//! Per ADR-0011 §"SparkConfig API shape": one constructor
//! [`SparkConfig::for_service`], six builder methods that take and
//! return `Self`. Every builder method is `#[must_use]`. The struct
//! itself is `#[non_exhaustive]` so future fields are non-breaking
//! additions.
//!
//! The builder methods construct and return the config; `init` reads
//! the resolved values through this module's `pub(crate)` accessors.
//! Fields are private.

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
    /// spark-ingest-auth-v0 / ADR-0069 DD2/DD3: the bearer token
    /// attached as `authorization: Bearer <token>` metadata to all
    /// three OTLP exporters (when set). Stored in a redacting
    /// [`BearerToken`] newtype so it never reaches a loggable surface
    /// (System Constraint 1). Defaulted `None` in `for_service`; read
    /// by `build_auth_metadata` in `init.rs`.
    pub(crate) bearer_token: Option<BearerToken>,
}

/// spark-ingest-auth-v0 / ADR-0069 DD3: a redacting newtype around the
/// bearer-token secret. Its `Debug` renders `BearerToken(<redacted>)`
/// and there is no value-`Display`, so `SparkConfig`'s derived `Debug`
/// (which recurses into this type) never echoes the JWT. The raw value
/// is reached only via [`BearerToken::expose`], whose single caller is
/// `build_auth_metadata` in `init.rs`. The redacting `Debug` is the
/// load-bearing security property (System Constraint 1): a future field
/// added to `SparkConfig` cannot accidentally un-redact the token, and
/// the secret-ness travels with the value through every move/clone.
#[derive(Clone)]
pub(crate) struct BearerToken(String);

impl BearerToken {
    /// Wrap a raw token value. The secret-ness travels with the value
    /// through every move/clone (DD3).
    pub(crate) fn new(token: impl Into<String>) -> Self {
        BearerToken(token.into())
    }

    /// The raw token bytes. The single call site is `build_auth_metadata`
    /// in `init.rs`, which writes it into a gRPC `MetadataMap` (the wire)
    /// — never into a `tracing` macro (DD3).
    pub(crate) fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for BearerToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("BearerToken(<redacted>)")
    }
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
            bearer_token: None,
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

    /// Set the bearer token attached as `authorization: Bearer <token>`
    /// metadata to all three OTLP exporters (spans, logs, metrics),
    /// uniformly (ADR-0069 DD1). This is the supported in-code way to
    /// authenticate Spark's telemetry against a gateway that demands a
    /// bearer credential (e.g. an aegis-authenticated aperture).
    ///
    /// The token is a SECRET: it is stored in a redacting newtype and
    /// never appears on any loggable surface (System Constraint 1 /
    /// ADR-0069 DD3).
    ///
    /// # Precedence vs `OTEL_EXPORTER_OTLP_HEADERS` (env-as-override)
    ///
    /// `opentelemetry-otlp =0.27` honours the conventional
    /// `OTEL_EXPORTER_OTLP_HEADERS` env var natively on Spark's exporter
    /// construction path, with percent-decoding, independently of this
    /// knob (ADR-0069 § Amendment). When BOTH this knob AND a
    /// concurrently-set `OTEL_EXPORTER_OTLP_HEADERS=authorization=...`
    /// are present, the upstream exporter merges them with
    /// `HeaderMap::extend`, which OVERWRITES on key collision — so the
    /// **env-set `authorization` is the final writer and takes effect**
    /// (env-as-override). This knob is the primary in-code API; a
    /// concurrently-set env header is an operator override applied last
    /// by upstream. Spark writes no env-handling code (DD2-revised).
    ///
    /// # No-token behaviour
    ///
    /// When neither this knob nor `OTEL_EXPORTER_OTLP_HEADERS` is set,
    /// Spark attaches no `authorization` header — an unauthenticated
    /// collector keeps working unchanged (System Constraint 4 / DD5).
    /// Exporting to an authenticated gateway without a token yields
    /// gateway-side `missing_claim` denials (the gateway's surfacing,
    /// not Spark's). Spark sends whatever token it is given honestly; it
    /// does not pre-validate `exp`/`iss`/`aud` — a rejected token is the
    /// gateway's judgement.
    #[must_use]
    pub fn with_bearer_token(mut self, token: impl Into<String>) -> Self {
        self.bearer_token = Some(BearerToken::new(token));
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
