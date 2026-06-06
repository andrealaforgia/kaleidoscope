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
    /// three OTLP exporters (when resolved). Stored in a redacting
    /// [`BearerToken`] newtype so it never reaches a loggable surface
    /// (System Constraint 1). Defaulted `None` in `for_service`.
    ///
    /// DISTILL SCAFFOLD (Mandate 7, RED-not-BROKEN): this field +
    /// [`SparkConfig::with_bearer_token`] are the minimal compile
    /// scaffold so the `slice_08_ingest_auth.rs` acceptance tests
    /// COMPILE against the intended API. At DISTILL the token is
    /// stored but NOT yet attached to the exporters (DELIVER lands
    /// `build_auth_metadata` + the apply-shim in `init.rs`), so an
    /// export to an authenticated aperture is still DENIED — which is
    /// exactly what makes the auth acceptance tests behaviourally RED.
    pub(crate) bearer_token: Option<BearerToken>,
}

/// spark-ingest-auth-v0 / ADR-0069 DD3: a redacting newtype around the
/// bearer-token secret. Its `Debug` renders `BearerToken(<redacted>)`
/// and there is no value-`Display`, so `SparkConfig`'s derived `Debug`
/// (which recurses into this type) never echoes the JWT. The raw value
/// is reached only via [`BearerToken::expose`], whose single intended
/// caller (DELIVER) is `build_auth_metadata` in `init.rs`.
///
/// DISTILL SCAFFOLD (Mandate 7): the redacting `Debug` is implemented
/// NOW because the never-log acceptance test (`the_configured_token_*`)
/// asserts the redacted shape; the structural redaction is the
/// load-bearing security property (System Constraint 1) and is the one
/// behaviour the scaffold must already honour so the test classifies as
/// a genuine guardrail rather than BROKEN.
#[derive(Clone)]
pub(crate) struct BearerToken(
    // DISTILL SCAFFOLD: `expose` (the only reader) is consumed by
    // DELIVER's `build_auth_metadata`; at DISTILL the value is stored
    // and redacted but not yet read on any non-test path. `dead_code`
    // is allowed only for the scaffold window — DELIVER removes the
    // allow when it wires the accessor into `init.rs`.
    #[allow(dead_code)] String,
);

impl BearerToken {
    /// Wrap a raw token value. The secret-ness travels with the value
    /// through every move/clone (DD3).
    pub(crate) fn new(token: impl Into<String>) -> Self {
        BearerToken(token.into())
    }

    /// The raw token bytes. The single intended call site (DELIVER) is
    /// `build_auth_metadata`, which writes it into a gRPC `MetadataMap`
    /// (the wire) — never into a `tracing` macro.
    ///
    /// DISTILL SCAFFOLD: unused on non-test paths until DELIVER wires
    /// the metadata attachment; the never-log acceptance test exercises
    /// the redacting `Debug`, not this accessor.
    #[allow(dead_code)]
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
    /// uniformly (ADR-0069 DD1/DD2). Highest precedence in the auth
    /// resolution chain (`with_bearer_token` >
    /// `OTEL_EXPORTER_OTLP_HEADERS` > none).
    ///
    /// The token is a SECRET: it is stored in a redacting newtype and
    /// never appears on any loggable surface (System Constraint 1).
    ///
    /// No-token behaviour: when neither this knob nor
    /// `OTEL_EXPORTER_OTLP_HEADERS` is set, Spark attaches no
    /// `authorization` header — an unauthenticated collector keeps
    /// working unchanged (System Constraint 4). Exporting to an
    /// authenticated gateway without a token yields gateway-side
    /// `missing_claim` denials (the gateway's surfacing, not Spark's).
    ///
    /// DISTILL SCAFFOLD (Mandate 7, RED-not-BROKEN): at DISTILL this
    /// method only STORES the token; it does NOT yet attach it to the
    /// exporters (DELIVER lands the attachment in `init.rs`). An export
    /// to an authenticated aperture is therefore still DENIED at
    /// DISTILL — the deliberate RED state the `slice_08_ingest_auth.rs`
    /// acceptance tests pin.
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
