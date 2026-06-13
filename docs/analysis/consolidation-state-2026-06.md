# Kaleidoscope — Consolidation / Integration State Assessment

Date: 2026-06-13
Scope: read-only assessment of `main`. No code changed.
Question: how close is Kaleidoscope to a runnable system where a user
sends OTLP telemetry in one end and sees/queries it out the other
(metrics in Prism, logs + traces via the query APIs), and what is
missing to experiment with it?

---

## Executive verdict

Kaleidoscope today is **a set of well-built, durable, individually
runnable binaries that do NOT yet run together as one live
experimentable system out of the box.** The pieces exist and share a
storage *path*, but:

1. **There is no single command / compose / Makefile to bring the stack
   up.** No `docker-compose.yml`, no `compose.yaml`, no `Makefile`/
   `justfile`/run-script anywhere in the repo (verified: `find` over the
   tree excluding `node_modules`/`target` returns nothing). The README
   "Quick start" only covers the `kaleidoscope-cli` NDJSON path, not the
   gateway+query+Prism stack.
2. **Ingest and query are separate OS processes that share state only
   through a filesystem path, and the query processes load that state
   ONCE at startup.** A query API started before telemetry arrives will
   not see the new data until it is restarted. This is the central
   consolidation gap.
3. **Only metrics are visualised.** Prism calls `/api/v1/query_range`
   (metrics) only; logs/traces have query-API binaries but no UI.

So the honest summary: **durable components + a real ingest binary +
three real query binaries + a metrics UI, all proven in-process by the
integration suite, but not yet assembled into a single live stack with
shared state and a one-command run story.**

---

## 1. Crate inventory + role + integration status

Determined from `cargo metadata --no-deps`, the reverse-dependency graph
(who depends on whom among workspace crates), and reading the binary
composition roots.

Legend: **WIRED** = part of the runnable gateway→store→query→Prism path.
**LIB** = library consumed by a wired binary. **STANDALONE/UNWIRED** =
builds, but no wired binary or deployment runs it (only tests, or
nothing).

| Crate | Role (one line) | Status | Evidence |
|---|---|---|---|
| `aperture` (bin+lib) | OTLP ingest gateway engine (gRPC :4317, HTTP :4318) | **WIRED (lib) via gateway**; bin is a standalone forward-proxy | `kaleidoscope-gateway` calls `aperture::spawn` (gateway/main.rs:127); aperture bin = `cargo run -p aperture` forward-proxy |
| `aperture-storage-sink` (lib) | Translates OTLP → durable pillars (lumen/ray/pulse) | **LIB, WIRED** | only reverse-dep is `kaleidoscope-gateway`; `StorageSink::with_all_stores` (gateway/main.rs:91) |
| `kaleidoscope-gateway` (bin) | Runnable OTLP ingest that persists to the three pillars | **WIRED — the ingest binary** | composition root reads pillar_root, opens lumen/ray/pulse, injects sink into aperture (gateway/main.rs:67-127) |
| `pulse` (lib+crash bin) | Time-series metric store (file-backed durable) | **WIRED** (written by gateway, read by query-api) | gateway/main.rs:84; query-api/main.rs:111 |
| `lumen` (lib+crash bin) | Log store (file-backed durable) | **WIRED** (gateway write, log-query-api read) | gateway/main.rs:76; log-query-api/main.rs:105 |
| `ray` (lib+crash bin) | Trace store (file-backed durable) | **WIRED** (gateway write, trace-query-api read) | gateway/main.rs:80; trace-query-api/main.rs:108 |
| `query-api` (bin) | Prometheus-shaped `/api/v1/query_range` over Pulse; can also serve Prism bundle | **WIRED — metrics read** (separate process) | query-api/main.rs |
| `log-query-api` (bin) | Log query HTTP API over Lumen (:9091) | **WIRED — logs read** (separate process) | log-query-api/main.rs |
| `trace-query-api` (bin) | Trace query HTTP API over Ray (:9092) | **WIRED — traces read** (separate process) | trace-query-api/main.rs |
| `query-http-common` (lib) | Shared read-tier HTTP/auth/tracing | **LIB, WIRED** | reverse-deps: the 3 query APIs |
| `aegis` (lib) | TenantId + JWT `Validator` (authN/Z, tenancy) | **WIRED (library)** — TenantId everywhere; Validator in ingest auth + read auth | reverse-deps: nearly every crate; `aegis::Validator` in aperture/compose.rs:58 and query read-auth |
| `prism` (app, TS/React) | Single-metric PromQL query/chart explorer | **WIRED — metrics only** | `QueryPanel` fetches `query_range` against `config.backend.url` (QueryPanel.tsx:172-187) |
| `wal-recovery` (lib) | Torn-tail-tolerant WAL replay | **LIB, WIRED** | reverse-deps: beacon, cinder, lumen, pulse, ray, sluice, strata |
| `kaleidoscope-cli` (bin) | Operator NDJSON ingest/read (lumen+cinder+self-observe) | **STANDALONE binary** (separate from the gateway path) | no reverse-deps; README quick start uses it |
| `self-observe` (lib) | MetricsRecorder bridges (self-observability) | **LIB, partial** — only `kaleidoscope-cli` | reverse-dep: cli only; gateway/query wire `NoopRecorder` |
| `cinder` (lib+crash bin) | Local tier-metadata governor (tiering) | **UNWIRED in the gateway path** | reverse-deps: cli, integration-suite, self-observe — NOT gateway |
| `sluice` (lib+crash bin) | Durable ingest buffer/queue | **UNWIRED** — only integration-suite (test) | gateway writes stores directly; no buffer in the path |
| `sieve` (lib) | Sampling/filtering | **UNWIRED** — nothing depends on it (it depends on aperture) | reverse-deps: none |
| `strata` (lib+crash bin) | Profile store | **UNWIRED** — only integration-suite | reverse-deps: integration-suite |
| `augur` (lib) | Anomaly detection | **UNWIRED** — only integration-suite | reverse-deps: integration-suite |
| `beacon` (lib+crash bin) | Alerting/SLO burn-rate engine | **UNWIRED** | reverse-deps: beacon-server, loom |
| `beacon-server` (bin) | Standalone alerting server | **STANDALONE/UNWIRED** | no reverse-deps; not in any compose/run story |
| `loom` (lib+bin) | TOML rule-catalogue change control | **UNWIRED** | no reverse-deps |
| `codex` (lib) | Schema registry / semantic conventions | **UNWIRED** — only `spark` | reverse-dep: spark |
| `spark` (lib) | Manual-init OTel SDK wrapper (client-side) | **STANDALONE** (client instrumentation, not server) | no reverse-deps |
| `otlp-conformance-harness` (lib) | OTLP conformance test suite | **LIB (test)** — used by aperture | reverse-dep: aperture |
| `integration-suite` (lib+tests) | Cross-crate in-process composition tests | **TEST harness** | no reverse-deps |
| `regenerate-codex-corpus` (bin) | Dev tool | STANDALONE dev tool | xtask-like |

---

## 2. The end-to-end data path

```
                          OTLP client (curl / otel SDK / spark)
                                     │  gRPC :4317 / HTTP :4318
                                     ▼
              ┌──────────────────────────────────────────┐
              │  kaleidoscope-gateway  (aperture engine)   │  OK (binary exists, wires all 3 signals)
              │  default tenant via KALEIDOSCOPE_DEFAULT_TENANT (fail-closed if unset)
              └──────────────────────────────────────────┘
                                     │  StorageSink::with_all_stores
                 ┌───────────────────┼────────────────────┐
                 ▼                   ▼                    ▼
        pulse (metrics)        lumen (logs)         ray (traces)        OK durable, per-record fsync (ADR-0049)
        $ROOT/pulse            $ROOT/lumen          $ROOT/ray
                 │                   │                    │
                 │   *** GAP: shared only by filesystem PATH, not live state ***
                 │   query procs load store into memory ONCE at open(); no re-read/watch
                 ▼                   ▼                    ▼
        query-api :9090     log-query-api :9091   trace-query-api :9092   OK binaries, but SEPARATE processes
                 │
                 ▼
        prism (React)  ── /api/v1/query_range ─▶ query-api               OK metrics only
        logs UI / traces UI                                              GAP (no UI; APIs only)

   sluice (buffer)  sieve (sampling)  cinder (tiering)                   GAP: exist, NOT in the path
   beacon/beacon-server (alerting)  augur (anomaly)  loom                GAP: standalone/unwired
```

- **Ingest — OK.** `kaleidoscope-gateway` binds aperture's gRPC `:4317`
  and HTTP/protobuf `:4318` (aperture/lib.rs:3,11-12), accepts all three
  OTLP-stable signals, and writes logs→lumen, traces→ray, metrics→pulse
  (gateway/main.rs:89-96). Earned-Trust probes (sink + fsync) gate the
  bind (gateway/main.rs:110). The standalone `aperture` bin is a
  forward-proxy to another OTLP backend, not the persisting path.
- **Routing/gateway — OK but minimal.** The gateway is the fan-out; it
  writes directly to the three stores. There is **no queue/buffer
  (sluice), no sampling (sieve), no tiering (cinder) in the path.**
- **Storage — OK, durable.** All three are `FileBacked*Store` with
  per-record WAL append + fsync (pulse/file_backed.rs:325; ADR-0049
  discipline at :226-285). Data is on disk immediately.
- **Query — WIRED but DISCONNECTED at runtime.** Each query API opens
  the same path the gateway writes (`$ROOT/pulse|lumen|ray`) — see the
  doc-comments and `FileBacked*Store::open` calls in each main.rs. BUT
  `open()` loads the snapshot + replays the WAL into an in-memory
  `Mutex<HashMap>` ONCE (pulse/file_backed.rs:111-222) and the query
  handlers read that frozen map. There is no reload/watch/re-read of the
  WAL (grep for reload|watch|inotify|reopen in the query+store crates
  finds only test/recovery references). **Consequence:** a query process
  only sees data that was on disk at *its own* `open()`.
- **Visualisation — metrics only.** Prism's `QueryPanel` issues
  `query_range` to `config.backend.url` (a single metrics backend). No
  logs or traces panels exist.

---

## 3. The run story

**There is no single command, and no compose/Makefile/script.** Verified:
- No `docker-compose.yml` / `compose.yaml` anywhere (find over tree).
- No `Makefile` / `justfile` at root.
- README "Quick start" (README.md:123-150) is ONLY the
  `kaleidoscope-cli` NDJSON ingest/read demo — not the gateway+query+Prism
  stack.
- Deployment artifacts that DO exist: `Dockerfile` (cli),
  `Dockerfile.gateway`, `Dockerfile.query-api` — three independent images,
  not composed together.

Binaries (`cargo metadata` bin targets) and how each launches:

| Binary | Launch | Default port |
|---|---|---|
| `kaleidoscope-gateway` | `cargo run -p kaleidoscope-gateway [pillar_root]` | gRPC 4317 / HTTP 4318 |
| `aperture` | `cargo run -p aperture [config.toml]` (standalone forward-proxy) | 4317/4318 |
| `query-api` | `cargo run -p query-api` | 9090 |
| `log-query-api` | `cargo run -p log-query-api` | 9091 |
| `trace-query-api` | `cargo run -p trace-query-api` | 9092 |
| `kaleidoscope-cli` | `cargo run -p kaleidoscope-cli -- ingest/read <tenant> <dir>` | n/a |
| `beacon-server` | standalone, not in any run story | n/a |
| `*-crash-target`, `regenerate-codex-corpus` | test/dev fixtures | n/a |

**Can Andrea run a few commands today and have a working system?**
Partially, with a sharp caveat. Manual sequence (all sharing
`KALEIDOSCOPE_PILLAR_ROOT`):

```bash
# 1. ingest
KALEIDOSCOPE_PILLAR_ROOT=/tmp/kal KALEIDOSCOPE_DEFAULT_TENANT=acme \
  cargo run -p kaleidoscope-gateway
# 2. send OTLP to :4317/:4318 ...
# 3. THEN start the query APIs (must be AFTER ingest, or restart them)
KALEIDOSCOPE_PILLAR_ROOT=/tmp/kal KALEIDOSCOPE_QUERY_TENANT=acme \
  cargo run -p query-api          # 9090 metrics
KALEIDOSCOPE_PILLAR_ROOT=/tmp/kal KALEIDOSCOPE_LOG_QUERY_TENANT=acme \
  cargo run -p log-query-api      # 9091 logs
KALEIDOSCOPE_PILLAR_ROOT=/tmp/kal KALEIDOSCOPE_TRACE_QUERY_TENANT=acme \
  cargo run -p trace-query-api    # 9092 traces
# 4. Prism: build apps/prism, point KALEIDOSCOPE_QUERY_STATIC_DIR at dist
```

**What is missing for a real run story:** a compose/Makefile that brings
all of this up; a getting-started doc for the *gateway* path (not just
the CLI); a telemetry generator / sample data; and — critically — the
live-state fix below, because step 3-after-step-2 (or "restart to see new
data") is not an acceptable experiment loop.

---

## 4. The store-sharing / state problem (the crux)

**Verdict: ingest and query do NOT share live state. They share a
filesystem path; the query side snapshots it once at startup.**

Evidence:
- Stores are **file-backed + durable**, not in-memory-only: each is a
  `FileBacked*Store` writing a WAL + snapshot under `$ROOT/{pulse,lumen,
  ray}` (gateway/main.rs:49-87), with per-record fsync.
- All three query mains open the **same path** the gateway writes (their
  own doc-comments say "the same store the gateway writes through" —
  query-api/main.rs:23-24, log-query-api/main.rs:22-23,
  trace-query-api/main.rs:21-22).
- BUT `FileBacked*Store::open` loads snapshot+WAL into an in-process
  `Mutex<Inner{ series: HashMap, ... }>` exactly once
  (pulse/file_backed.rs:111-222), and queries read that in-memory map.
  No file-watch, no per-query re-read (grep confirms).

So:
- A query process **started before** telemetry arrives returns stale/empty
  results until restarted — the common "bring up the stack, then send a
  metric, then look" flow **fails to show the metric**.
- The integration suite's "loop is complete" proof
  (`v1_three_durable_stores_compose.rs`,
  `v1_three_adapters_compose_under_restart.rs`) is **in-process**: it
  writes, drops the store, **re-opens**, and asserts recovery. It proves
  durability + restart recovery + tenant isolation — NOT live two-process
  ingest↔query sharing.

This is the single most important consolidation gap. Two viable fixes:
(a) **one composed process** that holds the stores once and serves both
ingest and query against the same in-memory `Arc<Store>` (the shape the
integration suite already exercises); or (b) make the query stores
**re-read on each query / watch the WAL** so a separate query process
reflects new appends. (a) is the smaller, more honest first step for an
experiment.

Minor secondary hazard: gateway and each query process both open the same
WAL in append mode with no cross-process file lock; query never writes so
it is benign today, but two writers (e.g. two gateways) would corrupt.

---

## 5. Auth / config friction for experimenting

Good news: **the recent auth/TLS work does NOT block a simple local
experiment**, as long as you set a tenant.

- **Ingest (gateway):** the gateway builds `Config::builder().build()`
  with **no `jwt_auth`** (gateway/main.rs:124) → ingest auth is **off** by
  default. (Only the standalone `aperture` *TOML-path* refuses to start
  without an auth block — gateway path and `aperture` with no config arg
  run unauthenticated.) TLS is plaintext at v0; `tls.enabled=true`
  fail-closes, but it is off by default.
- **The one required knob:** the gateway fail-closes on records lacking a
  `tenant.id` unless `KALEIDOSCOPE_DEFAULT_TENANT` is set
  (gateway/main.rs:192-197). So you MUST set a default tenant.
- **Query (all three APIs):** read-auth is **optional and additive** — a
  *wholly absent* auth config = env-tenant mode (main.rs comments + 
  `resolve_read_auth`). Minimal config: set
  `KALEIDOSCOPE_QUERY_TENANT` / `KALEIDOSCOPE_LOG_QUERY_TENANT` /
  `KALEIDOSCOPE_TRACE_QUERY_TENANT` to the same tenant the gateway uses.
  No tokens, no secrets needed. (A *partial* auth config is a refuse-to-
  start error, exit 2 — but you simply leave all auth env vars unset.)

**Minimal-friction local posture:** auth off everywhere; set
`KALEIDOSCOPE_DEFAULT_TENANT=acme` on the gateway and
`KALEIDOSCOPE_*_QUERY_TENANT=acme` on the query APIs; share
`KALEIDOSCOPE_PILLAR_ROOT`. This works end-to-end *modulo the §4
live-state gap* (you must (re)start the query APIs after ingest).

---

## 6. What exists but is unwired (the shelf)

Verified against current reverse-dep graph + composition roots:

| Component | Current state | Matters for a first experiment? |
|---|---|---|
| `aegis` | **Now wired** — TenantId pervasive; `Validator` wired into ingest auth (aperture/compose.rs) and read auth (query APIs) | already in |
| `sluice` (buffer) | **Still unwired** — only integration-suite | No (future durability/backpressure) |
| `sieve` (sampling) | **Still unwired** — nothing depends on it | No (future) |
| `cinder` (tiering) | Wired into `kaleidoscope-cli`, **not** the gateway path | No (future / cold tier) |
| `strata` (profiles) | **Unwired** — only integration-suite | No (4th signal, future) |
| `augur` (anomaly) | **Unwired** — only integration-suite | No (future) |
| `beacon` / `beacon-server` (alerting/SLO) | **Standalone/unwired**, not in any run story | No (future) |
| `loom` (rule catalogue) | **Unwired** | No (future) |
| `codex` (schema registry) | **Unwired** — only `spark` | No (future) |
| `self-observe` | Wired into `kaleidoscope-cli` only; gateway/query use `NoopRecorder` | Nice-to-have |
| `spark` (SDK) | Standalone client lib | Useful as a telemetry *source* for the experiment |

For a minimal experimentable system, the only crates that MATTER are the
ones already wired (gateway + aperture + storage-sink + lumen/pulse/ray +
the three query APIs + prism + aegis). Everything else on the shelf is
future scope.

---

## 7. The gap list (ordered)

### MUST-HAVE for a first experiment

1. **Fix live state-sharing (the crux).** Either compose ingest+query in
   one process over a shared `Arc<Store>`, or make the query stores
   reflect post-startup appends (re-read/watch). Touches:
   `crates/kaleidoscope-gateway`, `crates/{query-api,log-query-api,
   trace-query-api}`, and the store crates `crates/{pulse,lumen,ray}/src/
   file_backed.rs`. Without this, "send a metric and see it" fails unless
   you restart the reader.
2. **One-command run story.** Add a `docker-compose.yml` (using the
   existing `Dockerfile.gateway` / `Dockerfile.query-api` + a Prism static
   image) OR a `Makefile`/`justfile` that launches gateway + 3 query APIs
   + Prism with a shared pillar volume and the minimal tenant env. Touches:
   repo root, the three Dockerfiles, `apps/prism`.
3. **Getting-started for the gateway path.** README currently documents
   only the CLI. Add the OTLP→gateway→query→Prism walkthrough with the
   minimal env (default tenant + query tenants + pillar root). Touches:
   `README.md`, `docs/`.
4. **A telemetry generator / sample data.** A scripted way to push one
   metric + one log + one trace via OTLP (e.g. a `spark`-based sender or a
   curl-of-protobuf helper) so the user has something to query. Touches:
   `crates/spark`, `scripts/`.

### NICE-TO-HAVE (not blocking a first experiment)

5. **Logs/traces UI in Prism** (today metrics-only). Touches `apps/prism`.
6. **`step` honoured in query-api** (raw points today, ADR-0062). Touches
   `crates/query-api`.
7. **Wire `sluice` (buffer) / `sieve` (sampling) into the ingest path**
   for realism. Touches `crates/{sluice,sieve,aperture-storage-sink}`.
8. **`self-observe` in the gateway/query** (replace `NoopRecorder`).
9. **`cinder` tiering, `beacon` alerting, `augur` anomaly, `strata`
   profiles** — later phases.

---

## Appendix — how this was determined

- `cargo metadata --no-deps` for bin/lib targets and the reverse-dep graph.
- Read the four composition roots: `crates/kaleidoscope-gateway/src/
  {main,composition}.rs`, `crates/{query-api,log-query-api,
  trace-query-api}/src/main.rs`.
- Read `crates/pulse/src/file_backed.rs` (`open` loads to in-memory
  `Mutex<Inner>` once; per-record fsync append at :325).
- `find` for compose/Makefile/k8s (none; only per-feature
  `docs/feature/.../devops/environments.yaml`).
- Prism: `apps/prism/src/{app/App.tsx,panels/query/QueryPanel.tsx}`
  (metrics `query_range` only).
- Integration suite: `crates/integration-suite/tests/v1_three_durable_
  stores_compose.rs` (in-process write→drop→reopen recovery proof).
