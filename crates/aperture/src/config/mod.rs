//! Aperture configuration.
//!
//! The full schema lives in
//! `docs/feature/aperture/design/component-design.md > Configuration schema`.
//! Slice 01 lights up the smallest viable surface: gRPC + HTTP bind
//! addresses, sink kind (defaulting to stub), and a few forward-compat
//! knobs the integration tests can pin without naming the whole schema.
//! Slice 07 will replace this with the figment-driven TOML loader.

use std::net::SocketAddr;
use std::path::Path;
use std::time::Duration;

/// Aperture configuration.
///
/// Field-public-by-design within the crate; the integration tests
/// construct configurations through [`Config::builder`] and never
/// inspect the fields directly.
///
/// Several fields (`forwarding_endpoint`, `forwarding_timeout`,
/// `max_concurrent_requests`, `drain_deadline`, `tls_enabled`,
/// `spiffe_enabled`) are accepted by the builder at Slice 01 but not
/// yet read by the application core — the slices that exercise them
/// (06, 05, 08, 07) will introduce the consumers. The
/// `#[allow(dead_code)]` is per-field rather than per-struct so a
/// genuinely-orphan field still warns.
#[derive(Debug, Clone)]
pub struct Config {
    pub(crate) grpc_bind_addr: SocketAddr,
    #[allow(dead_code)]
    pub(crate) http_bind_addr: SocketAddr,
    pub(crate) sink_kind: SinkKind,
    #[allow(dead_code)]
    pub(crate) forwarding_endpoint: String,
    #[allow(dead_code)]
    pub(crate) forwarding_timeout: Duration,
    #[allow(dead_code)]
    pub(crate) max_concurrent_requests: u32,
    #[allow(dead_code)]
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

    /// Load a configuration from a TOML file. Slice 07 lands the
    /// figment-driven loader; until then this returns a not-yet-supported
    /// error rather than panicking.
    pub fn from_toml_path(_path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        Err(ConfigError(
            "Config::from_toml_path is not implemented until Slice 07".to_string(),
        ))
    }

    /// Parse a TOML string. Slice 07 lands the figment loader.
    pub fn from_toml_str(_toml: &str) -> Result<Self, ConfigError> {
        Err(ConfigError(
            "Config::from_toml_str is not implemented until Slice 07".to_string(),
        ))
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

    /// Pin the per-transport concurrency cap.
    pub fn max_concurrent_requests(mut self, cap: u32) -> Self {
        self.max_concurrent_requests = cap;
        self
    }

    /// Pin the drain deadline (graceful shutdown).
    pub fn drain_deadline(mut self, deadline: Duration) -> Self {
        self.drain_deadline = deadline;
        self
    }

    /// Configure the sink kind to `forwarding` and pin the downstream
    /// endpoint. Slice 06 exercises this.
    pub fn forwarding_sink(mut self, endpoint: impl Into<String>) -> Self {
        self.sink_kind = SinkKind::Forwarding;
        self.forwarding_endpoint = endpoint.into();
        self
    }

    /// Pin the forwarding-sink request timeout.
    pub fn forwarding_timeout(mut self, timeout: Duration) -> Self {
        self.forwarding_timeout = timeout;
        self
    }

    /// Set the forward-compat `tls.enabled` knob.
    pub fn tls_enabled(mut self, enabled: bool) -> Self {
        self.tls_enabled = enabled;
        self
    }

    /// Set the forward-compat `auth.spiffe.enabled` knob.
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
        let either_ephemeral = self.grpc_bind_addr.port() == 0 || self.http_bind_addr.port() == 0;
        if !either_ephemeral && self.grpc_bind_addr == self.http_bind_addr {
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
        assert!(cfg.is_ok(), "two `127.0.0.1:0` must build OK: {:?}", cfg);
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
}
