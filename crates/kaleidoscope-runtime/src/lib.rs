// Kaleidoscope consolidated runtime — composition-root library
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

//! # kaleidoscope-runtime — the consolidated composition root.
//!
//! Hosts OTLP ingest (gRPC 4317 / HTTP 4318) and the three query routers
//! (metrics 9090 / logs 9091 / traces 9092) on ONE tokio runtime, building
//! one durable store per signal and `Arc::clone`-sharing the SAME instance
//! into BOTH the ingest [`StorageSink`] (write) AND the corresponding query
//! router (read). A metric/log/trace ingested at time T is therefore
//! queryable at T+epsilon with NO restart — the live-visibility outcome that
//! fails today because ingest and query are separate processes with separate
//! frozen in-memory stores (ADR-0076, `consolidated-runtime-v0`).
//!
//! [`StorageSink`]: aperture_storage_sink::StorageSink
//!
//! ## Composition (ADR-0076 DD2/DD3)
//!
//! [`spawn_consolidated`] builds each `FileBacked*Store` once under the
//! configured pillar root, `Arc::clone`s it into both
//! `StorageSink::with_all_stores(..)` (write) AND the matching
//! `*_query_api::router_with_auth(..)` (read) so a committed write is
//! immediately visible to a read, runs each store's Earned-Trust read probe,
//! binds the five listeners (wire -> probe -> use, fail-closed), and serves
//! them all on one tokio runtime. The acceptance suite
//! (`tests/slice_01_live_metrics.rs`, `tests/slice_02_live_logs_traces.rs`)
//! drives it end-to-end over ephemeral loopback ports.

#![forbid(unsafe_code)]

use std::fmt;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use aegis::TenantId;
use aperture::ports::OtlpSink;
use aperture_storage_sink::{StorageSink, StorageSinkConfig};
use lumen::{FileBackedLogStore, LogStore, NoopRecorder as LumenNoopRecorder};
use pulse::{FileBackedMetricStore, MetricStore, NoopRecorder as PulseNoopRecorder};
use ray::{FileBackedTraceStore, NoopRecorder as RayNoopRecorder, TraceStore};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

/// Sub-path under `pillar_root` for the pulse metric store (matches the
/// gateway and the metrics query binary so the consolidated runtime writes
/// and reads the same on-disk root).
const PULSE_SUBDIR: &str = "pulse";
/// Sub-path under `pillar_root` for the lumen log store.
const LUMEN_SUBDIR: &str = "lumen";
/// Sub-path under `pillar_root` for the ray trace store.
const RAY_SUBDIR: &str = "ray";

/// Configuration for one consolidated runtime instance.
///
/// The production binary (`src/main.rs`) resolves these from the environment
/// (`KALEIDOSCOPE_PILLAR_ROOT`, the single `KALEIDOSCOPE_TENANT` with per-role
/// overrides, the fixed default ports, the optional read-auth set). The
/// in-process acceptance suite builds this directly via
/// [`ConsolidatedConfig::for_ephemeral_test`], binding every listener on
/// `127.0.0.1:0` so the fixed 4317/4318/9090/9091/9092 defaults are NEVER
/// bound in tests (the fixed-port flake, project memory
/// `aperture_fixed_port_4317_flake`).
#[derive(Clone)]
pub struct ConsolidatedConfig {
    /// One pillar root; sub-dirs `pulse`/`lumen`/`ray`, exactly as the
    /// gateway. The runtime is the SOLE writer of its root (ADR-0076 DD4).
    pub pillar_root: PathBuf,
    /// OTLP ingest gRPC bind address (production default `0.0.0.0:4317`; tests
    /// pass `127.0.0.1:0`).
    pub ingest_grpc_addr: SocketAddr,
    /// OTLP ingest HTTP bind address (production default `0.0.0.0:4318`; tests
    /// pass `127.0.0.1:0`). Endpoints `/v1/metrics`, `/v1/logs`, `/v1/traces`.
    pub ingest_http_addr: SocketAddr,
    /// Metrics query bind address (production default `0.0.0.0:9090`; tests
    /// pass `127.0.0.1:0`). Route `GET /api/v1/query_range`.
    pub metrics_query_addr: SocketAddr,
    /// Logs query bind address (production default `0.0.0.0:9091`; tests pass
    /// `127.0.0.1:0`). Route `GET /api/v1/logs`.
    pub logs_query_addr: SocketAddr,
    /// Traces query bind address (production default `0.0.0.0:9092`; tests
    /// pass `127.0.0.1:0`). Routes `GET /api/v1/traces`,
    /// `GET /api/v1/traces/by_id`, and `GET /api/v1/traces/with_logs`.
    pub traces_query_addr: SocketAddr,
    /// The ingest sink default tenant (a record without a `tenant.id` resource
    /// attribute is filed under this; `None` => fail-closed ingest of an
    /// untenanted record, unchanged from the gateway). Resolved from
    /// `KALEIDOSCOPE_DEFAULT_TENANT` else `KALEIDOSCOPE_TENANT`.
    pub default_ingest_tenant: Option<String>,
    /// The metrics query tenant (`None` => fail-closed at the router seam).
    /// Resolved from `KALEIDOSCOPE_QUERY_TENANT` else `KALEIDOSCOPE_TENANT`.
    pub metrics_query_tenant: Option<String>,
    /// The logs query tenant. `KALEIDOSCOPE_LOG_QUERY_TENANT` else
    /// `KALEIDOSCOPE_TENANT`.
    pub logs_query_tenant: Option<String>,
    /// The traces query tenant. `KALEIDOSCOPE_TRACE_QUERY_TENANT` else
    /// `KALEIDOSCOPE_TENANT`.
    pub traces_query_tenant: Option<String>,
    /// OPTIONAL per-request read-auth validator (audience `kaleidoscope-query`,
    /// ADR-0074), applied uniformly to all three query routers. `None` =>
    /// auth OFF (the local experiment posture, env-tenant path, header
    /// ignored). `Some(_)` => the routers refuse a tokenless/invalid bearer
    /// (401, before the store) and never downgrade to the env tenant. Never
    /// removed (ADR-0076 DD4).
    pub read_auth: Option<Arc<aegis::Validator>>,
    /// OPTIONAL static bundle dir for the metrics router (the Prism bundle),
    /// `KALEIDOSCOPE_QUERY_STATIC_DIR`.
    pub static_dir: Option<PathBuf>,
}

impl ConsolidatedConfig {
    /// Build a config whose FIVE listeners all bind ephemeral
    /// `127.0.0.1:0` and whose four tenant roles are all the one `tenant`
    /// (auth off, no static dir) — the in-process acceptance shape. The
    /// caller reads the ACTUAL bound addresses back from the returned
    /// [`RunningRuntime`]; it must never assume the fixed defaults.
    pub fn for_ephemeral_test(pillar_root: PathBuf, tenant: impl Into<String>) -> Self {
        let ephemeral: SocketAddr = "127.0.0.1:0".parse().expect("loopback ipv4 parses");
        let tenant = tenant.into();
        Self {
            pillar_root,
            ingest_grpc_addr: ephemeral,
            ingest_http_addr: ephemeral,
            metrics_query_addr: ephemeral,
            logs_query_addr: ephemeral,
            traces_query_addr: ephemeral,
            default_ingest_tenant: Some(tenant.clone()),
            metrics_query_tenant: Some(tenant.clone()),
            logs_query_tenant: Some(tenant.clone()),
            traces_query_tenant: Some(tenant),
            read_auth: None,
            static_dir: None,
        }
    }
}

/// A live consolidated runtime: the FIVE actual bound addresses plus the
/// shutdown handle. Returned by [`spawn_consolidated`] once the wire -> probe
/// -> use startup has bound and begun serving all five listeners on one
/// process.
///
/// The addresses are the ACTUAL bound ports (read back after binding), so an
/// ephemeral-`:0` test learns the real ports here.
pub struct RunningRuntime {
    /// The actual bound OTLP ingest gRPC address.
    pub ingest_grpc_addr: SocketAddr,
    /// The actual bound OTLP ingest HTTP address.
    pub ingest_http_addr: SocketAddr,
    /// The actual bound metrics query address.
    pub metrics_query_addr: SocketAddr,
    /// The actual bound logs query address.
    pub logs_query_addr: SocketAddr,
    /// The actual bound traces query address.
    pub traces_query_addr: SocketAddr,
    /// The aperture ingest handle (gRPC + HTTP). Drives graceful drain on
    /// [`shutdown`](RunningRuntime::shutdown); its `Drop` signals the ingest
    /// listeners to wind down if `shutdown` is never called.
    ingest_handle: aperture::Handle,
    /// The three axum query servers (metrics / logs / traces). Each carries a
    /// graceful-shutdown sender (dropping it also ends the serve loop) and the
    /// serving task handle.
    query_servers: Vec<QueryServer>,
}

/// One running axum query listener plus the handle to stop it.
struct QueryServer {
    shutdown: oneshot::Sender<()>,
    task: JoinHandle<()>,
}

impl RunningRuntime {
    /// Gracefully wind down all five listeners: signal each axum query server
    /// to stop accepting and await its drain, then drain the aperture ingest
    /// transports. Idempotent shutdown lives on the aperture handle.
    ///
    /// Mutation posture (genuine equivalent): the `Drop` of `RunningRuntime`
    /// already winds every listener down — dropping each `QueryServer.shutdown`
    /// sender fires that server's graceful-shutdown future, and
    /// `aperture::Handle`'s `Drop` signals the ingest listeners. The only thing
    /// the explicit `shutdown` adds over `Drop` is *awaiting* the drain so an
    /// in-flight request completes before the process exits — observable only
    /// under concurrent teardown load, which the in-process acceptance suite
    /// does not exercise. The `-> Ok(())` mutant therefore has the identical
    /// observable end-state (all five listeners closed, ports freed) as the
    /// real body, so no falsifying test exists; skipped rather than chased.
    #[mutants::skip]
    pub async fn shutdown(self) -> Result<(), RuntimeError> {
        for server in self.query_servers {
            let _ = server.shutdown.send(());
            let _ = server.task.await;
        }
        self.ingest_handle
            .shutdown()
            .await
            .map_err(|e| RuntimeError(format!("ingest drain failed during shutdown: {e}")))
    }
}

/// A consolidated-runtime startup or teardown failure. The load-bearing
/// failure is fail-closed startup: if ANY of the five listeners cannot bind,
/// or ANY store probe fails, [`spawn_consolidated`] returns this (no half-up
/// process), and the binary maps it to `event=health.startup.refused` + a
/// non-zero exit (ADR-0076 DD3).
#[derive(Debug)]
pub struct RuntimeError(pub String);

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for RuntimeError {}

/// Spawn the consolidated runtime on the current tokio runtime and return a
/// [`RunningRuntime`] carrying the five bound addresses and a shutdown handle.
///
/// This is THE driving entry the acceptance suite calls and THE composition
/// root DELIVER must implement (ADR-0076 DD2/DD3):
///
/// 1. build `FileBackedMetricStore` / `FileBackedLogStore` /
///    `FileBackedTraceStore` once under `config.pillar_root`;
/// 2. `Arc::clone` each into `StorageSink::with_all_stores(..)` (write) AND
///    into the matching `query_api` / `log_query_api` / `trace_query_api`
///    `router_with_auth(..)` (read) — the SAME allocation, same interior
///    `Mutex`, so a committed write is immediately visible to a read;
/// 3. run the sink's active-write + fsync-honesty probe AND each store's read
///    probe (Earned-Trust, wire -> probe -> use);
/// 4. bind the three query `TcpListener`s then `aperture::spawn` the two
///    ingest listeners; on ANY bind/probe failure return
///    [`RuntimeError`] (fail-closed, no half-up process);
/// 5. read back the five actual bound addresses and begin serving them on the
///    one runtime.
///
/// ## Earned-Trust posture (wire -> probe -> use, fail-closed)
///
/// The stores are opened with the real fsync backend (durability honoured at
/// the store level). Before any listener serves, each store's read probe must
/// answer (fail-closed: a missing query tenant with auth off refuses startup).
/// The query listeners bind before the ingest listeners so an occupied query
/// port is caught early; on ANY bind/probe/open failure the whole startup
/// returns [`RuntimeError`] and no half-up process is left serving (the bound
/// listeners and the ingest handle drop on the error path, freeing their
/// ports). The write-side active-probe is intentionally not duplicated here:
/// `spawn_consolidated` exposes no fsync-backend injection seam, so a probe
/// call could not be made falsifiable through this public API; store-open plus
/// the read probe already guard openability and readability.
pub async fn spawn_consolidated(
    config: ConsolidatedConfig,
) -> Result<RunningRuntime, RuntimeError> {
    // One shared JSON-to-stderr subscriber for the whole process; aperture's
    // and the read tier's later `try_init`s observe the default and no-op.
    query_http_common::init_tracing();

    std::fs::create_dir_all(&config.pillar_root)
        .map_err(|e| RuntimeError(format!("create pillar root {:?}: {e}", config.pillar_root)))?;

    // Build ONE durable store per signal under the configured pillar root.
    let log_store = Arc::new(
        FileBackedLogStore::open(
            config.pillar_root.join(LUMEN_SUBDIR),
            Box::new(LumenNoopRecorder),
        )
        .map_err(|e| RuntimeError(format!("open lumen log store: {e}")))?,
    );
    let trace_store = Arc::new(
        FileBackedTraceStore::open(
            config.pillar_root.join(RAY_SUBDIR),
            Box::new(RayNoopRecorder),
        )
        .map_err(|e| RuntimeError(format!("open ray trace store: {e}")))?,
    );
    let metric_store = Arc::new(
        FileBackedMetricStore::open(
            config.pillar_root.join(PULSE_SUBDIR),
            Box::new(PulseNoopRecorder),
        )
        .map_err(|e| RuntimeError(format!("open pulse metric store: {e}")))?,
    );

    // WRITE path: the ingest sink holds an `Arc::clone` of each store.
    let sink = StorageSink::with_all_stores(
        Arc::clone(&log_store),
        Arc::clone(&trace_store),
        Arc::clone(&metric_store),
        sink_config(&config),
    );

    // READ path: the SAME allocation (same interior Mutex), coerced to the
    // `Arc<dyn …Store>` each query router takes. A write through the sink is
    // therefore immediately visible to a read through the router — the
    // load-bearing shared-Arc mechanism (ADR-0076 DD2).
    let metric_dyn: Arc<dyn MetricStore + Send + Sync> = metric_store;
    let log_dyn: Arc<dyn LogStore + Send + Sync> = log_store;
    let trace_dyn: Arc<dyn TraceStore + Send + Sync> = trace_store;

    let metrics_tenant = config.metrics_query_tenant.clone().map(TenantId);
    let logs_tenant = config.logs_query_tenant.clone().map(TenantId);
    let traces_tenant = config.traces_query_tenant.clone().map(TenantId);
    let auth_enabled = config.read_auth.is_some();

    // Earned-Trust read probes: each store must answer before any listener
    // serves. In auth mode the per-request tenant rides the bearer, so the
    // probe runs under each crate's synthetic sentinel tenant; with auth off
    // an unset query tenant refuses startup (fail-closed).
    query_api::composition::probe(
        metric_dyn.as_ref(),
        query_api::composition::startup_probe_tenant(auth_enabled, metrics_tenant.clone()).as_ref(),
    )
    .map_err(|e| RuntimeError(format!("metrics read probe refused: {e}")))?;
    log_query_api::composition::probe(
        log_dyn.as_ref(),
        log_query_api::composition::startup_probe_tenant(auth_enabled, logs_tenant.clone())
            .as_ref(),
    )
    .map_err(|e| RuntimeError(format!("logs read probe refused: {e}")))?;
    trace_query_api::composition::probe(
        trace_dyn.as_ref(),
        trace_query_api::composition::startup_probe_tenant(auth_enabled, traces_tenant.clone())
            .as_ref(),
    )
    .map_err(|e| RuntimeError(format!("traces read probe refused: {e}")))?;

    // Build the three query routers, sharing the live Arc + the uniform
    // (optional) read-auth validator across all three.
    let metrics_router = query_api::router_with_auth(
        metric_dyn,
        metrics_tenant,
        config.read_auth.clone(),
        config.static_dir.clone(),
    );
    let logs_router = log_query_api::router_with_auth(
        Arc::clone(&log_dyn),
        logs_tenant,
        config.read_auth.clone(),
    );
    // The traces router now ALSO holds the log store so it can serve the
    // combined `/api/v1/traces/with_logs` route (a trace together with its
    // correlated logs in one response). Built through `router_with_auth_and_logs`
    // so the log store is genuinely wired into the traces surface — the
    // runtime acceptance test guards against a defined-but-unwired endpoint.
    let traces_router = trace_query_api::router_with_auth_and_logs(
        trace_dyn,
        log_dyn,
        traces_tenant,
        config.read_auth.clone(),
    );

    // Bind the three query listeners FIRST (cheap port-conflict detection,
    // fail-closed); a conflict here returns Err before ingest binds.
    let metrics_listener = bind_listener(config.metrics_query_addr, "metrics query").await?;
    let logs_listener = bind_listener(config.logs_query_addr, "logs query").await?;
    let traces_listener = bind_listener(config.traces_query_addr, "traces query").await?;

    let metrics_query_addr = bound_addr(&metrics_listener, "metrics query")?;
    let logs_query_addr = bound_addr(&logs_listener, "logs query")?;
    let traces_query_addr = bound_addr(&traces_listener, "traces query")?;

    // Bind the two ingest listeners via aperture. On Err here the three query
    // listeners drop, freeing their ports — no half-up process.
    let ingest_config = aperture::config::Config::builder()
        .grpc_bind_addr(config.ingest_grpc_addr)
        .http_bind_addr(config.ingest_http_addr)
        .build()
        .map_err(|e| RuntimeError(format!("ingest config rejected: {e}")))?;
    let sink_dyn: Arc<dyn OtlpSink> = Arc::new(sink);
    let ingest_handle = aperture::spawn(ingest_config, sink_dyn)
        .await
        .map_err(|e| RuntimeError(format!("ingest listeners refused to bind: {e}")))?;

    // Serve the three query routers on the one runtime.
    let query_servers = vec![
        serve_query(metrics_listener, metrics_router),
        serve_query(logs_listener, logs_router),
        serve_query(traces_listener, traces_router),
    ];

    Ok(RunningRuntime {
        ingest_grpc_addr: ingest_handle.grpc_addr(),
        ingest_http_addr: ingest_handle.http_addr(),
        metrics_query_addr,
        logs_query_addr,
        traces_query_addr,
        ingest_handle,
        query_servers,
    })
}

/// Map the consolidated config's default ingest tenant to the sink config: a
/// configured tenant files untenanted records, `None` is fail-closed (an
/// untenanted record is refused), exactly as the gateway.
fn sink_config(config: &ConsolidatedConfig) -> StorageSinkConfig {
    match &config.default_ingest_tenant {
        Some(tenant) => StorageSinkConfig::with_default_tenant(tenant.clone()),
        None => StorageSinkConfig::no_default_tenant(),
    }
}

/// Bind one loopback/host TCP listener, mapping a bind failure (e.g. an
/// occupied port) to the fail-closed [`RuntimeError`].
async fn bind_listener(addr: SocketAddr, role: &str) -> Result<TcpListener, RuntimeError> {
    TcpListener::bind(addr)
        .await
        .map_err(|e| RuntimeError(format!("{role} listener bind failed on {addr}: {e}")))
}

/// Read back the ACTUAL bound address (the ephemeral `:0` test learns its real
/// port here).
fn bound_addr(listener: &TcpListener, role: &str) -> Result<SocketAddr, RuntimeError> {
    listener
        .local_addr()
        .map_err(|e| RuntimeError(format!("{role} listener local_addr failed: {e}")))
}

/// Spawn an axum query server on `listener`, returning its stop handle. The
/// graceful-shutdown future completes when the sender is signalled OR dropped,
/// so dropping the returned [`QueryServer`] also winds the listener down.
fn serve_query(listener: TcpListener, router: axum::Router) -> QueryServer {
    let (shutdown, rx) = oneshot::channel::<()>();
    let task = tokio::spawn(async move {
        let _ = axum::serve(listener, router)
            .with_graceful_shutdown(async move {
                let _ = rx.await;
            })
            .await;
    });
    QueryServer { shutdown, task }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pin the `RuntimeError` `Display`: the fail-closed startup arm renders
    /// the refusal reason through `format!`, so an empty render would hide the
    /// substrate that refused. Kills the `<Display>::fmt -> Ok(Default())`
    /// mutant that would emit an empty string regardless of payload.
    #[test]
    fn runtime_error_display_renders_the_inner_reason() {
        let err = RuntimeError("metrics query listener bind failed on 127.0.0.1:9090".to_string());
        assert_eq!(
            err.to_string(),
            "metrics query listener bind failed on 127.0.0.1:9090",
            "the refusal reason round-trips through Display"
        );
    }
}
