# Wave Decisions — `experimentable-stack-v0` (DISTILL), Slice 2 / C3

> **Wave**: DISTILL (`nw-acceptance-designer` / Quinn).
> **Date**: 2026-06-14. Autonomous overnight run.
> **Feature**: `experimentable-stack-v0` — **Slice 2 (C3)**: the telemetry
> generator (`kaleidoscope-telemetrygen`) + its acceptance suite + RED-ready
> scaffold. Slice 1 (C2 run story) and Slice 3 (C4 docs) are out of this run.
> **Grounds**: DESIGN `design/wave-decisions.md` "## For Acceptance Designer" +
> ADR-0077 (F3, the generator); DISCUSS `discuss/user-stories.md` US-04/US-05.
> **Outputs**:
> - `crates/kaleidoscope-telemetrygen/` (scaffold crate: `Cargo.toml` +
>   `src/lib.rs` + `src/main.rs`), added to the root workspace members.
> - `crates/kaleidoscope-telemetrygen/tests/slice_02_send_to_see.rs` (the
>   acceptance suite, all `#[ignore]`d).
> British English, no em-dashes, no emoji.

---

## The outcome under test

A `spark`-based bin `kaleidoscope-telemetrygen` pushes sample OTLP
metrics + logs + traces to a RUNNING consolidated runtime, and that telemetry
becomes QUERYABLE — the send-to-see loop closes end to end through the REAL
OTLP/gRPC wire and the LIVE shared store. And: run against a DOWN stack it
fails CLEARLY (the mandatory pre-flight reachability probe, US-04 / ADR-0077
F3), never silent fire-and-forget.

---

## Test architecture

### Driving port: the COMPILED BIN as a subprocess

The generator's true driving port is the CLI invocation. The full-push
scenarios drive the COMPILED BIN
(`env!("CARGO_BIN_EXE_kaleidoscope-telemetrygen")`) as a real subprocess
(`tokio::process::Command`), pointed at the runtime's bound ingest gRPC port
via `OTEL_EXPORTER_OTLP_ENDPOINT` + `KALEIDOSCOPE_TENANT`. This is `@real-io`
`@adapter-integration`: real subprocess, real OTLP/gRPC transport, real live
store. It is also what makes the suite faithful to `spark`'s
**single-init-per-process** invariant — each run is its own process with
pristine spark/OTel global state, so "re-run is safe" is literally two
processes, exactly as a user re-runs the command. An in-process
`generate()`-loop could not express that without colliding on the OTel global
provider.

### Live runtime: REUSE the C1 composition root on EPHEMERAL ports

The suite stands up a live consolidated runtime IN THE TEST PROCESS via
`kaleidoscope_runtime::spawn_consolidated(ConsolidatedConfig::for_ephemeral_test(..))`
(a dev-dependency). The runtime serves the generator's pushes (in-process
ingest + shared `Arc<Store>`) and answers the "see" queries — one process, no
store drop/reopen between send and query. C1 is DONE and implemented, so the
spawn is GREEN; only the generator is RED.

### The probe contract is ALSO driven directly

The pre-flight reachability probe is locked twice: once as the library seam
`kaleidoscope_telemetrygen::probe_reachable(endpoint)` (a focused contract
scenario, callable in-process without spark), and once through the real bin
(the driving-port equivalent). Both assert the down-stack failure is a clear,
named unreachability report.

### EPHEMERAL ports — no fixed-port bind

Every runtime binds `127.0.0.1:0` for all five listeners; the actual bound
ports are read back from `RunningRuntime`. The fixed 4317/4318/9090/9091/9092
defaults are NEVER bound (project memory `aperture_fixed_port_4317_flake`). The
generator endpoint is always `http://{bound_grpc_addr}`. Confirmed by reading
the suite: the only literal ports are `127.0.0.1:0` (ephemeral) and the
window-bound integers `0`/`9999999999` (query time window, not ports).

### "See" asserts business outcomes

The query helpers GET the three routers over loopback and assert the sample
telemetry returns (`request_count` series present; the log body present; the
span returned by service window AND by trace id) — never transport details or
internal state.

---

## Scenario map

5 scenarios, all `@US-04`, all `#[ignore]`d. The error/edge share is well over
the 40% mandate.

| # | Scenario | Tags | Category | Drives | RED reason (right-reason proof) |
|---|----------|------|----------|--------|---------------------------------|
| 1 | Generated telemetry is queryable across all three signals | `@walking_skeleton @driving_port @real-io @adapter-integration` | happy / north star | bin subprocess + 3 query routers | scaffold bin exits non-zero -> "generator exits cleanly" fails (no push) |
| 2 | The reachability probe reports a clear failure when the endpoint is down | `@infrastructure-failure` | error | `probe_reachable()` library seam | scaffold returns the `__SCAFFOLD__` marker, not an unreachability report -> "clearly names unreachability" fails |
| 3 | Running the generator against a down stack fails clearly, not silently | `@infrastructure-failure @real-io` | error | bin subprocess vs a closed port | bin prints the scaffold marker, not an unreachability message -> the message assertion fails (NOT merely the non-zero check) |
| 4 | Generated telemetry is present for its tenant and invisible to another | `@driving_port @real-io @adapter-integration` | edge / isolation (pos + neg) | bin subprocess + two runtimes | the PRESENT half (acme visible to acme) fails first (no push) — anchors RED so the negative-absence half is never a trivial pass |
| 5 | Re-running the generator is safe and the telemetry stays queryable | `@real-io @adapter-integration` | edge | bin subprocess x2 | first run exits non-zero -> "both runs succeed" fails |

Error/edge: #2 and #3 are pure error (2/5 = 40%); #4 and #5 are edge. Error +
edge = 4/5 = 80%.

### Story coverage

- **US-04** — fully covered: one-command-pushes-all-three-signals (#1),
  generator-against-a-down-stack-fails-clearly (#2 contract + #3 bin),
  re-running-the-generator-is-safe (#5), plus tenant scoping (#4, the W3
  single-tenant posture made falsifiable as positive + negative).
- **US-05** (first-look-not-empty, the once-only seed) — DELIBERATELY OUT of
  this in-process suite. Per ADR-0077 F3 the seed is a COMPOSE concern (a
  marker-gated one-shot service on the shared volume) verified by the CI HTTP
  smoke, not by an in-process Rust test. Recorded here so the omission is a
  decision, not a silent drop.
- **prism-paints-the-sample-metric** (US-04 browser AC) — NOT automated (W6:
  Prism ECharts needs a CI-browser; manual/smoke only).

---

## The contract DELIVER implements

The scaffold compiles and exposes the seam DELIVER fills. Both functions
currently return `GenError::Scaffold { operation }` (the RED-not-BROKEN
placeholder).

```rust
pub struct GenConfig { pub endpoint: String, pub tenant: String, pub service_name: String }
pub struct GenSummary { pub metrics_pushed: u64, pub logs_pushed: u64, pub spans_pushed: u64 }

pub enum GenError {
    Unreachable { endpoint: String, detail: String },   // the load-bearing US-04 variant
    ExportFailed { endpoint: String, detail: String },
    InvalidConfig { detail: String },
    Scaffold { operation: &'static str },               // DELIVER deletes every return of this
}

pub async fn probe_reachable(endpoint: &str) -> Result<(), GenError>;
pub async fn generate(config: GenConfig) -> Result<GenSummary, GenError>;
```

DELIVER work (ADR-0077 F3):

1. **`probe_reachable`** — parse host:port out of the OTLP endpoint and do a
   bounded TCP connect (or a cheap query-endpoint GET); return
   `GenError::Unreachable { endpoint, .. }` whose `Display` names the endpoint
   and the word "unreachable" when the stack is down. The bin's stderr message
   must also tell the user to "bring the stack up" / "make up" (scenario #3
   asserts both phrases).
2. **`generate`** — after a successful probe, `spark::init(
   SparkConfig::for_service(&config.service_name).with_tenant_id(&config.tenant))`
   pointed at `config.endpoint`; emit via the global `opentelemetry` API a
   `request_count` counter, a `checkout failed: card declined` log, and a
   `GET /api/v1/query_range` span under trace id
   `4bf92f3577b34da6a3ce929d0e0e4736` (the C1 sample vocabulary, reused
   verbatim, service `kaleidoscope-demo`); drop the guard to force-flush all
   three signals synchronously; return the `GenSummary`.

The bin (`src/main.rs`) is a thin shell already written: it resolves
`OTEL_EXPORTER_OTLP_ENDPOINT` / `KALEIDOSCOPE_TENANT` / `OTEL_SERVICE_NAME`,
runs the probe FIRST (clear non-zero exit on failure), then `generate`. DELIVER
fills only the two library seams.

---

## In-process vs CI-smoke split (ADR-0077 F5 / W6)

- **In-process (this suite)** — the CI-testable core of the send-to-see loop:
  push via the real bin over the real OTLP wire, query the live store, assert
  the data returns. Plus the down-stack clear-failure, tenant scoping, and
  safe re-run.
- **CI HTTP smoke (DEVOPS)** — the SAME loop against a real composed stack
  (`make up` -> generate -> curl the three query endpoints), feedback not gate,
  and the once-only SEED (US-05).
- **Manual / browser** — "Prism paints `request_count`" is never a CI gate
  (W6). Not represented in this suite.

---

## RED-not-BROKEN proof (from RUNNING)

Commands run on `main`, `2026-06-14` (cargo via `~/.cargo/bin`):

1. **Compiles** — `cargo build -p kaleidoscope-telemetrygen` -> `Finished`.
2. **Scenarios discovered + ignored** —
   `cargo test -p kaleidoscope-telemetrygen --test slice_02_send_to_see -- --list`
   -> `5 tests`. Default run -> `test result: ok. 0 passed; 0 failed; 5
   ignored` (trunk stays green; the scaffold is committable).
3. **RED via `--ignored`** —
   `cargo test -p kaleidoscope-telemetrygen --test slice_02_send_to_see --
   --ignored --test-threads=1` -> `test result: FAILED. 0 passed; 5 failed`.
   Each failure is a BUSINESS reason, not a compile/setup error:
   - #1, #4, #5 fail because the scaffold bin exits non-zero (the probe seam
     returns `__SCAFFOLD__ ... not yet implemented`) so no telemetry is pushed;
   - #2 fails because the probe returns the scaffold marker instead of an
     unreachability report;
   - #3 fails on the unreachability MESSAGE assertion (not the non-zero check),
     proving the test is not satisfied by any non-zero exit.
   The C1 runtime SPAWNED successfully in every scenario (the reuse seam is
   live), so the RED is isolated to the generator under test.
4. **Lib suite green** — `cargo test --workspace --lib --locked` -> every
   `test result:` line reports `0 failed` (committable green).
5. **Clippy clean** — `cargo clippy -p kaleidoscope-telemetrygen --all-targets`
   -> no warnings, no errors.

No fixed port is bound anywhere in the suite (only `127.0.0.1:0` ephemeral
binds; the only integer literals are the query time window).

---

## Self-review (acceptance-designer critique dimensions)

| Dimension | Verdict | Note |
|-----------|---------|------|
| 1 Happy-path bias | **PASS** | 2/5 pure error + 2/5 edge = 80% non-happy; only #1 is a pure happy path. |
| 2 GWT compliance | **PASS** | Each scenario is a single Given/When/Then with one action; the Gherkin lives in the doc-comment above each test. |
| 3 Business language | **PASS** | Gherkin uses "telemetry", "tenant", "queryable", "bring the stack up" — no HTTP/gRPC/status codes. (Transport lives in the Layer-3 helpers, not the scenario text.) |
| 4 Coverage completeness | **PASS** | US-04 fully mapped; US-05 explicitly scoped to the compose seed + CI smoke (recorded, not dropped). |
| 5 Walking-skeleton user-centricity | **PASS** | #1's title is a user goal ("telemetry becomes queryable across all three signals"); Then-steps are user observations (the metric/log/span return), not internal side effects. |
| 6 Priority validation | **PASS** | The send-to-see loop is the exact Milestone-1 gap C3 fills; the reachability probe is the load-bearing US-04 down-stack guarantee. |
| 7 Observable-behaviour assertions | **PASS** | Every Then asserts a query-router return value or a process outcome (exit status + user-facing message); no private state, no DB-row poke, no file-existence check. |
| 8 Traceability | **PASS (A)** / **N/A (B)** | Every scenario tagged `@US-04`; US-05 dispositioned. Environment-to-scenario (Check B) is a compose/DEVOPS concern for this in-process suite. |
| 9 Walking-skeleton boundary proof | **PASS** | #1 is `@real-io @adapter-integration`: real subprocess + real OTLP wire + real live store. Litmus "if I deleted the real adapter would it still pass?" — NO: deleting the real push makes #1 RED (proven above). No `@in-memory` anywhere. |
| Mandate 1 (hexagonal) | **PASS** | Full-push scenarios drive the compiled BIN (the CLI driving port); the probe scenario drives the public `probe_reachable` contract. No internal component imported. |
| Mandate 7 (RED-not-BROKEN, no Fixture Theater) | **PASS** | Given-steps set up only preconditions (a running/absent runtime); no scenario passes without the generator's production code. The tenant-absence negative is anchored by a present-half that fails first, so it is never a trivial green. Proven RED above. |

**Approval (self-review): approved.** Blockers: 0. High: 0. Watch-item for
DELIVER: pinning the span to trace id `4bf92f3577b34da6a3ce929d0e0e4736` via
the OTel API likely needs a custom span context / id generator — it is an
explicit ADR-0077 commitment ("reused verbatim"), and scenario #1's by-id
assertion locks it.

---

## Handoff

- **DELIVER (`nw-software-crafter`)**: implement `probe_reachable` and
  `generate` (see "The contract DELIVER implements"); enable the scenarios one
  at a time (remove `#[ignore]`, make GREEN, commit, repeat), starting with the
  probe contract (#2) then the walking skeleton (#1). Run mutation before the
  first DELIVER commit (project memory). Then the C4 getting-started docs.
- **DEVOPS (`platform-architect`)**: the CI HTTP smoke and the marker-gated
  one-shot seed service (US-05) — the compose-side half of the send-to-see loop
  this suite does not cover in-process.
