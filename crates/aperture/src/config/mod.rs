//! Aperture configuration.
//!
//! The full schema lives in
//! `docs/feature/aperture/design/component-design.md > Configuration schema`.
//! Slice 01 lit up the typed builder; Slice 07 lands the figment-driven
//! TOML loader (ADR-0008) with `deny_unknown_fields` on every nested
//! struct so misspelled keys fail loud at config load.

use std::net::SocketAddr;
use std::path::Path;
use std::time::Duration;

use figment::{
    providers::{Env, Format},
    Figment,
};
use serde::Deserialize;

/// Env-var provider for ADR-0008's `APERTURE__` convention.
///
/// `Env::prefixed("APERTURE__").split("__")` strips the `APERTURE__`
/// prefix and converts remaining `__` separators into `.` path joins.
/// The schema wraps everything under `[aperture]`, so we re-prepend
/// the wrapper key with `.map()` — that way
/// `APERTURE__SINK__KIND=stub` resolves to `aperture.sink.kind`,
/// matching the TOML file shape and ADR-0008's documented examples.
fn env_provider() -> Env {
    Env::prefixed("APERTURE__")
        .split("__")
        .map(|key| format!("aperture.{key}").into())
}

/// Aperture configuration.
///
/// Field-public-by-design within the crate; the integration tests
/// construct configurations through [`Config::builder`] and never
/// inspect the fields directly.
///
/// Most fields are read by the call site that owns the corresponding
/// slice (forwarding sink in Slice 06, concurrency cap in Slice 05,
/// drain deadline in Slice 08, TLS / SPIFFE knobs in Slice 07 — the
/// last two are forward-compat at v0 and emit one warn line). The
/// `#[allow(dead_code)]` is per-field rather than per-struct so a
/// genuinely-orphan field still warns.
#[derive(Debug, Clone)]
pub struct Config {
    pub(crate) grpc_bind_addr: SocketAddr,
    #[allow(dead_code)]
    pub(crate) http_bind_addr: SocketAddr,
    pub(crate) sink_kind: SinkKind,
    pub(crate) forwarding_endpoint: String,
    pub(crate) forwarding_timeout: Duration,
    pub(crate) max_concurrent_requests: u32,
    pub(crate) drain_deadline: Duration,
    #[allow(dead_code)]
    pub(crate) tls_enabled: bool,
    #[allow(dead_code)]
    pub(crate) spiffe_enabled: bool,
}

/// Which sink the composition root wires up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SinkKind {
    Stub,
    Forwarding,
}

impl Config {
    /// Start building a configuration. The builder lets tests pin
    /// specific fields without naming the whole schema.
    pub fn builder() -> ConfigBuilder {
        ConfigBuilder::new()
    }

    /// Load a configuration from a TOML file (ADR-0008). The figment
    /// `Toml::file` provider reads the file; the
    /// `Env::prefixed("APERTURE__")` provider then layers env-var
    /// overrides on top (env beats file, per ADR-0008's "in that
    /// order" clause). The typed schema below rejects unknown fields
    /// per nested struct, so a misspelled key surfaces as a parse
    /// error rather than a silent default.
    ///
    /// The schema wraps everything under `[aperture]`, but ADR-0008's
    /// env-var convention drops that wrapper
    /// (`APERTURE__SINK__KIND=stub` overrides `[aperture.sink].kind`).
    /// The `.map()` call below restores the wrapper at provider time
    /// so env keys land in the same namespace as the TOML file.
    pub fn from_toml_path(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let raw: RawConfig = Figment::new()
            .merge(figment::providers::Toml::file(path.as_ref()))
            .merge(env_provider())
            .extract()
            .map_err(|e| ConfigError(format!("config parse failed: {e}")))?;
        raw.into_config()
    }

    /// Parse a TOML string (ADR-0008). Same provider stack as
    /// [`Config::from_toml_path`]; used by the integration tests and
    /// as the in-memory entry point for the binary's `--config <FILE>`
    /// flag (which `main.rs` wires in via `from_toml_path` for the
    /// file path; this method is the in-memory variant the integration
    /// tests reach for).
    pub fn from_toml_str(toml: &str) -> Result<Self, ConfigError> {
        let raw: RawConfig = Figment::new()
            .merge(figment::providers::Toml::string(toml))
            .merge(env_provider())
            .extract()
            .map_err(|e| ConfigError(format!("config parse failed: {e}")))?;
        raw.into_config()
    }

    pub(crate) fn grpc_bind_addr(&self) -> SocketAddr {
        self.grpc_bind_addr
    }

    #[allow(dead_code)]
    pub(crate) fn http_bind_addr(&self) -> SocketAddr {
        self.http_bind_addr
    }

    pub(crate) fn sink_kind(&self) -> SinkKind {
        self.sink_kind
    }

    /// Downstream endpoint configured for the forwarding sink. Empty
    /// when `sink_kind == Stub`. Slice 06 reads this when wiring the
    /// real `ForwardingSink`; the binary path validates non-emptiness
    /// at config-load time (Slice 07's figment loader).
    pub(crate) fn forwarding_endpoint(&self) -> &str {
        &self.forwarding_endpoint
    }

    /// Per-request timeout for the forwarding sink's downstream HTTP
    /// client. Slice 06 uses this for `accept`-path POSTs; the probe
    /// path uses its own 2 s budget per the design contract.
    pub(crate) fn forwarding_timeout(&self) -> Duration {
        self.forwarding_timeout
    }

    /// Per-transport concurrency cap. Slice 05 wires this into the
    /// gRPC and HTTP listener semaphores; Slice 08 reuses the same
    /// `Semaphore::available_permits()` to compute the in-flight count
    /// for the drain orchestrator (ADR-0010).
    pub(crate) fn max_concurrent_requests(&self) -> u32 {
        self.max_concurrent_requests
    }

    /// Drain deadline applied by the Slice 08 shutdown orchestrator.
    /// Default 30 s (k8s `terminationGracePeriodSeconds`-friendly).
    /// On expiry, in-flight requests are abandoned and a
    /// `event=drain_deadline_exceeded` warn line names the dropped
    /// count.
    pub(crate) fn drain_deadline(&self) -> Duration {
        self.drain_deadline
    }

    /// Forward-compat TLS knob (ADR-0008 / Slice 07). True at v0 means
    /// the operator opted in to TLS, but Aperture v0 ships plaintext
    /// only — the composition root emits one
    /// `event=tls_not_supported_in_v0` warn line and continues binding
    /// plaintext listeners. Phase 2 (Aegis) will read this knob and
    /// the `cert_path` / `key_path` to terminate TLS.
    pub(crate) fn tls_enabled(&self) -> bool {
        self.tls_enabled
    }

    /// Forward-compat SPIFFE knob (ADR-0008 / Slice 07). True at v0
    /// means the operator opted in to workload-identity-based auth,
    /// but Aperture v0 ships no auth — the composition root reuses the
    /// same `event=tls_not_supported_in_v0` warn line (per ADR-0008's
    /// "single warn per config-load" stance) and continues binding
    /// listeners with no auth.
    pub(crate) fn spiffe_enabled(&self) -> bool {
        self.spiffe_enabled
    }
}

/// Configuration error.
#[derive(Debug)]
pub struct ConfigError(pub String);

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ConfigError {}

/// Builder for [`Config`].
#[derive(Debug, Clone)]
pub struct ConfigBuilder {
    grpc_bind_addr: SocketAddr,
    http_bind_addr: SocketAddr,
    sink_kind: SinkKind,
    forwarding_endpoint: String,
    forwarding_timeout: Duration,
    max_concurrent_requests: u32,
    drain_deadline: Duration,
    tls_enabled: bool,
    spiffe_enabled: bool,
}

impl Default for ConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ConfigBuilder {
    /// Start a builder pre-loaded with the design-spec defaults.
    pub fn new() -> Self {
        Self {
            grpc_bind_addr: "0.0.0.0:4317".parse().expect("default grpc addr parses"),
            http_bind_addr: "0.0.0.0:4318".parse().expect("default http addr parses"),
            sink_kind: SinkKind::Stub,
            forwarding_endpoint: String::new(),
            forwarding_timeout: Duration::from_millis(5000),
            max_concurrent_requests: 1024,
            drain_deadline: Duration::from_millis(30_000),
            tls_enabled: false,
            spiffe_enabled: false,
        }
    }

    /// Pin the gRPC bind address. Tests pass `"127.0.0.1:0"` to bind on
    /// an ephemeral port discovered after startup via
    /// [`crate::Handle::grpc_addr`].
    pub fn grpc_bind_addr(mut self, addr: SocketAddr) -> Self {
        self.grpc_bind_addr = addr;
        self
    }

    /// Pin the HTTP bind address.
    pub fn http_bind_addr(mut self, addr: SocketAddr) -> Self {
        self.http_bind_addr = addr;
        self
    }

    // Setters for `max_concurrent_requests`, `drain_deadline`,
    // `forwarding_sink`, `forwarding_timeout`, `tls_enabled`,
    // `spiffe_enabled` are present here so the RED slice tests under
    // `tests/slice_{05,06,07,08}*.rs` compile against a stable
    // builder surface. Each setter's behaviour will be covered by a
    // GREEN test when its slice lands; until then the setters mutate
    // the builder field but the field is unread — the
    // `#[allow(dead_code)]` on the corresponding `Config` field keeps
    // the build clean. Mutation testing reports these as MISSED at
    // Slice 01 boundary by design (the slice that introduces the
    // setter's behaviour-asserting test will close the gap).

    /// Pin the per-transport concurrency cap. Behaviour exercised by
    /// `slice_05_backpressure.rs`.
    pub fn max_concurrent_requests(mut self, cap: u32) -> Self {
        self.max_concurrent_requests = cap;
        self
    }

    /// Pin the drain deadline. Behaviour exercised by
    /// `slice_08_graceful_shutdown.rs`.
    pub fn drain_deadline(mut self, deadline: Duration) -> Self {
        self.drain_deadline = deadline;
        self
    }

    /// Configure the sink kind to `forwarding` and pin the downstream
    /// endpoint. Behaviour exercised by `slice_06_forwarding_sink.rs`.
    pub fn forwarding_sink(mut self, endpoint: impl Into<String>) -> Self {
        self.sink_kind = SinkKind::Forwarding;
        self.forwarding_endpoint = endpoint.into();
        self
    }

    /// Pin the forwarding-sink request timeout. Behaviour exercised by
    /// `slice_06_forwarding_sink.rs`.
    pub fn forwarding_timeout(mut self, timeout: Duration) -> Self {
        self.forwarding_timeout = timeout;
        self
    }

    /// Set the forward-compat `tls.enabled` knob. Behaviour exercised
    /// by `slice_07_tls_schema_knob.rs`.
    pub fn tls_enabled(mut self, enabled: bool) -> Self {
        self.tls_enabled = enabled;
        self
    }

    /// Set the forward-compat `auth.spiffe.enabled` knob. Behaviour
    /// exercised by `slice_07_tls_schema_knob.rs`.
    pub fn spiffe_enabled(mut self, enabled: bool) -> Self {
        self.spiffe_enabled = enabled;
        self
    }

    /// Build the configuration.
    ///
    /// Slice 01 only validates that the gRPC and HTTP bind addresses are
    /// distinct (US-AP-01 UAT); subsequent slices add the rest of the
    /// post-deserialise validation rules from `component-design.md >
    /// Configuration schema`.
    ///
    /// Port `0` (OS-assigned ephemeral) is exempt from the
    /// "addresses must differ" check — two textual `127.0.0.1:0`
    /// configurations resolve to distinct ports at bind time, which
    /// is what the integration-test fixture relies on.
    pub fn build(self) -> Result<Config, ConfigError> {
        // Reject identical non-ephemeral bind addresses. Port 0 means
        // "OS-assigned" — the two textually-equal `127.0.0.1:0`
        // strings resolve to distinct ports at bind time, which is
        // what the integration-test fixture relies on. The check is
        // intentionally narrow: the only configuration we reject is
        // the one that cannot bind successfully.
        let conflicting_pinned_ports =
            self.grpc_bind_addr.port() != 0 && self.grpc_bind_addr == self.http_bind_addr;
        if conflicting_pinned_ports {
            return Err(ConfigError(format!(
                "grpc and http bind addresses must differ; both set to {}",
                self.grpc_bind_addr
            )));
        }
        Ok(Config {
            grpc_bind_addr: self.grpc_bind_addr,
            http_bind_addr: self.http_bind_addr,
            sink_kind: self.sink_kind,
            forwarding_endpoint: self.forwarding_endpoint,
            forwarding_timeout: self.forwarding_timeout,
            max_concurrent_requests: self.max_concurrent_requests,
            drain_deadline: self.drain_deadline,
            tls_enabled: self.tls_enabled,
            spiffe_enabled: self.spiffe_enabled,
        })
    }
}

// =========================================================================
// TOML schema — figment + serde with `deny_unknown_fields` (ADR-0008)
// =========================================================================
//
// The `RawConfig` shape mirrors the on-disk TOML the operator writes.
// Every nested struct sets `#[serde(deny_unknown_fields)]` so a
// misspelled key (`max_concurent_requests` instead of
// `max_concurrent_requests`) surfaces as a parse error rather than a
// silent default-value-use. The resulting `RawConfig` is then folded
// into the typed `Config` through `RawConfig::into_config`, which
// applies the same cross-field validation the builder does (e.g.
// rejecting identical pinned bind addresses).

/// Top-level TOML schema. The single `aperture` table mirrors ADR-0008.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConfig {
    aperture: ApertureSection,
}

/// `[aperture]` table. Holds the transport, sink, security, and
/// shutdown sub-tables.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ApertureSection {
    transport: TransportSection,
    #[serde(default)]
    sink: SinkSection,
    #[serde(default)]
    security: SecuritySection,
    #[serde(default)]
    shutdown: ShutdownSection,
}

/// `[aperture.transport]` table — the gRPC and HTTP arms.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TransportSection {
    grpc: TransportArm,
    http: TransportArm,
}

/// `[aperture.transport.{grpc,http}]` arm. `bind_addr` is required;
/// the size and concurrency knobs default to ADR-0008 values.
///
/// `max_recv_msg_size` is parsed for forward-compat (the schema is
/// shared between v0 and Phase 2) but unused at v0 — Slice 05's
/// concurrency limiter is the only backpressure surface lit up. The
/// field-level `#[allow(dead_code)]` keeps strict-clippy quiet
/// without suppressing the warning for genuinely-orphan fields.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TransportArm {
    bind_addr: SocketAddr,
    #[serde(default)]
    #[allow(dead_code)]
    max_recv_msg_size: Option<u32>,
    #[serde(default)]
    max_concurrent_requests: Option<u32>,
}

/// `[aperture.sink]` table — sink kind and forwarding details.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SinkSection {
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    forwarding: ForwardingSection,
}

/// `[aperture.sink.forwarding]` arm. Both fields default to empty so
/// `kind = "stub"` configurations need not name them.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct ForwardingSection {
    #[serde(default)]
    endpoint: Option<String>,
    #[serde(default)]
    timeout_ms: Option<u64>,
}

/// `[aperture.security]` table — TLS and SPIFFE knobs.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SecuritySection {
    #[serde(default)]
    tls: TlsSection,
    #[serde(default)]
    auth: AuthSection,
}

/// `[aperture.security.tls]` arm. `enabled` defaults to false; the
/// `cert_path` and `key_path` keys are accepted for forward-compat
/// (Phase 2 Aegis) but unused at v0.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct TlsSection {
    #[serde(default)]
    enabled: bool,
    #[serde(default)]
    #[allow(dead_code)]
    cert_path: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    key_path: Option<String>,
}

/// `[aperture.security.auth]` arm. SPIFFE is the only auth scheme the
/// schema names at v0; future schemes are additive.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct AuthSection {
    #[serde(default)]
    spiffe: SpiffeSection,
}

/// `[aperture.security.auth.spiffe]` arm. Same forward-compat shape as
/// `TlsSection` — the keys are accepted, only `enabled` flips
/// behaviour (and at v0 the only behaviour change is the warn line).
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SpiffeSection {
    #[serde(default)]
    enabled: bool,
    #[serde(default)]
    #[allow(dead_code)]
    workload_api_socket: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    trust_domain: Option<String>,
}

/// `[aperture.shutdown]` table. Only `drain_deadline_ms` is exposed at
/// v0; Slice 08 reads it.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct ShutdownSection {
    #[serde(default)]
    drain_deadline_ms: Option<u64>,
}

impl RawConfig {
    /// Fold the raw TOML schema into a typed [`Config`]. Re-uses the
    /// builder so cross-field validation (identical pinned bind
    /// addresses, etc.) lives in one place.
    fn into_config(self) -> Result<Config, ConfigError> {
        let aperture = self.aperture;

        let mut builder = Config::builder()
            .grpc_bind_addr(aperture.transport.grpc.bind_addr)
            .http_bind_addr(aperture.transport.http.bind_addr);

        // Per-transport concurrency cap. ADR-0008 declares the field
        // per-transport but ADR-0010 / Slice 05 takes a single cap at
        // v0. We honour the gRPC value when set; the HTTP value (if
        // distinct) is ignored at v0 by design — the binary's
        // `validate_config` will warn if they differ once Slice 08
        // lands the post-deserialise validator. At v0 we accept both
        // keys silently to keep the schema test surface small.
        if let Some(cap) = aperture.transport.grpc.max_concurrent_requests {
            builder = builder.max_concurrent_requests(cap);
        }

        // Sink selection. Default `kind` is "stub" per ADR-0008.
        match aperture.sink.kind.as_deref() {
            None | Some("stub") => {}
            Some("forwarding") => {
                let endpoint = aperture
                    .sink
                    .forwarding
                    .endpoint
                    .clone()
                    .unwrap_or_default();
                builder = builder.forwarding_sink(endpoint);
                if let Some(ms) = aperture.sink.forwarding.timeout_ms {
                    builder = builder.forwarding_timeout(Duration::from_millis(ms));
                }
            }
            Some(other) => {
                return Err(ConfigError(format!(
                    "unknown sink kind {other:?}; expected \"stub\" or \"forwarding\""
                )));
            }
        }

        if let Some(ms) = aperture.shutdown.drain_deadline_ms {
            builder = builder.drain_deadline(Duration::from_millis(ms));
        }

        builder = builder
            .tls_enabled(aperture.security.tls.enabled)
            .spiffe_enabled(aperture.security.auth.spiffe.enabled);

        builder.build()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_accepts_two_ephemeral_addresses_with_port_zero() {
        // Both ports are 0 (ephemeral); the OS assigns distinct ports
        // at bind time, so the textual equality must not be treated as
        // a conflict. The integration-test fixture relies on this.
        let cfg = Config::builder()
            .grpc_bind_addr("127.0.0.1:0".parse().unwrap())
            .http_bind_addr("127.0.0.1:0".parse().unwrap())
            .build();
        assert!(cfg.is_ok(), "two `127.0.0.1:0` must build OK: {cfg:?}");
    }

    #[test]
    fn build_rejects_identical_addresses() {
        let cfg = Config::builder()
            .grpc_bind_addr("127.0.0.1:4317".parse().unwrap())
            .http_bind_addr("127.0.0.1:4317".parse().unwrap())
            .build();
        assert!(cfg.is_err());
    }

    #[test]
    fn build_accepts_distinct_addresses() {
        let cfg = Config::builder()
            .grpc_bind_addr("127.0.0.1:4317".parse().unwrap())
            .http_bind_addr("127.0.0.1:4318".parse().unwrap())
            .build();
        assert!(cfg.is_ok());
    }

    #[test]
    fn build_accepts_when_only_grpc_port_is_ephemeral() {
        // grpc=0, http=4318 — distinct at bind time, must build OK.
        // Pinned because the `||` short-circuit is mutation-tested:
        // an `&&` flip would treat this as a conflict only when BOTH
        // ports are 0; this test forces the disjunction shape.
        let cfg = Config::builder()
            .grpc_bind_addr("127.0.0.1:0".parse().unwrap())
            .http_bind_addr("127.0.0.1:4318".parse().unwrap())
            .build();
        assert!(cfg.is_ok());
    }

    #[test]
    fn build_accepts_when_only_http_port_is_ephemeral() {
        // grpc=4317, http=0 — symmetric to the test above. Together
        // these two pin the `||` truth table independently of the
        // both-zero / neither-zero cases.
        let cfg = Config::builder()
            .grpc_bind_addr("127.0.0.1:4317".parse().unwrap())
            .http_bind_addr("127.0.0.1:0".parse().unwrap())
            .build();
        assert!(cfg.is_ok());
    }

    #[test]
    fn max_concurrent_requests_setter_round_trips_to_built_config() {
        // Slice 05 made the cap field load-bearing. Pin the setter
        // against an `Ok(Default::default())` mutation that would
        // silently drop the configured cap and leave the limiter at
        // the default 1024. The unit-level pin is the deterministic
        // defence: the slice-05 integration tests catch the same flip
        // by hanging (no refusal arrives), but a deterministic kill
        // is faster and easier to read in CI.
        let cfg = Config::builder()
            .grpc_bind_addr("127.0.0.1:0".parse().unwrap())
            .http_bind_addr("127.0.0.1:0".parse().unwrap())
            .max_concurrent_requests(7)
            .build()
            .expect("config builds");
        assert_eq!(cfg.max_concurrent_requests(), 7);
    }

    #[test]
    fn drain_deadline_setter_round_trips_to_built_config() {
        // Slice 08 made drain_deadline load-bearing. Pin the setter
        // against an `Ok(Default::default())` mutation that would
        // silently drop the configured deadline to 0 ms — the
        // shutdown orchestrator would then emit
        // `drain_deadline_exceeded` immediately on every shutdown,
        // breaking clean-drain semantics.
        let cfg = Config::builder()
            .grpc_bind_addr("127.0.0.1:0".parse().unwrap())
            .http_bind_addr("127.0.0.1:0".parse().unwrap())
            .drain_deadline(Duration::from_millis(7777))
            .build()
            .expect("config builds");
        assert_eq!(cfg.drain_deadline(), Duration::from_millis(7777));
    }

    #[test]
    fn default_drain_deadline_is_thirty_seconds() {
        // ADR-0008 / DISCUSS Q1 lock the default deadline at 30 s,
        // aligned with k8s `terminationGracePeriodSeconds`. Pin the
        // default against a mutation that would drop it to zero
        // (deadline-exceeded on every shutdown) or raise it to a
        // value that breaks operator-visible drain semantics.
        let cfg = Config::builder()
            .grpc_bind_addr("127.0.0.1:0".parse().unwrap())
            .http_bind_addr("127.0.0.1:0".parse().unwrap())
            .build()
            .expect("config builds");
        assert_eq!(cfg.drain_deadline(), Duration::from_secs(30));
    }

    #[test]
    fn env_var_override_replaces_toml_sink_kind_per_adr_0008() {
        // ADR-0008 declares the loader contract: `Toml::file(path)` +
        // `Env::prefixed("APERTURE__")` providers, in that order, so an
        // operator can override one knob without shipping a full
        // aperture.toml. The catalogue's expectations harness relies on
        // this for per-expectation overrides
        // (~/dev/kaleidoscope-expectations issue 002).
        //
        // figment::Jail isolates the env var to this test process scope
        // so other tests (and other CI runners) do not see it.
        figment::Jail::expect_with(|jail| {
            jail.set_env("APERTURE__SINK__KIND", "stub");
            let toml = r#"
                [aperture.transport.grpc]
                bind_addr = "127.0.0.1:0"

                [aperture.transport.http]
                bind_addr = "127.0.0.1:0"

                [aperture.sink]
                kind = "forwarding"

                [aperture.sink.forwarding]
                endpoint = "http://downstream:4318"
            "#;
            let config = Config::from_toml_str(toml).expect("config parses with env override");
            assert_eq!(
                config.sink_kind(),
                SinkKind::Stub,
                "APERTURE__SINK__KIND=stub must override [aperture.sink].kind=forwarding"
            );
            Ok(())
        });
    }

    #[test]
    fn env_var_override_replaces_toml_max_concurrent_requests_per_adr_0008() {
        // Issue 002's exact reproducer: cap the gRPC semaphore to 1 via
        // env var, even when the TOML pins it at 16. The catalogue's
        // A09 backpressure expectation needs this knob to be
        // overridable without shipping a per-expectation TOML.
        figment::Jail::expect_with(|jail| {
            jail.set_env("APERTURE__TRANSPORT__GRPC__MAX_CONCURRENT_REQUESTS", "1");
            let toml = r#"
                [aperture.transport.grpc]
                bind_addr = "127.0.0.1:0"
                max_concurrent_requests = 16

                [aperture.transport.http]
                bind_addr = "127.0.0.1:0"
            "#;
            let config = Config::from_toml_str(toml).expect("config parses with env override");
            assert_eq!(
                config.max_concurrent_requests(),
                1,
                "APERTURE__TRANSPORT__GRPC__MAX_CONCURRENT_REQUESTS=1 must override TOML 16"
            );
            Ok(())
        });
    }

    #[test]
    fn env_var_with_no_toml_override_leaves_toml_value_in_place() {
        // Symmetry check: when no env var is set, the TOML value wins
        // (i.e. the env provider doesn't accidentally introduce a
        // default that overrides the file). Pins the file-first /
        // env-overrides-file ordering against a mutation that would
        // swap the `merge` order.
        figment::Jail::expect_with(|_jail| {
            let toml = r#"
                [aperture.transport.grpc]
                bind_addr = "127.0.0.1:0"
                max_concurrent_requests = 42

                [aperture.transport.http]
                bind_addr = "127.0.0.1:0"
            "#;
            let config = Config::from_toml_str(toml).expect("config parses");
            assert_eq!(config.max_concurrent_requests(), 42);
            Ok(())
        });
    }

    #[test]
    fn default_max_concurrent_requests_is_one_thousand_twenty_four() {
        // ADR-0010 locks the default cap at 1024 per transport. Pin
        // the default value against a mutation that would drop the
        // cap to zero (refuse-everything) or raise it to u32::MAX
        // (refuse-nothing).
        let cfg = Config::builder()
            .grpc_bind_addr("127.0.0.1:0".parse().unwrap())
            .http_bind_addr("127.0.0.1:0".parse().unwrap())
            .build()
            .expect("config builds");
        assert_eq!(cfg.max_concurrent_requests(), 1024);
    }
}
