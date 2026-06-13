# Wave Decisions — `consolidated-runtime-v0` (DISTILL)

> **Wave**: DISTILL (`nw-acceptance-designer` / Quinn). Autonomous overnight run, 2026-06-13.
> **Feature**: `consolidated-runtime-v0` — item C1 (the spine).
> **Upstream SSOT**: ADR-0076 (`docs/product/architecture/adr-0076-consolidated-runtime.md`),
> DESIGN `design/wave-decisions.md` "## For Acceptance Designer", DEVOPS `devops/environments.yaml`.
> **Scope SSOT**: DISCUSS `discuss/user-stories.md` (US-01..US-05) + `discuss/story-map.md` (two slices).
> **nWave ORDER**: DISCUSS → DESIGN → DEVOPS → DISTILL → **DELIVER**. The crate
> `kaleidoscope-runtime` did NOT exist before this wave; DISTILL writes the acceptance suite +
> a RED-not-BROKEN scaffold so the suite COMPILES and is RED on behaviour, not on missing
> symbols. DELIVER writes the production composition.

---

## The outcome under test (the heart)

A metric/log/trace ingested into the consolidated process at time T is queryable from the SAME
process at T+epsilon **without a restart** — the live-visibility loop that fails today because
ingest and query are separate processes with separate frozen in-memory stores. C1 runs ingest +
the shared stores + the three query routers in ONE process over shared `Arc<Store>`s so a write
is immediately readable. The load-bearing guard
(`metric_is_queryable_immediately_after_it_is_sent`) would be RED on the separate-process
architecture and is GREEN only when the sink and router hold the SAME `Arc`.

---

## DT1 — Test architecture: in-process spawn on ephemeral ports

**Decision.** The acceptance suite is an **in-process** integration suite. Each scenario calls
the one driving entry `kaleidoscope_runtime::spawn_consolidated(config)` in the TEST process,
which builds the composition root, binds all five listeners, and returns a `RunningRuntime`
carrying the five **actual bound** `SocketAddr`s. The test then POSTs a real OTLP protobuf body
to the ingest HTTP listener (`reqwest`, `application/x-protobuf`, mirroring aperture's
`post_otlp_protobuf`) and GETs the query routers over loopback — all in ONE process, with NO
second process and NO store drop/reopen between send and query. That single-process
write-then-read is the proof the sink and router share the same live `Arc<Store>` (ADR-0076
Enforcement); if they ever held different instances the write would not be visible and the test
reds.

**Rationale.** This is the only shape that can falsify the frozen-snapshot bug: a two-process or
drop/reopen test would pass even on the broken architecture. It matches the sibling convention
(`integration-suite/tests/v1_three_durable_stores_compose.rs`, the aperture slice tests, the
read-auth slice tests `slice_07/09/05`): Rust `tests/slice_*.rs` integration tests with Gherkin
in doc-comments and `@tag` annotations in comments. **There are no `.feature` files in this
Rust workspace; the executable Gherkin lives in the doc-comments**, exactly as every sibling
acceptance suite does.

## DT2 — Ephemeral ports, MANDATORY (no fixed-port bind)

**Decision.** All five listeners bind ephemeral `127.0.0.1:0`; the test reads the actual bound
addresses back from `RunningRuntime`. The fixed defaults (ingest 4317/4318, query
9090/9091/9092) are **NEVER** bound in tests. `ConsolidatedConfig::for_ephemeral_test(..)` sets
every listener to `127.0.0.1:0`; the fail-closed scenario occupies a real ephemeral port and
hands it to the runtime as one of the five binds to force a conflict.

**Rationale.** The fixed-port 4317/4318 flake (project memory `aperture_fixed_port_4317_flake`,
`devops/environments.yaml` `acceptance_environment.ports`): a leaked mutant binder or a running
instance on a fixed port flakes the suite. Ephemeral bind + read-back is the fix; the suite is
safe to parallelise. **Confirmed: no test in the suite binds a fixed port** (grep evidence in
the self-review below).

## DT3 — RED-not-BROKEN scaffold (Mandate 7)

**Decision.** The new crate `crates/kaleidoscope-runtime` is the minimal scaffold that makes the
suite COMPILE while every scenario is RED on behaviour:

- `Cargo.toml` declares the union of reuse-seam deps DELIVER needs (`aperture`,
  `aperture-storage-sink`, `pulse`/`lumen`/`ray`, the three query crates, `query-http-common`,
  `aegis`, `axum`, `tokio`, `tracing`) + dev-deps for the suite (`reqwest`,
  `opentelemetry-proto`, `prost`, `serde_json`, `jsonwebtoken`, `tokio`). Added to the workspace
  members.
- `src/lib.rs` exposes the driving entry `pub async fn spawn_consolidated(config:
  ConsolidatedConfig) -> Result<RunningRuntime, RuntimeError>`, plus `ConsolidatedConfig`
  (pillar root, the five bind addrs, the four tenant roles, optional uniform read-auth
  validator, optional static dir) and `RunningRuntime` (the five actual bound addrs + a shutdown
  handle). The body is a **`__SCAFFOLD__ ... not yet implemented` panic** naming the exact
  composition DELIVER must build (mirroring the project's RED-not-BROKEN precedent — aperture's
  DISTILL `unimplemented!()` / panicking helpers, "that panic is the canonical RED state").
- `src/main.rs` is a thin bin (`kaleidoscope`) that resolves the host-binary surface from the
  environment (`KALEIDOSCOPE_PILLAR_ROOT`, the unified `KALEIDOSCOPE_TENANT` with per-role
  overrides, the fixed default ports, the optional read-auth set) into a `ConsolidatedConfig`
  and calls the lib. It contains NO store, NO router, NO domain logic — the shared-`Arc`
  composition lives entirely inside `spawn_consolidated`, which DELIVER fills.

**Trunk-green posture.** Every scenario is `#[ignore]`d (all 19 depend on the runtime spawning),
so `cargo test --workspace --all-targets` is GREEN until DELIVER (proof below). RED is observed
on demand with `--ignored`. This matches the read-auth slices' posture (RED scenarios ignored,
guardrails green) — here there is no auth-off guardrail subset because every scenario needs the
live runtime, so all are ignored.

## DT4 — Slice grouping mirrors DESIGN

- **Slice 1** (`tests/slice_01_live_metrics.rs`): US-01 + US-02 (metrics live loop). The feature
  walking skeleton.
- **Slice 2** (`tests/slice_02_live_logs_traces.rs`): US-03 (logs) + US-04 (traces, window +
  by-id) + US-05 (one-command, all-five-ports, three-signal capstone).

---

## Scenario list (by slice; happy vs error/edge)

> Total **19** scenarios. Error/edge **9** = **47%** (≥ 40% target met). Walking-skeleton
> scenarios: 2 (`metric_is_queryable_immediately_after_it_is_sent`,
> `every_signal_queryable_back_live_no_restart`). `@driving_port` on all entry-through-port
> scenarios. One `@kpi` freshness scenario.

### Slice 1 — metrics (US-01, US-02) — 9 scenarios

| # | Scenario | Story | Class | Tags |
|---|----------|-------|-------|------|
| 1 | `metric_is_queryable_immediately_after_it_is_sent` | US-01 | happy (WS, north star) | `@walking_skeleton @driving_port @US-01` |
| 2 | `metric_sent_after_startup_is_visible_without_restart` | US-01 | happy (regression guard) | `@driving_port @US-01` |
| 3 | `ingest_and_metrics_query_served_by_one_process` | US-01 | happy | `@driving_port @US-01` |
| 4 | `owning_tenant_read_returns_its_own_data` | US-02 | happy (isolation +ve) | `@driving_port @US-02` |
| 5 | `empty_store_returns_empty_success_not_error` | US-01 | **edge** | `@driving_port @US-01` |
| 6 | `cross_tenant_read_returns_empty` | US-02 | **error** (isolation −ve) | `@driving_port @US-02` |
| 7 | `optional_read_auth_stays_fail_closed_when_configured` | US-02 | **error** | `@driving_port @US-02` |
| 8 | `fail_closed_startup_on_bind_conflict` | US-01/US-05 | **error** | `@driving_port @US-01 @US-05` |
| 9 | `freshness_metric_returns_within_budget` | US-01 | edge (KPI) | `@kpi @driving_port @US-01` |

### Slice 2 — logs + traces + capstone (US-03, US-04, US-05) — 10 scenarios

| # | Scenario | Story | Class | Tags |
|---|----------|-------|-------|------|
| 10 | `log_is_queryable_immediately_after_it_is_sent` | US-03 | happy | `@driving_port @US-03` |
| 11 | `logs_empty_before_send_returns_empty_success` | US-03 | **edge** | `@driving_port @US-03` |
| 12 | `cross_tenant_log_read_returns_empty` | US-03 | **error** (isolation −ve) | `@driving_port @US-03` |
| 13 | `trace_is_queryable_by_window_immediately` | US-04 | happy | `@driving_port @US-04` |
| 14 | `trace_is_retrievable_by_id` | US-04 | happy | `@driving_port @US-04` |
| 15 | `trace_by_id_before_any_trace_returns_empty_success` | US-04 | **edge** | `@driving_port @US-04` |
| 16 | `cross_tenant_trace_read_returns_empty` (window + by-id) | US-04 | **error** (isolation −ve, incl by-id, ADR-0053) | `@driving_port @US-04` |
| 17 | `one_command_binds_all_five_ports` | US-05 | happy/edge (all five bind, no conflict) | `@driving_port @US-05` |
| 18 | `every_signal_queryable_back_live_no_restart` | US-05 | happy (WS, three-signal) | `@walking_skeleton @driving_port @US-05` |
| 19 | `fresh_stack_returns_empty_success_across_all_signals` | US-05 | **edge** | `@driving_port @US-05` |

Error/edge tally: #5,6,7,8 (slice 1) + #11,12,15,16,19 (slice 2) = **9 / 19 = 47%**.

### Story coverage (Dim 8 Check A — every story has ≥1 scenario)

US-01 → #1,2,3,5,8,9 · US-02 → #4,6,7 · US-03 → #10,11,12 · US-04 → #13,14,15,16 ·
US-05 → #8,17,18,19. **No orphan story.**

---

## The load-bearing scenarios (why they prove shared-live-state)

1. **LIVE VISIBILITY (north star)** — #1: empty runtime → POST one OTLP metric → immediately GET
   `query_range` → assert the point (value 1) returns, NO restart. RED on the separate-process
   (frozen-snapshot) architecture; GREEN only on the shared store.
2. **EMPTY-BEFORE-INGEST** — #5/#11/#15/#19: query before any ingest returns `status:success` +
   empty (HTTP 200), never an error.
3. **TENANT ISOLATION in-process** — positive (#4) present for `acme`; negative (#6 metrics, #12
   logs, #16 traces incl by-id) absent for `globex`. Negative controls are falsifiable: a query
   that ignored the tenant key would return `acme`'s data and FAIL.
4. **ALL FIVE LISTENERS BIND** — #17: ingest gRPC (TCP-connect proof) + ingest HTTP (push 200) +
   the three query routers (each 200) on one process, distinct non-zero ports, no conflict.
   #8 is the fail-closed converse: an occupied bind ⇒ startup REFUSED (no half-up process).
5. **THREE-SIGNAL, ONE PROCESS, NO RESTART** — #18: a metric, a log, and a trace each queryable
   back live from the one process.

---

## KPI / observability (soft gate)

`docs/product/kpi-contracts.yaml` does **not** exist (product-level KPI-contracts file absent).
**Soft-gate WARNING logged, proceeding.** The feature's freshness KPI is nonetheless defined in
`discuss/outcome-kpis.md` and `devops/environments.yaml` (`observability.freshness_kpi`): KPI 1
live-visibility = 100%, KPI 2 ingest-ack → query-returns p95 < 1 s, "for v0 the acceptance test
is the measurement". Scenario #9 (`@kpi`) timestamps the ingest-ack → query-returns interval and
asserts a **generous local budget** (5 s, well above the p95 < 1 s SLO); per
`devops/environments.yaml` and project memory `p95_wallclock_flakes_overnight`, CI is the
indicative SLO measure and threshold-raising is never the fix for a flake. The deterministic
contractual gate is the live-visibility value assertion (#1/#2/#18), not the timing.

---

## RED-not-BROKEN proof (run output, 2026-06-13)

- **Compiles (NOT BROKEN)**: `cargo test -p kaleidoscope-runtime --no-run` → `Finished` (exit 0);
  all four targets built (lib, bin `kaleidoscope`, `slice_01`, `slice_02`).
- **RED on behaviour (`--ignored`)**:
  - `slice_01_live_metrics`: `test result: FAILED. 0 passed; 9 failed` — every failure is the
    `__SCAFFOLD__ kaleidoscope_runtime::spawn_consolidated not yet implemented` panic at
    `src/lib.rs:217`, i.e. RED on the missing live runtime, NOT a compile error.
  - `slice_02_live_logs_traces`: `test result: FAILED. 0 passed; 10 failed` — same scaffold panic.
- **Trunk stays GREEN (default run, no `--ignored`)**: `cargo test -p kaleidoscope-runtime` →
  all targets `ok` (`9 ignored`, `10 ignored`, `0 failed`).
- **Additive (DD5)**: `cargo build --workspace` builds all binaries; no existing crate source was
  touched (only a workspace member added + the new crate).

**Verdict: RED-not-BROKEN proven.** The crate compiles into the workspace; the scenarios fail on
the scaffold's not-implemented panic (RED), not on missing symbols (BROKEN); trunk is green until
DELIVER via `#[ignore]`.

---

## The precise composition entry DELIVER must implement

`pub async fn spawn_consolidated(config: ConsolidatedConfig) -> Result<RunningRuntime, RuntimeError>`
in `crates/kaleidoscope-runtime/src/lib.rs`. Replace the `__SCAFFOLD__` panic with (ADR-0076
DD2/DD3):

1. Build `Arc<FileBackedMetricStore>` / `Arc<FileBackedLogStore>` / `Arc<FileBackedTraceStore>`
   once under `config.pillar_root` (sub-dirs `pulse`/`lumen`/`ray`), exactly as
   `kaleidoscope-gateway/src/main.rs`.
2. `Arc::clone` each into `StorageSink::with_all_stores(log, trace, metric, sink_config)` (WRITE;
   `sink_config` = `with_default_tenant(config.default_ingest_tenant)` else `no_default_tenant()`)
   **AND** the SAME `Arc` (coerced to `Arc<dyn …Store + Send + Sync>`) into the matching
   `query_api::router_with_auth(metric, metrics_query_tenant, read_auth, static_dir)` /
   `log_query_api::router_with_auth(log, logs_query_tenant, read_auth)` /
   `trace_query_api::router_with_auth(trace, traces_query_tenant, read_auth)` (READ).
3. Install one tracing subscriber (`query_http_common::init_tracing()`; aperture's + the read
   tier's later installs no-op).
4. Earned-Trust wire → probe → use: run the sink active-write + fsync-honesty probe (gateway
   `probe_or_refuse`, ADR-0049) and each store's read probe (ADR-0042/0047/0048).
5. Bind the three query `TcpListener`s on `config.{metrics,logs,traces}_query_addr` (cheap
   port-conflict detection), then `aperture::spawn(Config::builder().grpc_bind_addr(..)
   .http_bind_addr(..).build()?, sink_as_dyn)` for the two ingest listeners.
6. On ANY bind/probe failure return `RuntimeError` (the bin maps it to
   `event=health.startup.refused` + non-zero exit — no half-up process).
7. Read back the five ACTUAL bound `SocketAddr`s into `RunningRuntime` (the aperture `Handle`
   exposes `grpc_addr()`/`http_addr()`; the axum listeners' `local_addr()`), serve all five on
   one runtime, and carry the aperture `Handle` + axum task handles for `RunningRuntime::shutdown`.

DELIVER drives one `#[ignore]` away per scenario, in order, GREEN one at a time. The crate is
enrolled in the new `gate-5-mutants-kaleidoscope-runtime` CI job; `main.rs` may be
`#[mutants::skip]` like the sibling binaries, leaving the composition logic (tenant precedence,
bind/probe ordering, fail-closed branch) as the mutation surface.

### `@property`-shaped criteria (signal to DELIVER crafter)

The tenant-isolation invariant ("a query scoped to tenant X **never** returns tenant Y's data,
**regardless of** which signal or route") is property-shaped. The negative controls (#6/#12/#16)
encode it as concrete examples; if DELIVER wants stronger coverage it MAY lift them to
property-based tests over (ingest-tenant, query-tenant, signal) tuples. Not required for C1.

---

## Self-review (no nested reviewer invokable this run — verdict against the AD critique dimensions)

| Dim | Dimension | Verdict | Note |
|-----|-----------|---------|------|
| 1 | Happy-path bias | **PASS** | 9/19 = 47% error/edge (≥40%): empty-before-ingest ×4, isolation −ve ×3, read-auth fail-closed, bind-conflict fail-closed. |
| 2 | GWT compliance | **PASS** | Each scenario one Given/When/Then in the doc-comment; one driving action each. |
| 3 | Business-language purity | **PASS** | Gherkin uses send/query/tenant/metric/log/trace/restart; transport (HTTP/200/protobuf/`Arc`) lives only in step helpers, not in the Given/When/Then prose. |
| 4 | Coverage completeness | **PASS** | Every US-01..05 AC maps to ≥1 scenario (mapping above). |
| 5 | WS user-centricity | **PASS** | WS titles are user goals ("a metric is queryable immediately after it is sent", "every signal sent is queryable back live"); Then-steps assert user observations (the metric/log/trace comes back), not DB rows. |
| 6 | Priority validation | **PASS** | Targets the single load-bearing gap (live visibility); WS first, isolation guardrail second, capstone last — the story-map priority order. |
| 7 | Observable-behaviour assertions | **PASS** | Assertions read the query RESPONSE (status success, result length, value present, 401-refusal) and the bound addresses — observable outcomes from driving ports, never internal store state or `Arc` identity directly. |
| 8 | Traceability | **PASS** | Check A: every story tagged. Check B: the `clean`/`with-pre-commit`/`ci` environments in `devops/environments.yaml` all run the same ephemeral-port in-process suite; the suite's Given ("the consolidated runtime is running … empty pillar root") names the environment precondition. |
| 9 | WS boundary proof | **PASS** | Strategy = single-process in-process with REAL adapters: the WS builds the real `FileBacked*Store`s + real aperture ingest + real axum query servers over loopback. No `@in-memory` anywhere; deleting a real adapter would red the WS. Every driven adapter (pulse/lumen/ray ingest+query, aperture ingest) is exercised with real I/O by the live-visibility scenarios. |

**Approval (self-review): approved.** Blockers: 0. High: 0. One soft-gate WARNING recorded
(product `kpi-contracts.yaml` absent; freshness KPI sourced from `outcome-kpis.md` +
`environments.yaml`, `@kpi` scenario present).

### Mandate compliance evidence (CM-A..D)

- **CM-A (hexagonal)**: the suite imports ONLY the driving entry
  `kaleidoscope_runtime::{spawn_consolidated, ConsolidatedConfig, RunningRuntime}` + the wire
  clients (`reqwest`, `opentelemetry-proto`); zero imports of internal store/router/sink
  components for direct exercise (the stores are reached only through the running ingest/query
  ports). The one `aegis` import is to MINT a read-auth validator fixture (a precondition), not
  to exercise an internal component.
- **CM-B (business language)**: Gherkin doc-comments carry no transport jargon; `grep -i` for
  "200/HTTP/protobuf/Arc/Mutex/axum" in the `Scenario:` prose returns nothing (helpers hold it).
- **CM-C (walking skeletons)**: 2 WS (#1 metrics, #18 three-signal), both demo-able user goals;
  17 focused scenarios.
- **CM-D (pure-function/fixture)**: no environment-matrix fixture parametrisation; the suite uses
  one real adapter tier (the live runtime). Pure OTLP-body builders are extracted to
  `tests/common/mod.rs` (`encode_metric_request_count_one`, `encode_log_checkout_failed`,
  `encode_trace_span`) and response-shape readers (`metrics_result_len`, `array_len`, …) are pure
  functions over the response body.

---

## Handoff to DELIVER (`nw-software-crafter`)

- **Outer loop**: `crates/kaleidoscope-runtime/tests/slice_01_live_metrics.rs` then
  `tests/slice_02_live_logs_traces.rs`. Implement `spawn_consolidated` (the entry above), then
  un-`#[ignore]` and green ONE scenario at a time, in table order (WS #1 first).
- **First target**: `metric_is_queryable_immediately_after_it_is_sent` (#1) — the north-star
  guard; once it is GREEN the shared-`Arc` bet is proven on the simplest signal.
- **Verify the no-concurrency-change premise** holds in practice (ADR-0076 DD2) and add the
  freshness measurement variant (the fsync-in-lock watch-item, `environments.yaml`
  `fsync_in_lock_watch_item`) — measure, do not pre-optimise.
- **Do NOT** touch the four existing composition roots (additive, DD5). Run per-feature mutation
  (`gate-5-mutants-kaleidoscope-runtime`, 100% kill) after refactor.
