# Wave Decisions — `consolidated-runtime-v0` (DESIGN)

> **Wave**: DESIGN (`nw-solution-architect` / Morgan).
> **Date**: 2026-06-13. Autonomous overnight run.
> **Mode**: Decision 0 scope = APPLICATION; Decision 1 = PROPOSE.
> **Feature**: `consolidated-runtime-v0` — item C1 (the spine) of the consolidation roadmap.
> **Decision record**: **ADR-0076** (`docs/product/architecture/adr-0076-consolidated-runtime.md`).
> **Brief section**: `docs/product/architecture/brief.md` → `## Application Architecture — consolidated-runtime-v0` (C4 Container diagram + send→immediately-query sequence diagram).
> **Grounding read on `main`, 2026-06-13** (read-only; no Bash available, every claim below is from reading source or stated as a DELIVER must-verify).

> ### ANDREA-VETO FLAG (carried forward verbatim from DISCUSS W1 — single point of reversal)
>
> The feature is designed to the **single-process shared-`Arc<Store>`** model. **If Andrea prefers the distributed / multi-process WAL-watch shape, the mechanism reshapes** (a WAL-watch / reload adapter on the standalone query stores instead of a shared in-process store); the user-visible outcome and every acceptance scenario stay identical. Recorded as alternative **A2** in ADR-0076. Proceeding on single-process per the roadmap and decide-don't-ask.

---

## DESIGN decisions (DD1–DD5 — full rationale in ADR-0076)

| # | Decision | Choice | One-line rationale |
|---|----------|--------|--------------------|
| DD1 | New binary vs extend gateway | **New crate `crates/kaleidoscope-runtime`, bin `kaleidoscope`** | Smallest additive surface; keeps the gateway pure-ingest; extending it would force the ingest crate to depend on the three query crates. |
| DD2 | Shared-`Arc` composition | Build each store once; `Arc::clone` the **same** instance into the sink (concrete Arc) and the router (coerced `Arc<dyn Store>`) | Same allocation, same interior `Mutex` → a write is immediately visible to a read. No concurrency change (confirmed by reading). |
| DD3 | Runtime + port layout | One tokio runtime; one tracing install; five listeners (4317/4318/9090/9091/9092); wire→probe→use fail-closed startup | Reuses `aperture::spawn` (binds on current runtime) + three `axum::serve`; idempotent `try_init` keeps one subscriber. |
| DD4 | Local config posture | One `KALEIDOSCOPE_TENANT` drives all four roles (per-role vars override); one pillar root, one writer; auth off but never removed | One-command experiment; no regression of ingest-auth (ADR-0068) or read-auth (ADR-0074). |
| DD5 | Additive binaries | The four existing binaries are untouched and still build/run | Roadmap additive requirement; basis of the distributed future + the Dockerfiles. |

### Confirmed by reading (no-concurrency-change)

`MetricStore::ingest(&self,..)/query(&self,..)` (`pulse/src/store.rs:72-99`); `FileBackedMetricStore { .., state: Mutex<Inner> }` (`pulse/src/file_backed.rs:81`); `ingest` locks `self.state` at `file_backed.rs:324`, `query` locks the same at `:355`. A write commits + releases the lock before a subsequent read acquires it, so the write is visible. lumen/ray follow the same `&self` + `Mutex` shape (`lumen/src/store.rs:79-84`, `lumen/src/file_backed.rs:217,236`, `ray/src/store.rs:67`). **Therefore NO store concurrency change is required for correctness — confirmed by reading, not assumed.**

### DELIVER / DEVOPS must-verify (honest caveat — a latency characteristic, not correctness)

The per-record fsync runs **inside** the ingest write lock (`append_wal` → `fsync_file`, `pulse/src/file_backed.rs:325,515`). A query concurrent with a heavy ingest batch blocks until that fsync completes. **Non-issue for the local experiment + p95 < 1 s target; flagged for DELIVER/DEVOPS to MEASURE (the freshness KPI test) under concurrent fsync-heavy ingest, not to pre-optimise.** It does not threaten the live-visibility property, tenant isolation, or durability, and needs no store change for C1.

---

## MANDATORY Reuse Analysis

Everything load-bearing is REUSE. The single CREATE-NEW is the composition-root crate, justified below.

| Component / seam | Disposition | Source (read on `main`) | Justification |
|------------------|-------------|--------------------------|---------------|
| OTLP ingest engine + `spawn` seam | **REUSE** | `aperture::spawn(config, sink) -> Handle` (`aperture/src/lib.rs:381`; binds 4317/4318 on current runtime, `compose.rs:121`) | The ingest server, backpressure, readiness, graceful drain, ingest-auth, body caps all exist and are proven. |
| OTLP→durable translation + tenancy | **REUSE** | `StorageSink::with_all_stores(Arc<…Log>, Arc<…Trace>, Arc<…Metric>, cfg)` (`aperture-storage-sink/src/lib.rs:181`) | The write side; the gateway already Arc-shares the stores into it (`gateway/main.rs:91-96`). Verbatim reuse. |
| Metric store (pulse) | **REUSE, unchanged** | `FileBackedMetricStore` (`pulse/src/file_backed.rs:77`) | Interior-`Mutex` store is already safe for shared write+read; no change. |
| Log store (lumen) | **REUSE, unchanged** | `FileBackedLogStore` (`lumen/src/file_backed.rs`) | Same shape. |
| Trace store (ray) | **REUSE, unchanged** | `FileBackedTraceStore` (`ray/src/file_backed.rs`) | Same shape. |
| Metrics query router | **REUSE (lib seam)** | `query_api::router_with_auth(store, tenant, auth, static_dir)` (`query-api/src/lib.rs:152`) | Already accepts an injected `Arc<dyn MetricStore>`. THE reuse seam. |
| Logs query router | **REUSE (lib seam)** | `log_query_api::router(store, tenant)` / `router_with_auth` (`log-query-api/src/lib.rs:95,104`) | Already accepts an injected `Arc<dyn LogStore>`. |
| Traces query router (window + by-id) | **REUSE (lib seam)** | `trace_query_api::router(store, tenant)` / `router_with_auth` (`trace-query-api/src/lib.rs:100,110`) | Already accepts an injected `Arc<dyn TraceStore>`; both routes share the state. |
| Read-tier addr / tenant / auth / probe resolvers | **REUSE** | `*_query_api::composition::{resolve_addr, resolve_tenant, resolve_read_auth, probe}` | The runtime calls these per signal rather than re-deriving env handling. |
| Read-tier tracing install | **REUSE** | `query_http_common::init_tracing()` (`OnceLock` + `try_init`) | One install; aperture's `install_subscriber()` then no-ops (ADR-0015/0009). |
| Tenancy / auth primitives | **REUSE** | `aegis::TenantId` / `aegis::Validator` | Unchanged; tenant key + optional bearer paths preserved. |
| Fsync-honesty + active-write probes | **REUSE** | gateway `probe_or_refuse` (ADR-0049); store read probes (ADR-0042/0047/0048) | The runtime RUNS these before serving — its Earned-Trust obligation, discharged by reuse. |
| **The consolidated composition root + bin** | **CREATE-NEW** | `crates/kaleidoscope-runtime` (new), bin `kaleidoscope` | **Justification: no existing crate composes ingest + the three read routers on one runtime; the gateway is pure-ingest and must stay so (DD5 additive); extending it pollutes its dependency graph. The new crate contains NO domain logic, NO new store, NO new port — only wiring over the existing `pub` seams. It is the minimal additive surface.** |

**CREATE-NEW count: 1** (a wiring-only composition root). Every other element is reuse. No new domain concept, store, port, or external integration is introduced.

---

## Earned Trust (principle 12) — discharge by reuse

No new adapter is introduced, so no new `probe()` contract is authored. The runtime's Earned-Trust responsibility is the **composition-root invariant "wire → probe → use" across all five listeners**: build stores → run the sink's fsync + active-write probe AND each store's read probe → bind 4317/4318/9090/9091/9092 → only then serve; **any** probe or bind failure ⇒ `event=health.startup.refused` + non-zero exit (no half-up process). This is itself an acceptance criterion (US-01 empty-success / US-05 all-ports-bound + fail-closed), so it is a test, not a convention. The reused adapters keep their existing structural + behavioural probe-gold enforcement.

---

## For Acceptance Designer (Quinn, DISTILL)

### Driving entry (where to exercise behaviour)

- **The consolidated runtime binary** `kaleidoscope` (crate `kaleidoscope-runtime`), started **once** with an **empty** pillar root, `KALEIDOSCOPE_TENANT=acme`, auth off. It is the one composition root that binds ingest (gRPC 4317 / HTTP 4318) and the three query servers (9090/9091/9092) on **one process**.
- **The ingest push**: an OTLP export to the HTTP `:4318` endpoint (`/v1/metrics`, `/v1/logs`, `/v1/traces`) or gRPC `:4317`, for tenant `acme`.
- **The query GET**: `GET :9090/api/v1/query_range?query=..&start=..&end=..` (metrics), `GET :9091/api/v1/logs?..` (logs), `GET :9092/api/v1/traces?..` and `GET :9092/api/v1/traces/by_id?..` (traces).
- **Tests bind ephemeral `127.0.0.1:0` and sweep+retry** (fixed-port 4317/4318 flake, project memory). The single-process integration test that ingests-then-queries in one process — without dropping/reopening any store or starting a second process — is the load-bearing guard that the frozen-snapshot bug cannot return.

### What each acceptance criterion asserts

- **a-metric-is-queryable-immediately-after-it-is-sent** (US-01) — start the runtime with an empty pulse store for `acme`; POST OTLP `request_count`=1 at T; `GET /api/v1/query_range` over a window covering T returns a point with value 1 at T; **no restart** of anything between send and query. (The single-process write-then-read returning the value is the observable proof the sink and router hold the SAME `Arc`.)
- **an-empty-store-returns-empty-success-not-error** (US-01) — before any telemetry, `query_range` returns `{status:success, data:{resultType:matrix, result:[]}}`, HTTP 200, never 500.
- **a-post-startup-append-is-visible-without-restart** (US-01) — the runtime began empty before any telemetry; a metric sent **after** startup is returned by a later query with no restart. (This is the exact loop that fails in the separate-process world; it is the north-star regression guard.)
- **ingest-and-metrics-query-served-by-one-process** (US-01) — the OTLP push and the `query_range` are answered by the **same single running process**.
- **cross-tenant-read-returns-empty** (US-02) — ingest `request_count` for `acme`; a `query_range` scoped to `globex` returns empty success; **none** of `acme`'s data leaks. (Tenant isolation holds in-process.)
- **owning-tenant-read-returns-its-own-data** (US-02) — the same query scoped to `acme` returns the point.
- **untenanted-record-uses-the-default-tenant-else-fail-closed** (US-02) — with a default tenant configured, an OTLP record lacking `tenant.id` is attributed to it and is queryable under it; with no default configured, ingest of an untenanted record is refused (fail-closed), unchanged from the gateway.
- **optional-read-auth-stays-fail-closed-when-configured** (US-02) — `router_with_auth` with a `Some(validator)` still refuses a tokenless/invalid-bearer request (401, before the store) and never downgrades to the env tenant; the local posture leaves it off (env-tenant path, header ignored).
- **a-log-is-queryable-immediately-after-it-is-sent** (US-03) — POST OTLP log `"checkout failed: card declined"` for `acme` at T; `GET /api/v1/logs` over a window covering T returns it, no restart. Empty-before-send returns empty success; a `globex`-scoped query returns empty.
- **a-trace-is-queryable-by-window-and-by-id** (US-04) — POST an OTLP span (trace id `4bf92f3577b34da6a3ce929d0e0e4736`) for `acme` at T; `GET /api/v1/traces` (window) AND `GET /api/v1/traces/by_id` (point lookup) each return it, no restart. By-id with no match / empty store returns empty success (not error); a `globex`-scoped query returns empty; both routes are served by the same process as ingest.
- **one-command-three-signals-all-five-ports** (US-05, capstone) — one startup command runs ingest + all three query endpoints on one process; a metric, a log, and a trace each ingested for `acme` are each queryable back from their endpoint with no restart; all five endpoints bind and answer **without port conflict**; on a fresh pillar root all three query endpoints return empty successes before any telemetry and the data after one push per signal.
- **fail-closed-startup** (US-01/US-05) — if any of the five listeners cannot bind, or any store probe fails, the runtime refuses to start (`event=health.startup.refused`, non-zero exit) — no half-up process.

### Slice map (confirmed for DELIVER)

- **Slice 1 — metrics live loop**: US-01 + US-02. The new composition root builds all three stores and the `with_all_stores` sink (ingest persists every signal from the start — verbatim gateway reuse), and binds ingest (4317/4318) + the **metrics** query router (9090). Logs/traces query routers are added in slice 2. This is the feature walking skeleton; it derisks the whole single-process bet on the simplest signal.
- **Slice 2 — logs + traces + capstone**: US-03 (bind logs router 9091), US-04 (bind traces router 9092, both routes), US-05 (all five ports + three-signal proof on one process). Same composition pattern as slice 1, applied to lumen and ray, plus the all-ports capstone.

No adjustment to the DISCUSS two-slice plan is needed.

---

## Back-propagation to DISCUSS

**None.** No DISCUSS assumption changed: single-process (W1), additive (W2), minimal-friction local posture (W3), no-regression (W4), two-slice scope (W5), and the live-visibility observable (W6) are all designed to as stated. The DESIGN questions (new-binary-vs-extend, port layout, shared-store concurrency, single tracing install, one composition root) are all resolved within the single-process shape. `design/upstream-changes.md` is therefore intentionally absent.

---

## Self-review (no nested reviewer invoked this run — recorded verdict against the SA critique dimensions)

| Dimension | Verdict | Note |
|-----------|---------|------|
| Reuse Analysis present + every CREATE-NEW justified | **PASS** | One CREATE-NEW (wiring-only crate), justified; all load-bearing parts REUSE. |
| C4 + sequence diagrams present | **PASS** | C4 Container (one process, three shared stores, three routers, Arc edges) + send→query sequence in the brief. |
| ADR alternatives incl the veto alternative | **PASS** | A1 (extend gateway), A2 (distributed WAL-watch — the veto target), A3 (in-process broker). |
| Shared-`Arc` live-visibility mechanism precise + correct | **PASS** | Exact Arc flow + same-allocation/same-Mutex invariant; grounded in `store.rs`/`file_backed.rs` line citations. |
| No store concurrency regression | **PASS (confirmed by reading)** | `&self` + interior `Mutex`; ingest/query lock the same Mutex. Latency caveat flagged honestly, not hidden. |
| Additive-binaries constraint | **PASS** | DD5; four existing binaries untouched; enforced by `cargo build --workspace`. |
| Fail-closed startup | **PASS** | DD3 wire→probe→use across all five listeners; `health.startup.refused` + non-zero exit. |
| No overstated claims | **PASS** | The one thing not fully confirmable by reading (fsync/lock latency interaction) is named as a DELIVER/DEVOPS must-verify, not asserted as solved. |
| Resume-driven / bias check | **PASS** | Default modular-monolith composition; no microservices, no broker (A3 rejected as resume-driven); simplest solution that meets the need. |
| Priority validation (largest bottleneck) | **PASS** | Targets the single load-bearing gap named by the assessment §4 and the roadmap; simpler alternatives (A1/A3) and the scaling alternative (A2) documented. |

**Approval (self-review): approved.** Critical issues: 0. High issues: 0. One MEDIUM watch-item recorded (the fsync/lock read-latency interaction) — owned by DELIVER/DEVOPS as a measure-don't-pre-optimise item.

---

## Handoff

- **DISTILL (`acceptance-designer`, Quinn)**: the "For Acceptance Designer" section above + the per-story Gherkin in `discuss/user-stories.md` are the scenario SSOT. The single-process ingest-then-query test is the core. Bind ephemeral ports + sweep/retry.
- **DELIVER (`nw-software-crafter`)**: build `crates/kaleidoscope-runtime` (bin `kaleidoscope`) as the wiring-only composition root per ADR-0076 DD1–DD5. Verify the no-concurrency-change premise holds in practice (it should, per the reading) and measure the freshness KPI under concurrent ingest (the DD2 caveat). Touch no existing composition root.
- **DEVOPS (`platform-architect`)**: C2 (run story) wraps this binary; instrument the ingest-ack→query-returns interval for KPI 2 (p95 < 1 s); watch the fixed-port flake. No external-integration contract tests for C1.
