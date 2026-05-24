// Kaleidoscope trace-query-api — composition-root logic (testable seam)
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

//! Composition-root logic, lifted into the lib seam so the fail-closed
//! tenant resolution and the Earned-Trust probe (ADR-0048 Decision 8)
//! are unit-testable in isolation rather than buried in the binary. The
//! thin `src/main.rs` only reads the environment and calls these.

use std::net::SocketAddr;
use std::path::PathBuf;

use aegis::TenantId;
use ray::{ServiceName, TimeRange, TraceStore};

/// Default `pillar_root` when neither the CLI arg nor the env var is
/// set. Relative to the working directory, mirroring the gateway and
/// the sibling query APIs.
pub const DEFAULT_PILLAR_ROOT: &str = "kaleidoscope-data";
/// Sub-path under `pillar_root` for the ray trace store.
pub const RAY_SUBDIR: &str = "ray";
/// Default listen address: a sibling to the log and metrics read ports,
/// distinct so all three read APIs can run on one host.
pub const DEFAULT_ADDR: &str = "0.0.0.0:9092";
/// The probe's stand-in service name. The Earned-Trust probe needs a
/// `&ServiceName` to call `TraceStore::query`; an unlikely sentinel
/// keeps it from accidentally matching real seeded data.
const PROBE_SERVICE: &str = "__trace_query_api_probe__";

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

/// Resolve the tenant from the `KALEIDOSCOPE_TRACE_QUERY_TENANT` value.
/// Fail-closed: unset (`None`) or empty maps to `None`, which the
/// router refuses (ADR-0048 Decision 5).
pub fn resolve_tenant(env_value: Option<String>) -> Option<TenantId> {
    match env_value {
        Some(tenant) if !tenant.is_empty() => Some(TenantId(tenant)),
        _ => None,
    }
}

/// Resolve the listen address from the `KALEIDOSCOPE_TRACE_QUERY_ADDR`
/// value, else the default port. Returns the parse error so the binary
/// can surface a malformed address rather than panicking.
pub fn resolve_addr(env_value: Option<String>) -> Result<SocketAddr, std::net::AddrParseError> {
    let raw = env_value.unwrap_or_else(|| DEFAULT_ADDR.to_string());
    raw.parse()
}

/// Earned-Trust probe (ADR-0048 Decision 8): assert a tenant resolves
/// AND the store answers a trivial empty-range query before the
/// listener binds. A `None` tenant is the fail-closed refusal; a store
/// error is a read refusal. An empty `Ok` result is success.
pub fn probe(
    store: &(dyn TraceStore + Send + Sync),
    tenant: Option<&TenantId>,
) -> Result<(), String> {
    let Some(tenant) = tenant else {
        return Err("KALEIDOSCOPE_TRACE_QUERY_TENANT is unset or empty (fail-closed)".to_string());
    };
    let service = ServiceName::new(PROBE_SERVICE);
    store
        .query(tenant, &service, TimeRange::new(0, 0))
        .map(|_| ())
        .map_err(|e| format!("trace store probe failed: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ray::{
        InMemoryTraceStore, IngestReceipt, NoopRecorder, Predicate, Span, SpanBatch, TraceId,
        TraceStoreError,
    };

    // The binary's main() is a thin reader of the environment; the
    // testable precedence and the fail-closed/probe invariants live
    // here so they are mutation-killed by unit tests rather than left
    // to the composition root.

    /// A store whose `query` always fails: opened cleanly but
    /// unreadable.
    struct LyingTraceStore;

    impl TraceStore for LyingTraceStore {
        fn ingest(
            &self,
            _tenant: &TenantId,
            _batch: SpanBatch,
        ) -> Result<IngestReceipt, TraceStoreError> {
            Err(TraceStoreError::PersistenceFailed {
                reason: "ingest disabled".to_string(),
            })
        }

        fn get_trace(
            &self,
            _tenant: &TenantId,
            _trace_id: &TraceId,
        ) -> Result<Vec<Span>, TraceStoreError> {
            Err(TraceStoreError::PersistenceFailed {
                reason: "unreadable".to_string(),
            })
        }

        fn query(
            &self,
            _tenant: &TenantId,
            _service: &ServiceName,
            _range: TimeRange,
        ) -> Result<Vec<Span>, TraceStoreError> {
            Err(TraceStoreError::PersistenceFailed {
                reason: "unreadable".to_string(),
            })
        }

        fn query_with(
            &self,
            _tenant: &TenantId,
            _service: &ServiceName,
            _range: TimeRange,
            _predicate: &Predicate,
        ) -> Result<Vec<Span>, TraceStoreError> {
            Err(TraceStoreError::PersistenceFailed {
                reason: "unreadable".to_string(),
            })
        }
    }

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
    fn addr_resolution_falls_back_to_the_default_port() {
        let resolved = resolve_addr(None).expect("default addr parses");
        assert_eq!(resolved, DEFAULT_ADDR.parse().unwrap());
        let custom = resolve_addr(Some("127.0.0.1:1234".to_string())).expect("custom parses");
        assert_eq!(custom.port(), 1234);
        assert!(resolve_addr(Some("not-an-addr".to_string())).is_err());
    }

    #[test]
    fn probe_refuses_when_no_tenant_resolves() {
        let store = InMemoryTraceStore::new(Box::new(NoopRecorder));
        assert!(
            probe(&store, None).is_err(),
            "fail-closed: no tenant refuses startup"
        );
    }

    #[test]
    fn probe_refuses_when_the_store_cannot_be_read() {
        // A store that opened but lies on query refuses startup, so no
        // half-up listener binds over an unreadable store.
        let tenant = TenantId("acme".to_string());
        assert!(
            probe(&LyingTraceStore, Some(&tenant)).is_err(),
            "an unreadable store refuses startup"
        );
    }

    #[test]
    fn probe_succeeds_against_a_readable_store_with_a_tenant() {
        let store = InMemoryTraceStore::new(Box::new(NoopRecorder));
        let tenant = TenantId("acme".to_string());
        assert!(
            probe(&store, Some(&tenant)).is_ok(),
            "a readable store with a resolvable tenant passes the probe"
        );
    }
}
