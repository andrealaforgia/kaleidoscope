// Kaleidoscope query-api — composition-root logic (testable seam)
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

//! Composition-root logic, lifted into the lib seam (DD1) so the
//! fail-closed tenant resolution and the Earned-Trust probe (DD9) are
//! unit-testable in isolation rather than buried in the binary. The
//! thin `src/main.rs` only reads the environment and calls these.

use std::net::SocketAddr;
use std::path::PathBuf;

use aegis::TenantId;
use pulse::{MetricName, MetricStore, TimeRange};

/// Default `pillar_root` when neither the CLI arg nor the env var is
/// set. Relative to the working directory, mirroring the gateway.
pub const DEFAULT_PILLAR_ROOT: &str = "kaleidoscope-data";
/// Sub-path under `pillar_root` for the pulse metric store.
pub const PULSE_SUBDIR: &str = "pulse";
/// Default listen address: the conventional Prometheus HTTP API port,
/// so an operator pointing Prism's `backend.url` at the host needs no
/// extra mapping.
pub const DEFAULT_ADDR: &str = "0.0.0.0:9090";

/// Resolve `pillar_root` from an optional CLI arg, then an optional env
/// value, then the default. Pure over its inputs so the precedence is
/// testable without touching the process environment.
pub fn resolve_pillar_root(cli_arg: Option<String>, env_value: Option<String>) -> PathBuf {
    if let Some(arg) = cli_arg {
        return PathBuf::from(arg);
    }
    if let Some(env_path) = env_value {
        return PathBuf::from(env_path);
    }
    PathBuf::from(DEFAULT_PILLAR_ROOT)
}

/// Resolve the tenant from the `KALEIDOSCOPE_QUERY_TENANT` value.
/// Fail-closed: unset (`None`) or empty maps to `None`, which the
/// router refuses (DD7).
pub fn resolve_tenant(env_value: Option<String>) -> Option<TenantId> {
    match env_value {
        Some(tenant) if !tenant.is_empty() => Some(TenantId(tenant)),
        _ => None,
    }
}

/// Resolve the listen address from the `KALEIDOSCOPE_QUERY_ADDR` value,
/// else the default port. Returns the parse error so the binary can
/// surface a malformed address rather than panicking.
pub fn resolve_addr(env_value: Option<String>) -> Result<SocketAddr, std::net::AddrParseError> {
    let raw = env_value.unwrap_or_else(|| DEFAULT_ADDR.to_string());
    raw.parse()
}

/// Resolve the same-origin static-serving directory from the
/// `KALEIDOSCOPE_QUERY_STATIC_DIR` value (DD3/DD6, ADR-0043).
/// Default-off: unset (`None`) or empty maps to `None`, which leaves the
/// router API-only (byte-for-byte today's read side). A non-empty value
/// is the bundle directory the `ServeDir` fallback is pointed at.
pub fn resolve_static_dir(env_value: Option<String>) -> Option<PathBuf> {
    match env_value {
        Some(dir) if !dir.is_empty() => Some(PathBuf::from(dir)),
        _ => None,
    }
}

/// Earned-Trust probe (DD9): assert a tenant resolves AND the store
/// answers a trivial query before the listener binds. A `None` tenant
/// is the fail-closed refusal; a store error is a read refusal. An
/// empty `Ok` result is success.
pub fn probe(
    store: &(dyn MetricStore + Send + Sync),
    tenant: Option<&TenantId>,
) -> Result<(), String> {
    let Some(tenant) = tenant else {
        return Err("KALEIDOSCOPE_QUERY_TENANT is unset or empty (fail-closed)".to_string());
    };
    let sentinel = MetricName::new("__query_api_startup_probe__");
    store
        .query(tenant, &sentinel, TimeRange::new(0, 0))
        .map(|_| ())
        .map_err(|e| format!("metric store probe failed: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulse::{InMemoryMetricStore, NoopRecorder};

    // The binary's main() is a thin reader of the environment; the
    // testable precedence and the fail-closed/probe invariants live
    // here so they are mutation-killed by unit tests rather than left
    // to the composition root.

    #[test]
    fn pillar_root_precedence_is_cli_then_env_then_default() {
        assert_eq!(
            resolve_pillar_root(Some("cli".to_string()), Some("env".to_string())),
            PathBuf::from("cli"),
            "the CLI arg wins"
        );
        assert_eq!(
            resolve_pillar_root(None, Some("env".to_string())),
            PathBuf::from("env"),
            "the env value is next"
        );
        assert_eq!(
            resolve_pillar_root(None, None),
            PathBuf::from(DEFAULT_PILLAR_ROOT),
            "the default is the floor"
        );
    }

    #[test]
    fn tenant_resolution_is_fail_closed_on_unset_or_empty() {
        assert_eq!(
            resolve_tenant(Some("acme".to_string())),
            Some(TenantId("acme".to_string())),
            "a non-empty value resolves"
        );
        assert_eq!(
            resolve_tenant(Some(String::new())),
            None,
            "an empty value is fail-closed"
        );
        assert_eq!(resolve_tenant(None), None, "an unset value is fail-closed");
    }

    #[test]
    fn static_dir_resolution_is_off_on_unset_or_empty() {
        assert_eq!(
            resolve_static_dir(Some("/srv/prism".to_string())),
            Some(PathBuf::from("/srv/prism")),
            "a non-empty value points the ServeDir at the bundle"
        );
        assert_eq!(
            resolve_static_dir(Some(String::new())),
            None,
            "an empty value is default-off (API-only)"
        );
        assert_eq!(
            resolve_static_dir(None),
            None,
            "an unset value is default-off (API-only)"
        );
    }

    #[test]
    fn addr_resolution_falls_back_to_the_default_port() {
        let resolved = resolve_addr(None).expect("default addr parses");
        assert_eq!(resolved, DEFAULT_ADDR.parse().unwrap());
        let custom = resolve_addr(Some("127.0.0.1:1234".to_string())).expect("custom parses");
        assert_eq!(custom.port(), 1234);
        assert!(resolve_addr(Some("not-an-addr".to_string())).is_err());
    }

    #[test]
    fn probe_refuses_when_no_tenant_resolves() {
        let store = InMemoryMetricStore::new(Box::new(NoopRecorder));
        let result = probe(&store, None);
        assert!(result.is_err(), "fail-closed: no tenant refuses startup");
    }

    #[test]
    fn probe_succeeds_against_a_readable_store_with_a_tenant() {
        let store = InMemoryMetricStore::new(Box::new(NoopRecorder));
        let tenant = TenantId("acme".to_string());
        assert!(
            probe(&store, Some(&tenant)).is_ok(),
            "a readable store with a resolvable tenant passes the probe"
        );
    }
}
