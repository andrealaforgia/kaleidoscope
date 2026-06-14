# Wave Decisions — `experimentable-stack-v0` (DESIGN)

> **Wave**: DESIGN (`nw-solution-architect` / Morgan).
> **Date**: 2026-06-14. Autonomous overnight run.
> **Mode**: Decision 0 scope = APPLICATION (with infra topology); Decision 1 = PROPOSE.
> **Feature**: `experimentable-stack-v0` — items C2 + C3 + C4 (the remainder of Milestone 1) on top of the DONE, CI-green C1 consolidated runtime.
> **Decision record**: **ADR-0077** (`docs/product/architecture/adr-0077-experimentable-stack.md`).
> **Brief section**: `docs/product/architecture/brief.md` → `## Application Architecture — experimentable-stack-v0` (C4 deployment diagram + the send→see loop).
> **Grounding read on `main`, 2026-06-14** (read-only; no Bash available — every claim is from reading source or named as a DELIVER/DEVOPS must-verify). British English, no em-dashes.

---

## What this feature is

C1 (`consolidated-runtime-v0`) made the runtime exist: one `kaleidoscope` process binding OTLP ingest (4317/4318) + the three query routers (9090/9091/9092) over one shared store per signal. This feature wraps it into "one command, send, see": a compose run story (C2), a telemetry generator + seed (C3), and honest getting-started docs (C4). At the end Milestone 1 is met.

---

## DESIGN decisions (resolving DISCUSS flags F1-F5 — full rationale in ADR-0077)

| # | Flag | Choice | One-line rationale |
|---|------|--------|--------------------|
| F4 | Prism serving topology (LOAD-BEARING) | **Same-origin**: serve Prism `dist/` from the metrics router on 9090 via `KALEIDOSCOPE_QUERY_STATIC_DIR`; relative `/api/v1` just works | **Confirmed by reading, end to end** — the seam already ships; no CORS, no Prism config change, no separate service, no DELIVER code change. Separate-service+CORS (A1) rejected. |
| F1 | `Dockerfile.runtime` | **New three-stage** (rust build + node/pnpm prism build + debian runtime) baking the bin + Prism `dist/`; additive alongside the existing three | Mirrors `Dockerfile.query-api`; the node stage is the only new build surface, the price of same-origin Prism in one image. |
| F2 | compose + wrapper | **One runtime service + one named volume; a Makefile** (up/down/demo/seed/logs/clean) | One process = one writer by construction; `make` needs no extra tooling (no justfile/Makefile exists today, confirmed). `down` preserves the volume, `clean` clears it. |
| F3 | generator + seed | **New `crates/kaleidoscope-telemetrygen` bin on `spark`**; one-shot marker-gated seed service | Dogfoods built-from-scratch OTLP across all three signals; `spark` confirmed to build Tracer/Logger/Meter providers. CLI-extend (A2), external telemetrygen (A3), curl-protobuf (A4) rejected. |
| F5 | what CI exercises | **HTTP smoke loop only** (up → endpoints answer → generate → query returns), feedback not gate; browser paint stays manual | Per W6; project memory (Prism needs CI-browser; p95 flake discipline; pure trunk-based). |

### F4 — confirmed-by-reading (the load-bearing topology)

The same-origin path is **not** flagged for DELIVER to verify — it is already true in shipped code:

1. `query_api::router(store, tenant, static_dir: Option<PathBuf>)` mounts a `tower-http` `ServeDir` (with `index.html` SPA fallback) as the fallback service when `static_dir` is `Some`, and the exact `/api/v1/query_range` route wins over it (`crates/query-api/src/lib.rs:122-180`).
2. `spawn_consolidated` passes `config.static_dir.clone()` into the metrics `router_with_auth(metric_dyn, metrics_tenant, read_auth, static_dir)` (`crates/kaleidoscope-runtime/src/lib.rs:340-345`).
3. The binary resolves the env var: `static_dir: non_empty_env("KALEIDOSCOPE_QUERY_STATIC_DIR").map(PathBuf::from)` (`crates/kaleidoscope-runtime/src/main.rs:114`).
4. Prism ships `backend.url = "/api/v1"` relative (`apps/prism/public/config.json`); `vite build` copies `public/` into `dist/` (`apps/prism/package.json`).

The **only** new work for serving topology is packaging: get a Prism `dist/` into the runtime image and point `KALEIDOSCOPE_QUERY_STATIC_DIR` at it. No runtime or query-api code change. The standalone `query-api` already documents this same pattern (`Dockerfile.query-api`; README "Status").

### F3 — Earned Trust: the generator must probe the stack (load-bearing for US-04)

`spark::init` validates only that the endpoint URL parses; it does NOT probe connectivity, and the OTLP batch exporter is fire-and-forget (`crates/spark/src/init.rs`). Against a down stack a naive generator would export into the void and exit 0 — contradicting US-04 ("fail clearly, do not hang or exit silently"). DELIVER MUST add an explicit pre-flight reachability probe (TCP connect to the ingest port, or a cheap query-endpoint GET) and exit non-zero with a clear message if the stack is unreachable. This is principle 12 applied to the run story.

---

## MANDATORY Reuse Analysis

Everything load-bearing is REUSE. The CREATE-NEW set is wiring/packaging + a small generator, each justified.

| Component / seam | Disposition | Source (read on `main`, or rationale) | Justification |
|------------------|-------------|----------------------------------------|---------------|
| The consolidated runtime (`kaleidoscope` bin) | **REUSE, unchanged** | `crates/kaleidoscope-runtime/src/{lib,main}.rs` (ADR-0076) | The whole run story brings up this one binary; C2/C3/C4 add nothing to it. |
| Same-origin Prism static serving | **REUSE, unchanged** | `query_api::router(.., static_dir)` → `ServeDir` SPA fallback (`query-api/src/lib.rs:122-180`); wired through `spawn_consolidated` (`lib.rs:340-345`) + the env var (`main.rs:114`) | THE F4 seam. Already ships; set the env var, no code change, no CORS. |
| Prism bundle | **REUSE, unchanged** | `apps/prism` (`config.json` relative `/api/v1`; `pnpm build` → `dist/`) | Served same-origin; no config edit. Logs/traces panels stay out of scope (roadmap C5). |
| OTLP SDK for the generator | **REUSE** | `spark::init` builds Tracer/Logger/Meter providers over OTLP/gRPC, sets `tenant.id`, honours `OTEL_EXPORTER_OTLP_ENDPOINT`, force-flushes on drop (`crates/spark/src/init.rs`, `lib.rs`) | All three signals over real OTLP with one init; dogfoods the platform's own SDK. |
| Sample-data vocabulary | **REUSE** | C1 sample data via `shared-artifacts-registry.md` | `acme`, `request_count`, the declined-checkout log, trace id `4bf92f3577b34da6a3ce929d0e0e4736` — one vocabulary across generator, seed, docs, smoke. |
| Multi-stage Dockerfile pattern | **REUSE (pattern)** | `Dockerfile.query-api` (rust:1.88-slim-bookworm builder → debian:bookworm-slim runtime) | `Dockerfile.runtime` mirrors it, adding a node/pnpm prism-build stage. |
| Tenancy / fsync substrate / probes | **REUSE, unchanged** | C1 runtime's reused `aegis` tenancy + fsync-honesty probe (ADR-0049) + read probes | No new substrate at the runtime; the one new probe obligation is the generator's reachability probe (F3). |
| **`Dockerfile.runtime`** | **CREATE-NEW** | DEVOPS authors | No Dockerfile builds `kaleidoscope-runtime` today (the three build cli/gateway/query-api). Needed to ship the consolidated bin + Prism `dist/` in one image. Additive (W2/US-07). |
| **The compose file** | **CREATE-NEW** | DEVOPS authors | No `docker-compose.yml` exists (confirmed this run). The one-command run story's spine: one runtime service + one named volume + a one-shot seed. |
| **The Makefile** | **CREATE-NEW** | DEVOPS authors | No `Makefile`/`justfile` exists (confirmed). The thin one-command wrapper (up/down/demo/seed/logs/clean). `make` over `just` (no tooling install). |
| **`crates/kaleidoscope-telemetrygen` bin** | **CREATE-NEW** | DELIVER authors | No first-party OTLP demo generator exists; the CLI writes NDJSON not OTLP. A focused `spark`-based bin keeps the OTLP client dep out of the operator CLI and dogfoods the SDK. |
| **Getting-started docs (C4)** | **CREATE-NEW** | DELIVER authors (S3) | The README documents only the CLI demo; the consolidated path is undocumented. Honest about the W6 limit. |

**CREATE-NEW count: 5** (one Dockerfile, one compose, one Makefile, one small generator bin, one docs section). No new domain concept, store, port, query contract, or external integration. Every other element is reuse.

---

## For Acceptance Designer (Quinn, DISTILL)

### Driving entries (where to exercise behaviour)

- **`make up`** (over `docker compose up`) — brings up the runtime over an empty named volume, Prism served same-origin on 9090. The documented browser URL is `http://localhost:9090`.
- **The query GETs** — `GET :9090/api/v1/query_range?query=request_count&start=..&end=..` (metrics), `GET :9091/api/v1/logs?..` (logs), `GET :9092/api/v1/traces?..` and `GET :9092/api/v1/traces/by_id?..` (traces). Same contracts as C1.
- **The generator command** — `make demo` (or `make seed`) runs `kaleidoscope-telemetrygen`, pushing the sample OTLP for `acme` to ingest gRPC `:4317`.
- **The pre-existing assets** (US-07) — `cargo build --workspace` (the four standalone binaries) and the three existing Docker builds; the CLI NDJSON quick start.
- **Honest limit (W6)**: the HTTP loop is the CI-testable, gateable-as-feedback core. "Prism paints `request_count` in the browser" is manual / smoke-verified, NEVER a hard CI gate. Any automated bring-up avoids fixed-port binding where possible and sweeps/retries (`aperture_fixed_port_4317_flake`).

### What each acceptance criterion asserts

- **one-command-brings-the-stack-up** (US-01) — after the bring-up command, the runtime is running and the documented URL serves Prism (browser part manual/smoke per W6).
- **endpoints-answer-empty-success-before-any-telemetry** (US-01) — right after bring-up, metrics/logs/traces queries each return `{status:success, ... result:[]}` over HTTP 200, never an error or connection refusal.
- **runtime-and-prism-share-one-volume-and-tenant** (US-01) — a metric ingested for `acme` is returned by the query Prism issues over the same volume + tenant; no separate store, no path mismatch.
- **prism-served-by-the-consolidated-metrics-router** (US-02) — Prism's metrics query is answered by the 9090 router (same-origin); empty stack shows an honest empty state, unreachable backend shows a clear message (these two browser states are the manual/smoke part per W6).
- **fresh-checkout-comes-up-clean** (US-03) — a fresh `make up` creates an empty volume; endpoints return empty success; no stale state.
- **second-up-is-idempotent** (US-03) — a second bring-up on a running stack stays healthy; nothing duplicated or corrupted.
- **down-then-up-returns-to-a-working-stack** (US-03) — `make down` preserves the volume; `make up` returns to a working stack with prior telemetry present.
- **port-already-in-use-fails-clearly** (US-03) — a required host port in use causes a clear, named error and no half-up stack.
- **one-command-pushes-all-three-signals** (US-04, the HTTP smoke loop) — after the generator runs once, a metrics query returns `request_count`, a logs query returns `"checkout failed: card declined"`, and a traces query (window AND by-id, trace id `4bf92f3577b34da6a3ce929d0e0e4736`) returns the span — all for `acme`. **This is the load-bearing CI-testable assertion: up → generate → query returns the data.**
- **prism-paints-the-sample-metric** (US-04) — after the generator, Prism renders `request_count` as a series (browser-verified per W6, NOT a CI gate).
- **generator-against-a-down-stack-fails-clearly** (US-04) — run before the stack is up, the generator exits non-zero with a clear message that the ingest endpoint is unreachable, and does not hang or exit silently (the Earned-Trust pre-flight probe, F3).
- **re-running-the-generator-is-safe** (US-04) — a second run succeeds without error; queries still return telemetry.
- **first-look-not-empty** (US-05) — after `make up`, sample telemetry is present without the user running a separate command (the one-shot seed); scoped to `acme`; the seed runs once and does not duplicate on every restart (marker-gated). If the team adopts the documented `make demo` fallback instead, that is a recorded decision, not a silent drop.
- **docs-complete-the-loop** (US-06) — a cold reader follows the getting-started section (one command up, send, see a metric in Prism, query a log and a trace, minimal config) end to end unaided; the section states the W6 verification limit plainly and points CLI users to the preserved CLI demo.
- **pre-existing-paths-still-work** (US-07) — the four standalone binaries still build/run; the three existing Dockerfiles still build; the CLI NDJSON quick start still works and stays discoverable.

### Slice map (confirmed for DELIVER, matches DISCUSS W4)

- **Slice 1 — C2 run story**: US-01/02/03. The compose file + `Dockerfile.runtime` + the Makefile bring up the runtime with Prism same-origin over the shared volume; endpoints answer; idempotent/clean/port-conflict behaviours. Feature walking skeleton.
- **Slice 2 — C3 generator + seed**: US-04/05. The `kaleidoscope-telemetrygen` crate (with the reachability probe) + the marker-gated one-shot seed; the HTTP smoke loop turns green.
- **Slice 3 — C4 docs + guardrail**: US-06/07. The getting-started section (honest about W6) + the additive guardrail.

---

## Wave ownership (who writes what)

- **DEVOPS (`platform-architect`, Apex)** writes: `Dockerfile.runtime` (F1), the compose file + named volume + Makefile targets up/down/demo/seed/logs/clean (F2), the one-shot seed service + marker gate (F3 seed mechanism), and the CI HTTP smoke (F5). No deploy target.
- **DELIVER (`nw-software-crafter`)** writes: the `crates/kaleidoscope-telemetrygen` bin crate on `spark` + `opentelemetry`, including the mandatory pre-flight reachability probe (F3 generator), and the getting-started docs (C4). **No runtime/query-api code change is needed for same-origin Prism (F4 already shipped).**
- **DISTILL (`acceptance-designer`, Quinn)** writes: the Gherkin scenarios per the "For Acceptance Designer" section above + `discuss/user-stories.md`. The CI-testable core is the HTTP loop; the browser paint is manual/smoke (W6).

---

## External integrations

**None requiring contract tests.** The generator is a first-party OTLP client against Kaleidoscope's own ingest; Prism is first-party and served same-origin. The only external substrates are the local filesystem (reused fsync-honesty probe, ADR-0049) and Docker/compose as the orchestration runtime. No consumer-driven contract test recommended (consistent with ADR-0076).

---

## Back-propagation to DISCUSS

**None.** No DISCUSS assumption changed: build-on-C1 (W1), additive (W2), minimal-friction local posture (W3), three-slice scope (W4/W5), and the honest verification limit (W6) are all designed to as stated. The F1-F5 flags are resolved within those constraints. `design/upstream-changes.md` is therefore intentionally absent.

> Note on US-05: the once-only seed is delivered via a marker file on the shared volume, with the documented `make demo`-first fallback explicitly preserved (Luna's US-05 technical note). This is a DESIGN choice within the DISCUSS outcome, not a changed assumption, so it is recorded here rather than back-propagated.

---

## Self-review (no nested reviewer invoked this run — recorded verdict against the SA critique dimensions)

| Dimension | Verdict | Note |
|-----------|---------|------|
| Reuse Analysis present + every CREATE-NEW justified | **PASS** | 5 CREATE-NEW (Dockerfile, compose, Makefile, generator bin, docs), each justified; all load-bearing parts REUSE. |
| C4 diagram present | **PASS** | C4 deployment/Container diagram in the brief (runtime serving ingest+query+Prism-static, the shared volume, the generator + one-shot seed pushing OTLP); every arrow verb-labelled. |
| ADR alternatives incl the CORS path rejected | **PASS** | A1 (separate-Prism-service + CORS) rejected with rationale; A2/A3/A4 (CLI / external telemetrygen / curl-protobuf) and A5 (justfile) also rejected. |
| Same-origin-Prism mechanism confirmed-or-flagged | **PASS (confirmed by reading)** | F4 cited end to end: `query-api/src/lib.rs:122-180`, `kaleidoscope-runtime/src/lib.rs:340-345`, `main.rs:114`, `apps/prism/public/config.json`. No DELIVER code change; packaging only. |
| Additive constraint | **PASS** | W2/US-07; four binaries + three Dockerfiles + CLI demo untouched; enforced by `cargo build --workspace` + smoke Docker builds. |
| Honest CI/browser limit | **PASS** | F5/W6 stated in ADR, brief, and (to be) the docs; browser paint never a hard CI gate. |
| Earned Trust (principle 12) | **PASS** | The one new substrate dependency (the running stack, from the generator's side) gets a mandatory pre-flight reachability probe with an explicit down-stack fault scenario; runtime substrate reuses C1's probes. |
| Resume-driven / bias check | **PASS** | No microservices, no broker, no extra tooling; one container + one volume + a Makefile + a small bin. Simplest solution that meets the one-command-send-see need. |
| Priority validation (largest bottleneck) | **PASS** | Targets the exact Milestone-1 gap named by the assessment §3/§7 (no run story, no generator, no consolidated docs); simpler alternatives documented and rejected. |
| No overstated claims | **PASS** | F4 confirmed by reading (not overclaimed); the generator's silent-export limitation surfaced honestly and turned into a probe requirement; the browser limit stated three times. |

**Approval (self-review): approved.** Critical issues: 0. High issues: 0. One watch-item recorded for DELIVER: the generator's pre-flight reachability probe is load-bearing for US-04's down-stack AC and must not be skipped.

---

## Handoff

- **DISTILL (`acceptance-designer`, Quinn)**: the "For Acceptance Designer" section above + the per-story Gherkin in `discuss/user-stories.md` are the scenario SSOT. The HTTP smoke loop (up → generate → query returns) is the CI-testable core; the browser paint is manual/smoke (W6).
- **DEVOPS (`platform-architect`, Apex)**: F1/F2/F5 + the seed mechanism (F3) are yours; `outcome-kpis.md` carries the time-to-first-telemetry-seen target and the HTTP-smoke-vs-browser-manual split. No deploy target. No external-integration contract tests.
- **DELIVER (`nw-software-crafter`)**: build `crates/kaleidoscope-telemetrygen` (on `spark` + `opentelemetry`, with the mandatory reachability probe) and the C4 getting-started docs. No runtime/query-api change for same-origin Prism.
