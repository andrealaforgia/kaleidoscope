// Kaleidoscope log-query-api — composition-root logic (testable seam)
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
//! tenant resolution and the Earned-Trust probe (ADR-0047 Decision 6) are
//! unit-testable in isolation rather than buried in the binary. The thin
//! `src/main.rs` only reads the environment and calls these.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use aegis::TenantId;
use lumen::{LogStore, TimeRange};

/// Default `pillar_root` when neither the CLI arg nor the env var is
/// set. Relative to the working directory, mirroring the gateway and
/// query-api.
pub const DEFAULT_PILLAR_ROOT: &str = "kaleidoscope-data";
/// Sub-path under `pillar_root` for the lumen log store.
pub const LUMEN_SUBDIR: &str = "lumen";
/// Default listen address: a sibling to the metrics read port, distinct
/// so both read APIs can run on one host.
pub const DEFAULT_ADDR: &str = "0.0.0.0:9091";

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

/// Resolve the tenant from the `KALEIDOSCOPE_LOG_QUERY_TENANT` value.
/// Fail-closed: unset (`None`) or empty maps to `None`, which the router
/// refuses (ADR-0047 Decision 4).
pub fn resolve_tenant(env_value: Option<String>) -> Option<TenantId> {
    match env_value {
        Some(tenant) if !tenant.is_empty() => Some(TenantId(tenant)),
        _ => None,
    }
}

/// Resolve the listen address from the `KALEIDOSCOPE_LOG_QUERY_ADDR`
/// value, else the default port. Returns the parse error so the binary
/// can surface a malformed address rather than panicking.
pub fn resolve_addr(env_value: Option<String>) -> Result<SocketAddr, std::net::AddrParseError> {
    let raw = env_value.unwrap_or_else(|| DEFAULT_ADDR.to_string());
    raw.parse()
}

/// Env var carrying the read-auth issuer (ADR-0074 DD1). One of the four
/// `KALEIDOSCOPE_LOG_QUERY_AUTH_*` keys; all four must be present together
/// or wholly absent (a partial set refuses to start).
pub const AUTH_ISSUER_ENV: &str = "KALEIDOSCOPE_LOG_QUERY_AUTH_ISSUER";
/// Env var carrying the read-auth audience (ADR-0074 DD1/DD6;
/// `kaleidoscope-query` is the read audience).
pub const AUTH_AUDIENCE_ENV: &str = "KALEIDOSCOPE_LOG_QUERY_AUTH_AUDIENCE";
/// Env var carrying the PATH to the HS256 secret bytes (ADR-0074 DD1).
/// Never the bytes inline; the bytes are read at startup and moved
/// straight into the validator, never stored on a config nor logged.
pub const AUTH_SECRET_FILE_ENV: &str = "KALEIDOSCOPE_LOG_QUERY_AUTH_SECRET_FILE";
/// Env var carrying the PATH to the aegis tenant catalogue (ADR-0074 DD1).
pub const AUTH_CATALOGUE_ENV: &str = "KALEIDOSCOPE_LOG_QUERY_AUTH_CATALOGUE";

/// Synthetic tenant the Earned-Trust store-readability probe uses when
/// auth is configured (ADR-0074 DD3 arm 1). In auth mode the per-request
/// tenant comes from the validated bearer, so the env tenant is not
/// required; the probe still proves the store opens and answers, scoped
/// to this sentinel rather than to a real tenant.
const STARTUP_PROBE_TENANT: &str = "__log_query_api_startup_probe__";

/// Normalise an optional env value: unset (`None`) OR empty maps to
/// `None`, mirroring [`resolve_tenant`].
fn non_empty(value: Option<String>) -> Option<String> {
    value.filter(|v| !v.is_empty())
}

/// Resolve the OPTIONAL read-auth config (ADR-0074 DD1) into a validator.
///
/// The four `KALEIDOSCOPE_LOG_QUERY_AUTH_*` env values are passed in raw
/// (pure over its inputs, so the precedence is testable without touching
/// the process environment). The three outcomes:
///
/// - **all four absent** (unset or empty) → `Ok(None)`: the additive
///   opt-out, today's env-tenant mode unchanged (US-RAUTH-02).
/// - **all four present** AND the secret_file is readable AND the
///   catalogue loads → `Ok(Some(validator))`: auth mode, the validator
///   built once here (audience from the config, ADR-0074 DD6).
/// - **partial** (some but not all set) OR an unreadable secret_file OR
///   an unloadable catalogue → `Err(reason)`: a refuse-to-start config
///   error (ADR-0061/ADR-0068 precedent). The reason names the missing
///   key or the offending PATH, NEVER a secret byte.
pub fn resolve_read_auth(
    issuer: Option<String>,
    audience: Option<String>,
    secret_file: Option<String>,
    catalogue: Option<String>,
) -> Result<Option<Arc<aegis::Validator>>, String> {
    let issuer = non_empty(issuer);
    let audience = non_empty(audience);
    let secret_file = non_empty(secret_file);
    let catalogue = non_empty(catalogue);

    let present = [
        issuer.is_some(),
        audience.is_some(),
        secret_file.is_some(),
        catalogue.is_some(),
    ];
    let set_count = present.iter().filter(|set| **set).count();

    if set_count == 0 {
        return Ok(None);
    }
    if set_count < 4 {
        return Err(partial_reason(&present));
    }
    let validator = build_read_validator(
        &issuer.expect("issuer present when all four set"),
        &audience.expect("audience present when all four set"),
        &secret_file.expect("secret_file present when all four set"),
        &catalogue.expect("catalogue present when all four set"),
    )?;
    Ok(Some(Arc::new(validator)))
}

/// Build the refuse-to-start reason for a partial read-auth config,
/// naming every missing `KALEIDOSCOPE_LOG_QUERY_AUTH_*` key (never a
/// secret).
fn partial_reason(present: &[bool; 4]) -> String {
    let names = [
        AUTH_ISSUER_ENV,
        AUTH_AUDIENCE_ENV,
        AUTH_SECRET_FILE_ENV,
        AUTH_CATALOGUE_ENV,
    ];
    let missing: Vec<&str> = names
        .iter()
        .zip(present.iter())
        .filter(|(_, set)| !**set)
        .map(|(name, _)| *name)
        .collect();
    format!(
        "partial read-auth config: missing {} (all four KALEIDOSCOPE_LOG_QUERY_AUTH_* keys are required when read-auth is enabled)",
        missing.join(", ")
    )
}

/// Construct the aegis validator from a complete read-auth config,
/// reusing `aegis::Validator` verbatim (ADR-0074). The secret bytes are
/// read HERE and moved straight into the config; an unreadable
/// secret_file or an unloadable catalogue is a fail-closed startup
/// refusal naming the PATH (by `Display`) and the io/parse error class,
/// never a secret byte.
fn build_read_validator(
    issuer: &str,
    audience: &str,
    secret_file: &str,
    catalogue_path: &str,
) -> Result<aegis::Validator, String> {
    let hs256_key = std::fs::read(secret_file).map_err(|e| {
        format!(
            "read-auth secret_file {secret_file} is unreadable: {}",
            e.kind()
        )
    })?;
    let catalogue = aegis::load_catalogue(Path::new(catalogue_path))
        .map_err(|e| format!("read-auth catalogue {catalogue_path} could not be loaded: {e}"))?;
    Ok(aegis::Validator::new(aegis::ValidatorConfig {
        issuer: issuer.to_string(),
        audience: audience.to_string(),
        hs256_key,
        catalogue,
    }))
}

/// Choose the tenant the startup store-readability [`probe`] runs under.
///
/// When auth is configured the per-request tenant comes from the bearer
/// (ADR-0074 DD3 arm 1), so the env tenant is not required: the probe
/// runs under a synthetic sentinel tenant and the store-readability check
/// still passes with the env tenant unset. When auth is NOT configured
/// the existing env-tenant fail-closed behaviour is preserved verbatim:
/// an unset env tenant resolves to `None`, which the probe refuses.
pub fn startup_probe_tenant(auth_enabled: bool, env_tenant: Option<TenantId>) -> Option<TenantId> {
    if auth_enabled {
        return Some(TenantId(STARTUP_PROBE_TENANT.to_string()));
    }
    env_tenant
}

/// Earned-Trust probe (ADR-0047 Decision 6): assert a tenant resolves
/// AND the store answers a trivial empty-range query before the listener
/// binds. A `None` tenant is the fail-closed refusal; a store error is a
/// read refusal. An empty `Ok` result is success.
pub fn probe(
    store: &(dyn LogStore + Send + Sync),
    tenant: Option<&TenantId>,
) -> Result<(), String> {
    let Some(tenant) = tenant else {
        return Err("KALEIDOSCOPE_LOG_QUERY_TENANT is unset or empty (fail-closed)".to_string());
    };
    store
        .query(tenant, TimeRange::new(0, 0))
        .map(|_| ())
        .map_err(|e| format!("log store probe failed: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lumen::{InMemoryLogStore, LogBatch, LogStoreError, NoopRecorder, Predicate};

    // The binary's main() is a thin reader of the environment; the
    // testable precedence and the fail-closed/probe invariants live here
    // so they are mutation-killed by unit tests rather than left to the
    // composition root.

    /// A store whose `query` always fails: opened cleanly but unreadable.
    struct LyingLogStore;

    impl LogStore for LyingLogStore {
        fn ingest(
            &self,
            _tenant: &TenantId,
            _batch: LogBatch,
        ) -> Result<lumen::IngestReceipt, LogStoreError> {
            Err(LogStoreError::PersistenceFailed {
                reason: "ingest disabled".to_string(),
            })
        }

        fn query(
            &self,
            _tenant: &TenantId,
            _range: TimeRange,
        ) -> Result<Vec<lumen::LogRecord>, LogStoreError> {
            Err(LogStoreError::PersistenceFailed {
                reason: "unreadable".to_string(),
            })
        }

        fn query_with(
            &self,
            _tenant: &TenantId,
            _range: TimeRange,
            _predicate: &Predicate,
        ) -> Result<Vec<lumen::LogRecord>, LogStoreError> {
            Err(LogStoreError::PersistenceFailed {
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
        let store = InMemoryLogStore::new(Box::new(NoopRecorder));
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
            probe(&LyingLogStore, Some(&tenant)).is_err(),
            "an unreadable store refuses startup"
        );
    }

    #[test]
    fn probe_succeeds_against_a_readable_store_with_a_tenant() {
        let store = InMemoryLogStore::new(Box::new(NoopRecorder));
        let tenant = TenantId("acme".to_string());
        assert!(
            probe(&store, Some(&tenant)).is_ok(),
            "a readable store with a resolvable tenant passes the probe"
        );
    }

    // ----- read-auth config resolution (ADR-0074 DD1) -----

    /// A unique temp path for the read-auth unit tests (no fixed names so
    /// parallel runs do not collide).
    fn unique(label: &str) -> PathBuf {
        let stamp = format!(
            "{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock after epoch")
                .as_nanos()
        );
        std::env::temp_dir().join(format!("kaleidoscope-lqa-authcfg-unit-{label}-{stamp}"))
    }

    fn some(value: &str) -> Option<String> {
        Some(value.to_string())
    }

    #[test]
    fn read_auth_is_off_when_all_four_keys_are_absent_or_empty() {
        // The additive opt-out (US-RAUTH-02): wholly absent -> env-tenant mode.
        assert!(
            resolve_read_auth(None, None, None, None)
                .expect("absent config is not an error")
                .is_none(),
            "all four unset must resolve to env-tenant mode (None)"
        );
        // Empty strings are normalised to absent (mirrors resolve_tenant).
        assert!(
            resolve_read_auth(some(""), some(""), some(""), some(""))
                .expect("empty config is not an error")
                .is_none(),
            "all four empty must resolve to env-tenant mode (None)"
        );
    }

    #[test]
    fn read_auth_refuses_a_partial_config_naming_every_missing_key() {
        // issuer + audience set; secret_file + catalogue omitted.
        let reason = resolve_read_auth(
            some("acme-observability"),
            some("kaleidoscope-query"),
            None,
            None,
        )
        .expect_err("a partial config must refuse to start");
        assert!(
            reason.contains(AUTH_SECRET_FILE_ENV),
            "the refusal must name the missing secret_file key; reason: {reason}"
        );
        assert!(
            reason.contains(AUTH_CATALOGUE_ENV),
            "the refusal must name the missing catalogue key; reason: {reason}"
        );
        // The keys that ARE set must NOT be reported as missing.
        assert!(
            !reason.contains(AUTH_ISSUER_ENV) && !reason.contains(AUTH_AUDIENCE_ENV),
            "a set key must not be named as missing; reason: {reason}"
        );
    }

    #[test]
    fn read_auth_builds_a_validator_when_all_four_keys_are_present_and_readable() {
        let secret = unique("complete-secret");
        std::fs::write(&secret, b"hs256-key-bytes").expect("write secret");
        let catalogue = unique("complete-cat");
        std::fs::write(&catalogue, "[[tenants]]\nid = \"acme-prod\"\n").expect("write catalogue");

        let validator = resolve_read_auth(
            some("acme-observability"),
            some("kaleidoscope-query"),
            some(&secret.display().to_string()),
            some(&catalogue.display().to_string()),
        )
        .expect("a complete readable config builds a validator");
        assert!(
            validator.is_some(),
            "all four present + readable must build Some(validator)"
        );

        let _ = std::fs::remove_file(&secret);
        let _ = std::fs::remove_file(&catalogue);
    }

    #[test]
    fn read_auth_refuses_an_unreadable_secret_file_naming_the_path_not_the_bytes() {
        let catalogue = unique("unreadable-cat");
        std::fs::write(&catalogue, "[[tenants]]\nid = \"acme-prod\"\n").expect("write catalogue");
        let missing_secret = unique("NOTHERE-secret");
        // Deliberately do NOT create `missing_secret`.

        let reason = resolve_read_auth(
            some("acme-observability"),
            some("kaleidoscope-query"),
            some(&missing_secret.display().to_string()),
            some(&catalogue.display().to_string()),
        )
        .expect_err("an unreadable secret_file must refuse to start");
        assert!(
            reason.contains(&missing_secret.display().to_string()),
            "the refusal must name the unreadable PATH; reason: {reason}"
        );
        assert!(
            reason.contains("unreadable"),
            "the refusal must classify the failure as unreadable; reason: {reason}"
        );

        let _ = std::fs::remove_file(&catalogue);
    }

    #[test]
    fn startup_probe_tenant_is_synthetic_under_auth_and_the_env_tenant_otherwise() {
        // Auth on: a synthetic sentinel tenant regardless of the env tenant
        // (the env tenant is not required when auth is configured).
        let under_auth = startup_probe_tenant(true, None).expect("auth mode probes a tenant");
        assert_eq!(under_auth.0, STARTUP_PROBE_TENANT);
        // Auth off: the env tenant is threaded through unchanged (Some and None).
        assert_eq!(
            startup_probe_tenant(false, Some(TenantId("acme".to_string()))),
            Some(TenantId("acme".to_string())),
            "auth off threads the env tenant through unchanged"
        );
        assert_eq!(
            startup_probe_tenant(false, None),
            None,
            "auth off with no env tenant stays fail-closed (None)"
        );
    }
}
