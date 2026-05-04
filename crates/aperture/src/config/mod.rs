//! Aperture configuration. DISTILL stub.
//!
//! The full schema lives in
//! `docs/feature/aperture/design/component-design.md > Configuration schema`.
//! At DISTILL we expose only the surface the integration tests build
//! configurations against; DELIVER replaces this with a `figment`-driven
//! TOML loader.

// SCAFFOLD: true
// Status: DISTILL RED stub. Setters return `self` (idiomatic builder
// shape); `Config::from_toml_path`, `Config::from_toml_str`, and
// `ConfigBuilder::build` panic with `unimplemented!()`. DELIVER replaces
// the stub with the figment loader and the rich `ApertureConfig`
// schema per design/component-design.md.

use std::net::SocketAddr;
use std::path::Path;
use std::time::Duration;

/// Aperture configuration. DELIVER replaces this with the full struct
/// hierarchy in `design/component-design.md`. The integration tests
/// build configurations through [`Config::builder`].
#[derive(Debug)]
pub struct Config {
    _private: (),
}

impl Config {
    /// Start building a configuration. The builder lets tests pin
    /// specific fields without naming the whole schema, which is what
    /// the slice tests do (each test cares about one or two knobs).
    pub fn builder() -> ConfigBuilder {
        ConfigBuilder::new()
    }

    /// Load a configuration from a TOML file. DELIVER replaces this
    /// with the full `figment` loader; at DISTILL it panics. Slice 07's
    /// schema-knob test uses this entry point.
    pub fn from_toml_path(_path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        unimplemented!(
            "aperture::config::Config::from_toml_path — DELIVER lands this with Slice 07"
        )
    }

    /// Parse a TOML string. Used by the schema-knob slice to assert
    /// forward-compat keys parse without errors.
    pub fn from_toml_str(_toml: &str) -> Result<Self, ConfigError> {
        unimplemented!("aperture::config::Config::from_toml_str — DELIVER lands this with Slice 07")
    }
}

/// Configuration error. DELIVER replaces this with the
/// `ApertureError::ConfigInvalid` mapping per the design contract.
#[derive(Debug)]
pub struct ConfigError(pub String);

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ConfigError {}

/// Builder for [`Config`]. DELIVER replaces the body; the surface is
/// what the integration tests rely on.
#[derive(Debug)]
pub struct ConfigBuilder {
    _private: (),
}

impl Default for ConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ConfigBuilder {
    /// Start an empty builder. All knobs default to the design-spec
    /// defaults; setters override individual values.
    pub fn new() -> Self {
        Self { _private: () }
    }

    /// Pin the gRPC bind address. Tests pass `"127.0.0.1:0"` to bind on
    /// an ephemeral port discovered after startup via
    /// [`crate::Handle::grpc_addr`].
    pub fn grpc_bind_addr(self, _addr: SocketAddr) -> Self {
        let _ = _addr;
        self
    }

    /// Pin the HTTP bind address.
    pub fn http_bind_addr(self, _addr: SocketAddr) -> Self {
        let _ = _addr;
        self
    }

    /// Pin the per-transport concurrency cap.
    pub fn max_concurrent_requests(self, _cap: u32) -> Self {
        let _ = _cap;
        self
    }

    /// Pin the drain deadline (graceful shutdown).
    pub fn drain_deadline(self, _deadline: Duration) -> Self {
        let _ = _deadline;
        self
    }

    /// Configure the sink kind to `forwarding` and pin the downstream
    /// endpoint. Used by Slice 06.
    pub fn forwarding_sink(self, _endpoint: impl Into<String>) -> Self {
        let _ = _endpoint.into();
        self
    }

    /// Pin the forwarding-sink request timeout (default 5 s per
    /// design).
    pub fn forwarding_timeout(self, _timeout: Duration) -> Self {
        let _ = _timeout;
        self
    }

    /// Set the forward-compat `tls.enabled` knob to `true`. Slice 07
    /// uses this to assert the warn-line behaviour.
    pub fn tls_enabled(self, _enabled: bool) -> Self {
        let _ = _enabled;
        self
    }

    /// Set the forward-compat `auth.spiffe.enabled` knob to `true`.
    pub fn spiffe_enabled(self, _enabled: bool) -> Self {
        let _ = _enabled;
        self
    }

    /// Build the configuration. DELIVER replaces this with a real
    /// validator; at DISTILL it panics.
    pub fn build(self) -> Result<Config, ConfigError> {
        unimplemented!(
            "aperture::config::ConfigBuilder::build — DELIVER lands this in config/mod.rs"
        )
    }
}
