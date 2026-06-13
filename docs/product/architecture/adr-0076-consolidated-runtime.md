# ADR-0076 — Consolidated runtime: a new additive composition-root binary that hosts OTLP ingest and the three query routers on one tokio runtime over one shared `Arc<Store>` per signal

- **Status**: Accepted
- **Date**: 2026-06-13
- **Author**: `nw-solution-architect` (Morgan)
- **Feature**: `consolidated-runtime-v0` (item C1 — the spine of the consolidation roadmap)
- **Mode of operation**: PROPOSE (Decision 0 scope = APPLICATION; Decision 1 = PROPOSE). Autonomous overnight run.
- **Supersedes**: none
- **Superseded by**: none
- **Related**: ADR-0041 (`aperture-storage-sink` translation + tenancy — the write side reused here), ADR-0042 / ADR-0047 / ADR-0048 (the three query-API contracts + their injected-store seams — the read side reused here), ADR-0049 / ADR-0060 (Earned-Trust fsync-honesty + store-fsync durability — the probes the runtime must run before serving), ADR-0066 (aperture serve-loop error surfacing — the fail-on-serve-death posture the runtime mirrors across all five listeners), ADR-0068 (`aegis-ingest-auth` — the optional ingest auth that must not regress), ADR-0074 (`read-path-query-api-auth` — the optional per-request read auth that must not regress), ADR-0015 / ADR-0009 (single tracing-subscriber install invariant).

> **MODEL FORK — ANDREA MAY VETO (flag carried forward verbatim from DISCUSS W1).**
> This ADR is designed to the **SINGLE-PROCESS** model, DECIDED by Luna in DISCUSS and proceeded on per decide-don't-ask: one process builds ONE store per signal, wraps it in `Arc`, and hands the SAME instance to BOTH the ingest sink AND the query router, so a write is immediately visible to a read.
> **If Andrea prefers the DISTRIBUTED / multi-process shape instead**, the load-bearing work reshapes: the mechanism becomes a **WAL-watch / reload adapter** on the standalone query stores (so a separate query process reflects post-startup appends), NOT a shared in-process store. The user-visible outcome (send a metric, immediately query it back, no restart) and every acceptance scenario stay identical — only the mechanism changes. The "Alternatives considered" section below records the distributed-with-WAL-watch alternative as the one a veto selects. **Proceeding on single-process**; one word from Andrea flips the mechanism.

## Context

Kaleidoscope today is a set of well-built, individually runnable binaries that do not yet run together as one live experimentable system (`docs/analysis/consolidation-state-2026-06.md` §2-§4). The single load-bearing gap (assessment §4): **ingest and query are separate OS processes sharing only a filesystem path, and each file-backed query store loads its snapshot + WAL into an in-memory map exactly once at `open()` and never re-reads.** A query API started before telemetry arrives returns empty until it is restarted. The natural experiment loop — bring up the stack, send a metric, look — fails by construction. C1 fixes precisely this.

Verified by reading `main` on 2026-06-13:

- `kaleidoscope-gateway` (the ingest binary) already builds `Arc::new(FileBackedMetricStore::open(..))` for each of pulse/lumen/ray and `Arc::clone`s each into `StorageSink::with_all_stores(..)` (`crates/kaleidoscope-gateway/src/main.rs:76-96`). This is the write side.
- The three query routers ALREADY accept an injected store — this is the reuse seam:
  - `query_api::router(store: Arc<dyn MetricStore + Send + Sync>, tenant, static_dir)` and `router_with_auth(.., auth, ..)` (`crates/query-api/src/lib.rs:122,152`).
  - `log_query_api::router(store: Arc<dyn LogStore + Send + Sync>, tenant)` / `router_with_auth` (`crates/log-query-api/src/lib.rs:95,104`).
  - `trace_query_api::router(store: Arc<dyn TraceStore + Send + Sync>, tenant)` / `router_with_auth` (`crates/trace-query-api/src/lib.rs:100,110`).
- `StorageSink::with_all_stores(Arc<FileBackedLogStore>, Arc<FileBackedTraceStore>, Arc<FileBackedMetricStore>, config)` takes the concrete store Arcs (`crates/aperture-storage-sink/src/lib.rs:181`).
- The store trait methods take `&self` and the concrete file-backed adapters carry interior `Mutex<Inner>`: `MetricStore::ingest(&self,..)/query(&self,..)` (`crates/pulse/src/store.rs:72-99`), `FileBackedMetricStore { .., state: Mutex<Inner> }` (`crates/pulse/src/file_backed.rs:81`). lumen and ray follow the same shape (`crates/lumen/src/store.rs:79,84`; `crates/lumen/src/file_backed.rs:217,236`; `crates/ray/src/store.rs:67`).
- `aperture::spawn(config, sink) -> Handle` binds gRPC `:4317` and HTTP `:4318` on the **current** tokio runtime and returns after both listeners are bound (`crates/aperture/src/lib.rs:381`; `crates/aperture/src/compose.rs:121-273`). `Config::builder()` exposes `.grpc_bind_addr(..)/.http_bind_addr(..)` setters (defaults 4317/4318), so ephemeral ports are configurable for tests.
- Each query binary binds its own axum listener via `TcpListener::bind(addr)` + `axum::serve` after a wire→probe→use Earned-Trust step, with addresses from `KALEIDOSCOPE_QUERY_ADDR` (9090) / `KALEIDOSCOPE_LOG_QUERY_ADDR` (9091) / `KALEIDOSCOPE_TRACE_QUERY_ADDR` (9092) (`crates/query-api/src/main.rs`; `crates/{log,trace}-query-api/src/composition.rs::resolve_addr`).

The whole feature is therefore **composition, not new product surface**: build each store once, share the one `Arc` between sink and router, host all five listeners on one runtime.

## What DESIGN must lock (this ADR resolves DD1–DD5)

1. **DD1** — new binary vs extend the gateway.
2. **DD2** — the precise shared-`Arc` composition (the Arc flow), and confirmation that no store concurrency change is required.
3. **DD3** — the runtime + port layout (five listeners on one runtime; one tracing install; fail-closed startup).
4. **DD4** — the minimal-friction single-tenant local config posture, without regressing the optional ingest-auth / read-auth seams.
5. **DD5** — the additive-binaries constraint.

## Decision

Introduce a **new, additive composition-root crate `crates/kaleidoscope-runtime`** exposing one binary (bin name `kaleidoscope`) that, on one tokio runtime, builds one durable store per signal, `Arc::clone`s the **same** instance into both the ingest `StorageSink` and the corresponding query router, binds the OTLP ingest ports (4317/4318) and the three query ports (9090/9091/9092), runs every Earned-Trust probe before serving, and serves them all from one process. The four existing binaries (`kaleidoscope-gateway`, `query-api`, `log-query-api`, `trace-query-api`) are untouched and continue to build and run.

### DD1 — a NEW binary in a NEW crate, not an extension of the gateway

**Decision: new crate `crates/kaleidoscope-runtime`, bin `kaleidoscope`.**

The consolidated runtime needs the union of the gateway's dependencies (`aperture`, `aperture-storage-sink`, `pulse`, `lumen`, `ray`) **plus** the three query-API library crates (`query-api`, `log-query-api`, `trace-query-api`) and `query-http-common`. Putting a second `[[bin]]` inside `kaleidoscope-gateway` would force the gateway crate's `Cargo.toml` to depend on the three query crates even though the pure-ingest `kaleidoscope-gateway` binary never uses them — coupling the ingest binary's crate to the read tier and eroding the "gateway is pure ingest" boundary the distributed future relies on.

A new composition-root-only crate keeps every existing composition root byte-for-byte, adds exactly one new entry point, contains **no new domain logic, no new store, no new port** — only wiring — and depends solely on the existing `pub` library seams. It is the smallest additive surface that satisfies the constraint.

The bin is named `kaleidoscope` (the product's one-command entry) so the run story (C2) and getting-started doc (C4) can wrap a single friendly command; the crate is `kaleidoscope-runtime` to disambiguate from `kaleidoscope-cli` and `kaleidoscope-gateway`.

### DD2 — the shared-`Arc` composition (the precise Arc flow); no concurrency change required

The composition root builds each store **once** and shares the **same allocation** with both consumers (illustrative shape; the crafter owns the exact code):

```text
let metric_store: Arc<FileBackedMetricStore> = Arc::new(FileBackedMetricStore::open(pulse_path, ..)?);
let log_store:    Arc<FileBackedLogStore>    = Arc::new(FileBackedLogStore::open(lumen_path, ..)?);
let trace_store:  Arc<FileBackedTraceStore>  = Arc::new(FileBackedTraceStore::open(ray_path, ..)?);

// WRITE side — with_all_stores takes the concrete Arcs (verbatim reuse of the gateway wiring)
let sink = StorageSink::with_all_stores(
    Arc::clone(&log_store), Arc::clone(&trace_store), Arc::clone(&metric_store), sink_config);

// READ side — the SAME Arc, coerced to the trait object each router accepts
let metrics_router = query_api::router_with_auth(
    Arc::clone(&metric_store) as Arc<dyn MetricStore + Send + Sync>, metrics_tenant, metrics_auth, static_dir);
let logs_router = log_query_api::router_with_auth(
    Arc::clone(&log_store)   as Arc<dyn LogStore + Send + Sync>,    logs_tenant,   logs_auth);
let traces_router = trace_query_api::router_with_auth(
    Arc::clone(&trace_store) as Arc<dyn TraceStore + Send + Sync>,  traces_tenant, traces_auth);
```

**The load-bearing invariant**: `Arc::clone` shares the same heap allocation and the same interior `Mutex<Inner>`; coercing `Arc<FileBackedMetricStore>` to `Arc<dyn MetricStore + Send + Sync>` attaches a vtable, it does not copy the store. So the sink's write and the router's read go through the **same Mutex on the same state**. This is the entire mechanism by which a metric ingested at time T is queryable at T+epsilon with no restart.

**No store concurrency change is required, confirmed by reading** (`main`, 2026-06-13):

- the trait methods take `&self` (`pulse/src/store.rs:72-99`, `lumen/src/store.rs:79-84`, `ray/src/store.rs:67`);
- each file-backed adapter serialises all state behind one `Mutex<Inner>` (`pulse/src/file_backed.rs:81`);
- `ingest` acquires `self.state.lock()` (`pulse/src/file_backed.rs:324`) and `query` acquires the same lock (`:355`); a write commits and releases the lock before any subsequent read can acquire it, so the write is fully visible to the read.

**Honest caveat flagged to DELIVER / DEVOPS (a latency characteristic, NOT a correctness issue and NOT requiring a store change for C1)**: the per-record fsync runs **inside** the write lock (`append_wal` → `fsync_file`, `pulse/src/file_backed.rs:325,515`). A `query` issued concurrently with a heavy ingest batch therefore blocks until that batch's fsync completes and the lock releases. For the local single-experimenter workload and the p95 < 1 s freshness target this is a non-issue; under sustained high-throughput ingest a read could see added latency. DELIVER/DEVOPS should measure read latency under concurrent fsync-heavy ingest (the freshness KPI test is the natural home) and not pre-optimise. The live-visibility property and tenant isolation are unaffected.

### DD3 — runtime + port layout; one tracing install; fail-closed startup

- **One tokio runtime.** The bin is a single `#[tokio::main]`. Both the ingest listeners (via `aperture::spawn`, which binds on the current runtime) and the three axum query servers (`axum::serve(TcpListener, router)`) run on it.
- **One tracing subscriber.** Install exactly one JSON-to-stderr subscriber as the first statement of `main` (reusing the existing shared init — `query_http_common::init_tracing()` — which is `OnceLock` + `try_init`-guarded). aperture's own `install_subscriber()` (`compose.rs:136`) and any other tier's install then observe a default already present and no-op via `try_init`. This is the established single-install invariant (ADR-0015 / ADR-0009; the gateway already relies on exactly this idempotence at `main.rs:156`). No double-install.
- **Port layout**: ingest gRPC `:4317`, ingest HTTP `:4318` (aperture defaults via `Config::builder()`); metrics query `:9090`, logs query `:9091`, traces query `:9092` (the three existing query defaults). Each is configurable through the existing env knob (`KALEIDOSCOPE_QUERY_ADDR` / `KALEIDOSCOPE_LOG_QUERY_ADDR` / `KALEIDOSCOPE_TRACE_QUERY_ADDR`; aperture bind addrs through its config). Tests bind ephemeral `127.0.0.1:0` and sweep+retry (the fixed-port 4317/4318 flake, project memory `aperture_fixed_port_4317_flake`).
- **Fail-closed startup ("wire → probe → use", mirroring the gateway and the query mains)**: the runtime (a) builds the three stores; (b) runs the sink's active-write + fsync-honesty probe (the gateway's `probe_or_refuse`, ADR-0049) and each query store's read probe (`*_query_api::composition::probe`, ADR-0042/0047/0048); (c) binds the three query `TcpListener`s and the two ingest listeners; (d) only then serves. If **any** bind fails or **any** probe fails, the runtime emits `event=health.startup.refused` with the substrate descriptor and exits non-zero — no half-up process. To keep the half-up window minimal, the three query listeners are bound first (cheap port-conflict detection), then `aperture::spawn` binds the ingest ports; if a later step fails, the already-acquired resources are released as the process exits (and the aperture `Handle` is shut down). A post-bind serve-loop death of any of the five listeners is treated as fatal and winds the others down, mirroring aperture's ADR-0066 posture.
- **Co-hosting / shutdown**: the five serving futures plus the SIGTERM/SIGINT signal are raced with `tokio::select!`; on a signal the aperture `Handle` drains (its existing graceful drain) and the axum servers stop via graceful shutdown.

### DD4 — minimal-friction single-tenant local posture; auth seams preserved

- **One tenant knob for the experiment.** The runtime reads a single `KALEIDOSCOPE_TENANT`; when set it drives the ingest default tenant **and** all three query tenants, so the one-command experiment needs only `KALEIDOSCOPE_TENANT=acme`. The existing per-role vars still work and **override** the unified one when present (precedence: per-role var > `KALEIDOSCOPE_TENANT` > unset): `KALEIDOSCOPE_DEFAULT_TENANT` (ingest), `KALEIDOSCOPE_QUERY_TENANT` / `KALEIDOSCOPE_LOG_QUERY_TENANT` / `KALEIDOSCOPE_TRACE_QUERY_TENANT` (query). This is a composition-root config decision only; it invents no new store or router semantics.
- **One pillar root, one writer.** `KALEIDOSCOPE_PILLAR_ROOT` (or CLI arg 1), sub-dirs `pulse`/`lumen`/`ray`, exactly as the gateway. The runtime is the **sole** writer of its pillar root; it must not be co-run against a separate `kaleidoscope-gateway` on the same root (two writers would corrupt the WAL — assessment §4). Documented constraint.
- **Auth off by default, never removed.** Local posture: no ingest auth (`Config::builder().build()` default, no `jwt_auth`), no read auth (the `KALEIDOSCOPE_*_QUERY_AUTH_*` set wholly absent → env-tenant mode). Both the optional fail-closed ingest auth (ADR-0068) and the optional fail-closed per-request read auth (ADR-0074, `router_with_auth` with a `Some(validator)`) remain available and behave identically when configured. A partial read-auth config still refuses to start (exit 2), unchanged.
- `KALEIDOSCOPE_QUERY_STATIC_DIR` continues to be honoured for the metrics router (the Prism bundle), optional.

### DD5 — additive-binaries constraint

C1 adds an entry point; it removes nothing. `kaleidoscope-gateway`, `query-api`, `log-query-api`, and `trace-query-api` remain valid binaries (the basis of the distributed future and of the existing Dockerfiles). Enforced by the fact that the new crate touches none of their sources and by `cargo build --workspace` plus the existing integration suite staying green.

## Alternatives considered

### A1 — Extend `kaleidoscope-gateway` with a second binary (rejected)

Add a `[[bin]]` to the gateway crate that also hosts the three query routers. **Rejected**: forces the gateway crate to depend on the three query crates even for the pure-ingest binary, coupling the ingest crate to the read tier and weakening the boundary the distributed future depends on. The new-crate option achieves the same one-composition-root outcome with a strictly smaller blast radius. (One genuine merit — a single crate — does not outweigh the dependency-graph pollution.)

### A2 — Distributed / multi-process with a WAL-watch (reload) adapter on the query stores (the VETO alternative)

Keep ingest and query as separate processes and teach each file-backed query store to re-read or watch its WAL as it grows, so a separate query process reflects post-startup appends. **This is the shape Andrea's veto (W1) would select.** Rejected for C1 because: it buys horizontal distribution Kaleidoscope cannot yet use, at the cost of a new cross-process freshness mechanism (file-watch / re-read, inotify portability, snapshot/WAL race windows, a cross-process file-lock to make two-writer safe) — strictly more code and more failure modes than sharing one `Arc`, for a *scaling* benefit that is not an *experimentation* need today. It is a real future (a scaling concern), not the spine. If selected, the user-visible outcome and every acceptance scenario are unchanged; only the mechanism behind them moves from a shared `Arc` to a WAL-watch adapter, and Milestone 1 reshapes around that adapter.

### A3 — A long-lived in-process broker / message bus between a write actor and a read actor (rejected)

Run ingest and query as two in-process tasks communicating via a channel, with the store owned by one actor. **Rejected as resume-driven for this problem**: it reintroduces, inside one process, exactly the freshness-asymmetry C1 exists to remove (the reader would see only what the writer-actor has drained), adds an actor framework and back-pressure semantics, and is strictly more complex than the shared `Arc` the store's interior `Mutex` already makes safe. The store is already concurrency-safe for shared read/write; an extra indirection earns nothing.

## Consequences

### Positive

- The send-then-see loop works by construction for all three signals, in one process, with no restart — the north-star outcome (`outcome-kpis.md`).
- Maximal reuse: zero changes to the stores, the sink, the query routers, the aperture engine, or aegis. The only new code is one composition-root crate (no domain logic).
- The four existing binaries and the distributed future are preserved intact (additive).
- Fail-closed startup and the single tracing install are inherited from established, already-enforced patterns; no new Earned-Trust contract is introduced because no new adapter is introduced — the runtime's Earned-Trust obligation is discharged by **running every reused adapter's existing probe before any listener serves** (itself an acceptance criterion, US-01/US-05).
- Tenant isolation, optional ingest auth, optional read auth, and per-record fsync durability are unchanged (no regression).

### Negative / trade-offs

- Single-writer by construction: the consolidated runtime must own its pillar root; co-running it with a separate gateway on the same root would corrupt the WAL (documented constraint, mitigated by single-process).
- No horizontal distribution: the consolidated runtime is one process; scaling reads independently of writes is explicitly out of scope for C1 (the A2 future, behind the veto).
- Read latency can be coupled to ingest fsync under sustained high-throughput ingest (DD2 caveat); a non-issue for the experiment workload, flagged to DELIVER/DEVOPS to measure rather than pre-optimise.

## Enforcement (principle 11)

- **Additive constraint**: `cargo build --workspace` builds all five binaries; the existing integration suite stays green. CI is feedback, not a gate (project memory `kaleidoscope_pure_trunk_based`), but a red here is the signal.
- **The shared-`Arc` invariant** (sink and router hold the same instance) is enforced behaviourally by the US-01 single-process ingest-then-query acceptance test: if they ever held different instances the write would not be visible and the test reds. This is the load-bearing guard and it is a test, not a convention.
- **Fail-closed startup** is enforced by the US-01/US-05 acceptance scenarios (all five ports bound on one process; a probe/bind failure refuses to start) and by the reused adapters' existing structural + behavioural probe-gold runners (aperture's `probe_gold_runner`, the stores' fsync probes).
- **Mutation testing** (per-feature, 100% kill gate, ADR-0005) applies to the new crate's composition logic (the tenant-precedence resolution and the bind/probe ordering are the branch-bearing parts).

## External integrations (principle 10)

**None new.** The only external substrate is the local filesystem (covered by the reused fsync-honesty probe, ADR-0049) and the OTLP clients an experimenter points at the ingest ports (their own curl/SDK, not a third-party production API). No consumer-driven contract test is recommended for C1.
