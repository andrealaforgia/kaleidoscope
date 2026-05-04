# KPI Instrumentation — `aperture` v0 (DEVOPS)

> **Wave**: DEVOPS (`nw-platform-architect` / Apex).
> **Date**: 2026-05-04.
> **Author**: Apex.
> **Source of truth for KPIs**:
> [`docs/feature/aperture/discuss/outcome-kpis.md`](../discuss/outcome-kpis.md).
> **CI workflow**: `.github/workflows/ci.yml`.
> **Companion documents**: `wave-decisions.md`, `platform-architecture.md`,
> `observability-design.md`, `monitoring-alerting.md`.

---

## Framing

Aperture is a service. Its eight outcome KPIs split into three
categories:

1. **Build-time corroborated** — KPI 1, KPI 6 (and partly 2, 3, 4,
   7): a CI test asserts the runtime invariant the KPI measures, so
   passing the test corroborates that the runtime would be
   well-behaved. The actual runtime ratios are operator-side.
2. **Runtime ratios** — KPI 2, KPI 4, KPI 7 (and partly 3, 5, 8): the
   metric is a ratio observable from the operator's stderr-log
   aggregator over a rolling window. Aperture emits the data; the
   operator computes the ratio.
3. **Release-cadence load tests** — KPI 5, KPI 8: a 1-hour load test
   (KPI 5) and a 1000-restart scenario (KPI 8) are too expensive for
   per-commit CI but cheap enough for per-release. Deferred to a
   future release wave per `wave-decisions.md > A8/Q5`.

Aperture has **no `/metrics` endpoint at v0** (DISCUSS Q6,
ADR-0009). Every KPI's data source is therefore one of:

- a CI test outcome (build-time);
- the operator's stderr-log aggregation (runtime);
- a release-cadence CI artefact (deferred).

This document specifies, for each of the eight KPIs, the data source,
the collection mechanism, the storage location, the reading cadence,
and the alerting (if any). North-star and guardrail markings come
from `outcome-kpis.md`'s metric hierarchy; they are not relitigated
here.

---

## North star

> **Acceptance latency for a valid OTLP/gRPC logs export ≤ 50 ms p99
> under non-overload conditions, with refusal-not-drop guarantee under
> overload conditions.**

The north-star metric splits along the same three categories:

- **Latency p99 ≤ 50 ms** — runtime; operator-side measurement
  (per-request latency observable from operator's request-trace
  pipeline if any, or as `latency_ms` field on `sink_accepted`
  stderr lines once DELIVER lands the field per ADR-0009 +
  `component-design.md`). Future Pulse phase (Phase 4) may add a
  `/metrics` endpoint exporting a histogram if operators report the
  stderr-aggregation approach insufficient.
- **Refusal-not-drop guarantee** — build-time corroborated; KPI 6's
  `@property`-tagged test in `slice_05_backpressure.rs` defends it
  per-commit; KPI 5's release-cadence load test defends it under
  realistic-volume conditions.

The north star is therefore both build-time defended (refusal-not-
drop) and runtime-observable (latency); neither is the responsibility
of the other.

---

## Per-KPI specification

### KPI 1 — Walking-skeleton round-trip

> **Binary milestone**: 100% of the documented Slice-01 demo command
> sequence completes without manual intervention.

| Aspect | Specification |
|---|---|
| Type | Binary (per-commit pass/fail) |
| Data source | `crates/aperture/tests/slice_01_walking_skeleton.rs` (13 active tests) |
| Collection | Gate 1 (`cargo test`) post-graduation per `wave-decisions.md > A2`. The 13 tests under `slice_01_walking_skeleton.rs` go RED on day one (DISTILL D7) and GREEN as DELIVER's Slice 01 lands; thereafter every commit asserts the walking-skeleton path holds. |
| Build-blocking? | Yes (every commit, post-graduation) |
| Operator-side runtime measurement | None at v0. Operators verify by running the documented Slice-01 demo command (`docs/feature/aperture/slices/slice-01-walking-skeleton.md`) on their own deployment if desired. |
| Storage | GitHub Actions run history; per-commit test pass/fail visible in the Actions UI. |
| Reading cadence | Every CI run. |
| Alert threshold | Any test failure fails Gate 1, blocks merge (post-graduation). |
| Dashboard | None (binary milestone). |
| Owner | DEVOPS (CI infrastructure); Aperture (test fixtures). |

This KPI is the project-level walking-skeleton tripwire (DISCUSS
KPI 1; DISTILL Mandate CM-C). It graduates from binary to ratio
("how many SDK clients successfully round-tripped a logs export this
quarter?") in Phase 1 when pilot operators are running Aperture; the
v0 contract is binary because v0 has no production deployments yet.

---

### KPI 2 — Transport-coverage acknowledgement ratio

> **gRPC OK / HTTP 200 ratio ≥ 99% under non-overload conditions.**

| Aspect | Specification |
|---|---|
| Type | Runtime ratio (operator-side) |
| Data source | Operator's stderr aggregation: `count(sink_accepted) / count(request_received)` per transport, 5-minute rolling window. |
| Collection (build-time corroboration) | `slice_01_walking_skeleton.rs::customer_exports_one_log_record_and_receives_grpc_ok` (gRPC); `slice_02_http_protobuf_and_readiness.rs::customer_posts_valid_logs_body_and_receives_status_200` (HTTP). Per-commit assertion that the success-path returns OK / 200. |
| Build-blocking? | Yes (the slice tests are the build-time defence; the runtime ratio is operator-side) |
| Operator-side runtime measurement | Operator's log query: `count_over_time({app="aperture"} \| json \| event="sink_accepted") / count_over_time({app="aperture"} \| json \| event="request_received") [5m]`, grouped by `transport`. |
| Storage | Operator's log aggregator (Loki, ELK, Splunk, journald, etc.). |
| Reading cadence | Every 5 minutes (operator-side); sample query plates documented in `observability-design.md > Operator query patterns`. |
| Alert threshold | Operator-side: ratio drops below 95% sustained for 5 minutes → page (per `monitoring-alerting.md`). |
| Dashboard | Operator-side. Sample dashboard panel queries provided as prose in `monitoring-alerting.md`. |
| Owner | Operator (runtime); Aperture (event vocabulary) + DEVOPS (CI corroboration). |

The build-time corroboration is the contract that the runtime ratio
holds **assuming Aperture's accept-path is correct**. The runtime
ratio degrading below 95% indicates either (a) a downstream incident
(ForwardingSink failing) or (b) an Aperture-internal regression that
the slice tests should have caught. Either way, alerting is
operator-side.

---

### KPI 3 — Readiness three-state machine

> **100% of CI test runs assert `/readyz` returns 200 only when both
> listeners are bound AND not draining; survey-based confirmation
> from at least 3 pilot operators 30 days post-Phase-1 launch.**

| Aspect | Specification |
|---|---|
| Type | Build-time structural + qualitative survey |
| Data source (structural) | `crates/aperture/tests/slice_02_http_protobuf_and_readiness.rs` (15 tests; the readiness-state UAT scenarios assert `/readyz` 200 when both listeners bound + 503 during startup or shutdown drain); `crates/aperture/tests/slice_08_graceful_shutdown.rs::shutdown_flips_readyz_to_503_draining_within_100ms`. |
| Data source (survey) | A 5-question form sent to each pilot operator 30 days post-Phase-1 launch: "is /readyz what you probe?", "do you parse stderr?", "did you customise drain_deadline?", "did you adjust max_concurrent_requests?", "any unexpected behaviour during rolling restarts?". |
| Collection (structural) | Gate 1 post-graduation; the slice tests run on every commit. |
| Collection (survey) | Manual (Andrea conducts the interviews 30 days post-launch; quarterly thereafter). |
| Build-blocking? | Yes (structural slice tests); no (survey). |
| Storage | GitHub Actions run history (structural); a future `docs/evolution/<yyyy-qN>-aperture-pilot-operator-survey.md` (survey; file does not exist yet, first quarter creates it). |
| Reading cadence | Every CI run (structural); quarterly (survey). |
| Alert threshold | Structural test failure → blocks merge. Survey responses are reviewed at retrospective time, not alerted on. |
| Dashboard | None at v0; the survey is a qualitative input to the next-iteration backlog. |
| Owner | DEVOPS (CI structural); Andrea (survey, post-launch). |

The structural side is the load-bearing defence (every commit asserts
the state machine); the survey is the longitudinal validation that
operators are using `/readyz` as designed.

---

### KPI 4 — Per-signal acknowledgement ratio

> **Per-signal acknowledgement ratio ≥ 99% under non-overload, all
> three signals lit.**

| Aspect | Specification |
|---|---|
| Type | Runtime ratio (operator-side) |
| Data source | Operator's stderr aggregation: per-signal `count(sink_accepted) / count(request_received)` ratio, 5-minute rolling. The `signal` field is a literal `logs` / `traces` / `metrics` per the closed event vocabulary (DISCUSS D1). |
| Collection (build-time corroboration) | `slice_03_traces.rs` (10 tests), `slice_04_metrics.rs` (9 tests), `slice_01_walking_skeleton.rs` (logs). Each per-signal slice asserts both the accept-path and the reject-path in the closed-vocabulary stderr lines. |
| Build-blocking? | Yes (slice tests); the runtime ratio is operator-side. |
| Operator-side runtime measurement | Operator's log query: `count_over_time({app="aperture"} \| json \| event="sink_accepted" \| signal=~"logs|traces|metrics") / count_over_time({app="aperture"} \| json \| event="request_received" \| signal=~"logs|traces|metrics") [5m]`, grouped by `signal`. |
| Storage | Operator's log aggregator. |
| Reading cadence | Every 5 minutes (operator-side). |
| Alert threshold | Operator-side: any per-signal ratio drops below 95% sustained for 5 minutes → page (per `monitoring-alerting.md`). |
| Dashboard | Operator-side. |
| Owner | Operator (runtime); Aperture (event vocabulary) + DEVOPS (CI corroboration). |

Same shape as KPI 2, decomposed by signal. The runtime ratio drift
on a single signal indicates a per-signal regression (e.g. a malformed
trace export pattern from a specific SDK version).

---

### KPI 5 — Concurrency saturation events

> **Refusal-rate equals exceeded-cap-rate; zero silent drops in a
> 1-hour load test at 2x cap.**

| Aspect | Specification |
|---|---|
| Type | **Release-cadence load test** (deferred to future release wave per `wave-decisions.md > A8/Q5`); per-commit corroboration via slice 05. |
| Data source (per-commit corroboration) | `crates/aperture/tests/slice_05_backpressure.rs` — 10 tests including the `@property`-tagged `every_excess_request_under_overload_receives_a_deterministic_refusal_or_acceptance`. |
| Data source (release cadence) | A future `crates/aperture/tests/load_kpi5_overload.rs` (`#[ignore]`-tagged so per-commit CI skips; release CI invokes with `--ignored`) that pumps 2x cap concurrent gRPC requests for 1 hour (or compressed cadence) and asserts `count(request_received) − count(sink_accepted) − count(reject_*) − count(concurrency_cap_hit) == 0`. |
| Collection (per-commit) | Gate 1 post-graduation. |
| Collection (release cadence) | Future release-wave CI gate: `cargo test -p aperture --test load_kpi5_overload --ignored`. |
| Build-blocking? (per-commit) | Yes |
| Build-blocking? (release cadence) | Yes (when the gate is wired) |
| Storage | GitHub Actions run history (per-commit); a future `load-test-report.json` artefact at release-cadence with per-second event counts. |
| Reading cadence | Every CI run (slice 05); every release (load test). |
| Alert threshold | Slice 05 test failure → blocks merge. Load-test reconciliation non-zero → blocks release. |
| Dashboard | Operator-side: `count(concurrency_cap_hit) > 0` over a 5-min window → ticket (per DISCUSS handoff and `monitoring-alerting.md`). |
| Owner | DEVOPS (CI infrastructure); Aperture (test fixtures). |

The load test is the volume-shape corroboration of the slice-test
contract. v0 ships without the load test; the slice tests are the
defence at v0; the load test joins at the first release wave.

**KPI 5 release-cadence test sketch**:

```rust
// crates/aperture/tests/load_kpi5_overload.rs (FUTURE)
#![cfg(any())] // disabled at v0; release wave enables and adds #[ignore]

#[ignore = "release-cadence; expensive"]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn one_hour_at_two_x_cap_reconciles_to_zero_residual() {
    // start aperture with cap=8
    // pump 16 concurrent gRPC clients each sending 1 request/100ms for the configured duration
    // capture events: request_received, sink_accepted, reject_*, concurrency_cap_hit
    // assert: count(request_received) - count(sink_accepted) - count(reject_*) - count(concurrency_cap_hit) == 0
}
```

The compressed cadence (e.g. 5 minutes of pumping at adjusted rate
that matches the 1-hour scenario's request volume) keeps the test
within CI runner limits while preserving the reconciliation
invariant.

---

### KPI 6 — Refusal-not-drop invariant

> **100% of cap-exceeded requests carry a documented refusal status;
> 0% silent drops.**

| Aspect | Specification |
|---|---|
| Type | Build-time property assertion |
| Data source | `crates/aperture/tests/slice_05_backpressure.rs::every_excess_request_under_overload_receives_a_deterministic_refusal_or_acceptance` (the `@property`-tagged UAT). |
| Collection | Gate 1 post-graduation; the property test fires `N=10` concurrent requests against a `cap=2` instance and asserts every response is HTTP 200 / gRPC OK or HTTP 503 / gRPC RESOURCE_EXHAUSTED — never a connection drop, timeout, or any other status. |
| Build-blocking? | Yes |
| Operator-side runtime measurement | None directly; KPI 6 is a structural contract. The operator-observable expression of KPI 6 is "no silent drops in stderr count reconciliation": `count(request_received) ≈ count(sink_accepted) + count(reject_*) + count(concurrency_cap_hit)` over any rolling window. Any persistent imbalance is a contract violation worth investigating. |
| Storage | GitHub Actions run history. |
| Reading cadence | Every CI run. |
| Alert threshold | Test failure → blocks merge. |
| Dashboard | None directly; the KPI 5 dashboard's reconciliation count is the operator-side surface. |
| Owner | DEVOPS (CI); Aperture (test fixture). |

KPI 6 is the load-bearing non-silent-drop invariant; per `outcome-
kpis.md`'s metric hierarchy it is one of the three primary leading
indicators (KPIs 5, 6, 8). The build-time property test is the
defence; no operator-side dashboard is needed because the slice test
is structurally airtight.

---

### KPI 7 — Downstream-acceptance ratio

> **Downstream-acceptance ratio ≥ 99% under healthy-downstream
> conditions.**

| Aspect | Specification |
|---|---|
| Type | Runtime ratio (operator-side); build-time corroboration via Slice 06. |
| Data source (build-time corroboration) | `crates/aperture/tests/slice_06_forwarding_sink.rs` — 11 tests (3 walking-skeleton: probe success + downstream-receives + accepted-event-includes-downstream-endpoint; 2 focused: fall-back probe + latency_ms field; 6 error: probe-lies, 503-on-POST, refused, timeout, sink_failed, unreachable). |
| Data source (runtime ratio) | Operator's log query (in two parts): (a) Aperture's `count(sink_accepted) / count(sink_failed + sink_accepted)` per signal/transport; (b) cross-checked with the operator's downstream-side request-receive count. |
| Collection (build-time) | Gate 1 post-graduation. |
| Collection (runtime) | Operator-side, every 5 minutes. |
| Build-blocking? | Yes (slice tests); the runtime ratio is operator-side. |
| Storage | GitHub Actions run history (slice 06); operator's log aggregator (runtime). |
| Reading cadence | Every CI run (slice tests); every 5 minutes (runtime). |
| Alert threshold | Slice 06 test failure → blocks merge. Runtime: ratio drops below 95% sustained for 5 minutes → page (per `monitoring-alerting.md`). |
| Dashboard | Operator-side; sample queries in `monitoring-alerting.md`. |
| Owner | Operator (runtime); Aperture (event vocabulary; ForwardingSink) + DEVOPS (CI corroboration). |

The probe contract (ADR-0007 Earned-Trust) and the gold-test (Gate 8,
future) defend the startup-time contract; KPI 7 is the steady-state
runtime contract. Both are needed.

---

### KPI 8 — Graceful-restart drop ratio

> **Zero silent drops in a 1000-restart load test (offered load
> below capacity, downstream healthy).**

| Aspect | Specification |
|---|---|
| Type | **Release-cadence load test** (deferred per `wave-decisions.md > A8/Q5`); per-commit corroboration via slice 08. |
| Data source (per-commit corroboration) | `crates/aperture/tests/slice_08_graceful_shutdown.rs` — 5 active tests (clean drain emits `event=in_flight_drained`; deadline-exceeded emits warn line with dropped count; readyz flips to 503 within 100 ms; in-flight request completes when drain finishes within deadline; signal field on event matches transport) + 1 `#[ignore]`d SIGTERM equivalence test. |
| Data source (release cadence) | A future `crates/aperture/tests/load_kpi8_restart.rs` (`#[ignore]`-tagged) that loops spawn-handle-shutdown 1000 times under continuous offered load and asserts `count(request_received) == count(sink_accepted) + count(reject_*) + count(drain_deadline_exceeded_dropped_count)`. |
| Collection (per-commit) | Gate 1 post-graduation. |
| Collection (release cadence) | Future release-wave CI gate. |
| Build-blocking? (per-commit) | Yes |
| Build-blocking? (release cadence) | Yes (when the gate is wired) |
| Storage | GitHub Actions run history (per-commit); future `load-test-report.json` artefact (release cadence). |
| Reading cadence | Every CI run (slice 08); every release (load test). |
| Alert threshold | Slice 08 failure → blocks merge. Reconciliation imbalance in load test → blocks release. |
| Dashboard | None at v0; the load-test artefact is the release-time review surface. |
| Owner | DEVOPS (CI infrastructure); Aperture (test fixtures). |

Per `outcome-kpis.md`'s budget note: at Aperture's expected startup
time of ~50 ms per restart, 1000 restarts plus drain time fits in
roughly 5–10 minutes of wall-clock CI time. Adequate for release
cadence.

---

## Summary table

| KPI | Type | Data source category | Build-blocking? | Cadence | Storage |
|---|---|---|---|---|---|
| 1 | Binary milestone | Slice 01 test outcome | Yes (post-graduation) | Every commit | GitHub Actions run history |
| 2 | Runtime ratio + corroboration | Operator stderr + slice 01/02 | Yes (slice); operator (runtime) | Every commit (slice); every 5 min (runtime) | Run history + operator's log aggregator |
| 3 | Structural + survey | Slice 02/08 + pilot survey | Yes (slice); manual (survey) | Every commit (slice); quarterly (survey) | Run history + `docs/evolution/...` |
| 4 | Runtime ratio + corroboration | Operator stderr + slice 03/04 | Yes (slice); operator (runtime) | Every commit (slice); every 5 min (runtime) | Same as KPI 2 |
| 5 | Property + release load test | Slice 05 + future load test | Yes (slice); release (load test) | Every commit (slice); every release (load test) | Run history + future load-test artefact |
| 6 | Property | Slice 05 `@property` | Yes | Every commit | Run history |
| 7 | Runtime ratio + corroboration | Operator stderr + slice 06 | Yes (slice); operator (runtime) | Every commit (slice); every 5 min (runtime) | Same as KPI 2 |
| 8 | Property + release load test | Slice 08 + future load test | Yes (slice); release (load test) | Every commit (slice); every release (load test) | Run history + future load-test artefact |

Five of eight KPIs have build-time-only defences (1, 3, 6) or
build-time-defence-plus-future-load-test (5, 8). Three (2, 4, 7) are
runtime ratios that require operator-side measurement at v0.

---

## Comparison with the harness's KPI shape

The harness's seven KPIs were build-time / corpus-driven (5 of 7 CI-
output-driven, 1 external, 1 on-demand). Aperture's eight KPIs split
differently because Aperture has a runtime: 5 of 8 are build-time-
shaped (binary, structural, property — all asserted by per-commit
slice tests), 3 of 8 are runtime ratios (operator-side), and 2 of 8
have a future release-cadence component.

The KPI 4 verdict-counts artefact pattern from the harness's DEVOPS
wave **does not transfer** to Aperture: Aperture's KPIs are runtime
ratios computed over the operator's log stream, not corpus counts
computed over committed test vectors. The verdict-counts JSON pattern
is the right shape for "we have a corpus of inputs whose verdicts we
want to tally"; Aperture's KPIs are "we have a runtime whose event
ratios we want to track", which is the operator's job to report.

A future Pulse phase (Phase 4) may introduce a `/metrics` endpoint
that exports Aperture-side counters; at that point a Prometheus or
OTel Collector data feed becomes the central KPI machinery. v0
deliberately scopes that out (DISCUSS Q6, ADR-0009).

---

## Build-time KPI artefact

Aperture v0 produces **no Aperture-specific CI artefact** (in
contrast to the harness's `verdict-counts.json`). The reasons:

- KPIs 1, 3, 6 are binary (test-pass / test-fail); the GitHub
  Actions run history is the durable record.
- KPIs 2, 4, 7 are runtime ratios; the artefact would be empty at
  v0 because no production deployments exist on the Kaleidoscope
  side.
- KPIs 5, 8 are deferred to release cadence; the future release wave
  introduces a `load-test-report.json` artefact at that time.

This is structurally honest: introducing an empty CI artefact "for
completeness" would be ceremony without value, the same anti-pattern
the harness DEVOPS rejected for `cargo test`-only-without-Gates-2-3-5.

---

## Dashboards (operator-side)

There are no Kaleidoscope-side dashboards. The DISCUSS handoff
listed two dashboard categories:

1. **Real-time dashboards for KPIs 2, 4, 5, 7** — operational metrics
   on operators' existing log dashboards. The dashboard *queries* are
   simple counts/ratios over the closed `log_event_vocabulary`; the
   dashboard *infrastructure* is whatever the operator already runs.
   Sample queries are documented in `monitoring-alerting.md`.

2. **Weekly reports for KPIs 3, 6, 8** — quality-assurance metrics
   that belong in a release-cadence report, not a real-time
   dashboard. KPI 3's structural side is asserted every CI run (the
   release report is summarised, not generated; "all slice tests
   green this release" is the report). KPI 6's `@property` test is
   the same. KPI 8's release-cadence load test produces the
   load-test-report.json artefact at release time.

A future Pulse phase may produce a Kaleidoscope-side reference
Grafana JSON for operator-friendly adoption; v0 does not.

---

## Alerting

DISCUSS handoff named four alerting thresholds (operator-side, at
v0). Documented in `monitoring-alerting.md`:

1. `count(sink_accepted) / count(request_received)` per transport
   drops below 95% sustained for 5 min → page.
2. Any `/healthz` non-200 response → page (fatal invariant).
3. `count(concurrency_cap_hit) > 0` over a 5-min window → ticket.
4. Any new outbound network connection from Aperture beyond
   ForwardingSink → page (CI invariant `no_telemetry_on_telemetry`
   should have caught this; if it reaches production, that is a
   CI-gate failure, not just a production incident).

Aperture itself emits no alerts. Operators wire the queries into their
preferred alerting system.

---

## Baseline measurement

All eight KPIs are greenfield (per `outcome-kpis.md`). No baseline
collection needed before launch — Aperture v0 IS the baseline. After
30 days post-Phase-1 (when pilot operators are running real
deployments), the runtime KPIs (2, 4, 7) become longitudinal. KPI 3's
survey kicks in at the same milestone.

---

## Forward-compatibility

The KPI instrumentation in this wave is sized for v0 (Aperture's
service-shape, no Kaleidoscope-side runtime, no central telemetry).
When a future wave introduces:

- **Kaleidoscope-side runtime telemetry** (Pulse, Phase 4): KPIs 2,
  4, 7 graduate from operator-side runtime ratios to
  Pulse-aggregated metrics; a `/metrics` endpoint or OTLP-out
  becomes the data feed. The KPI definitions stay the same; the
  instrumentation changes.
- **Release pipeline** (Phase 1 or later): KPIs 5, 8 graduate from
  "deferred" to wired release-cadence gates per the schedule above.
- **Pilot-operator engagement** (Phase 1): KPI 3's survey side
  begins; the first quarterly evolution document is created.

Each forward-compatibility hook is named here so the future wave
reads the contract without re-deriving it.
</content>
</invoke>
