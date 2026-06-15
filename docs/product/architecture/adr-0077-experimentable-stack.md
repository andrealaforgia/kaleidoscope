# ADR-0077: The experimentable stack — one-command compose, same-origin Prism, a spark-based telemetry generator

## Status

Accepted (DESIGN wave, `experimentable-stack-v0`, items C2 + C3 + C4 of the
consolidation roadmap — the remainder of Milestone 1). Author: Morgan
(`nw-solution-architect`), 2026-06-14, PROPOSE mode, autonomous overnight run.
Read-only grounding on `main` (no Bash available; every claim below is either
cited to source read this run or named explicitly as a DELIVER/DEVOPS
must-verify).

Builds on ADR-0076 (`consolidated-runtime-v0`, the C1 spine — the `kaleidoscope`
binary, crate `kaleidoscope-runtime`). This ADR designs the run story, the
generator, and the docs that wrap that binary into "one command, send, see".

## Context

C1 made the consolidated runtime exist: one process binding OTLP ingest
(gRPC 4317 / HTTP 4318) and the three query routers (metrics 9090 / logs 9091 /
traces 9092) over one shared store per signal, so a metric sent at T is queryable
at T with no restart. What is still missing (consolidation state assessment
`docs/analysis/consolidation-state-2026-06.md` §3/§7) is the newcomer-facing
loop: there is no `docker-compose.yml`, no `Makefile`, no run script, no
telemetry generator, and the README quick start documents only the CLI NDJSON
path. A newcomer cannot get to "a running stack in my browser with data in it"
without hand-plumbing.

The DISCUSS wave (`discuss/wave-decisions.md`) fixed the constraints: build on
the consolidated runtime not the old five binaries (W1); strictly additive (W2);
minimal-friction local posture — one tenant `acme`, auth off, no TLS, one shared
pillar volume (W3); and an explicit honest verification limit — CI can exercise
the HTTP loop but does not drive a browser to confirm Prism paints a chart (W6).
DISCUSS handed DESIGN five flags to resolve: F1 (a `Dockerfile.runtime`), F2
(compose + wrapper shape), F3 (the generator + seed), F4 (the Prism serving
topology — load-bearing), F5 (what CI exercises).

Quality attributes that drive this design (ISO 25010): **usability /
installability** (one command to a running, non-empty stack) is the primary
driver; **portability** (runs on any Docker host, no local Rust/Node toolchain)
and **reliability** (idempotent bring-up, clear failure on port conflict, no
half-up stack) are the secondary drivers. There is no new performance,
scalability, or security surface — auth is off by local design (W3) and the only
external substrate is the local filesystem (the reused fsync-honesty probe,
ADR-0049). There is no deploy target: Kaleidoscope deploys nothing; operators run
the stack locally.

## Decision

### F4 (load-bearing) — serve Prism same-origin from the metrics router; no separate Prism service, no CORS

The consolidated runtime serves Prism's built bundle from the **same origin** as
the metrics query API on port 9090, by pointing `KALEIDOSCOPE_QUERY_STATIC_DIR`
at the Prism `dist/`. Prism's relative `backend.url` (`/api/v1`) is then answered
by the same origin, so there is **no CORS, no Prism config change, and no
separate Prism web service**.

This is **confirmed by reading, end to end** (not assumed, not flagged for
DELIVER):

1. `query_api::router(store, tenant, static_dir: Option<PathBuf>)` mounts a
   `tower-http` `ServeDir` (with an `index.html` SPA fallback) as the router's
   fallback service when `static_dir` is `Some`, and the **exact**
   `/api/v1/query_range` route wins over the static fallback
   (`crates/query-api/src/lib.rs:122-180`).
2. `spawn_consolidated` passes `config.static_dir.clone()` into the metrics
   `query_api::router_with_auth(metric_dyn, metrics_tenant, read_auth,
   static_dir)` (`crates/kaleidoscope-runtime/src/lib.rs:340-345`).
3. The production binary resolves the env var into that field:
   `static_dir: non_empty_env("KALEIDOSCOPE_QUERY_STATIC_DIR").map(PathBuf::from)`
   (`crates/kaleidoscope-runtime/src/main.rs:114`).
4. Prism ships `backend.url = "/api/v1"` relative
   (`apps/prism/public/config.json`), and `vite build` copies `public/` into
   `dist/` (`apps/prism/package.json` build script `tsc -b && vite build`).

So the wiring already exists in shipped code. The **only** new work for the
serving topology is packaging: a Prism `dist/` must be present in the runtime
image and the env var must point at it. There is **no DELIVER code change** to
the runtime or query-api for same-origin Prism; it is a Dockerfile/compose
concern (F1/F2). The standalone `query-api` already documents this same pattern
(`Dockerfile.query-api` header; README "Status").

### F1 — add `Dockerfile.runtime` (multi-stage), additive alongside the existing three

A new `Dockerfile.runtime` builds the `kaleidoscope` binary AND bakes in the
Prism bundle, mirroring the existing multi-stage pattern
(`Dockerfile.query-api`):

- **Stage 1 (rust builder)** `rust:1.88-slim-bookworm`: `cargo build --release -p
  kaleidoscope-runtime --locked`.
- **Stage 2 (prism builder)** a Node image: `pnpm install && pnpm build` in
  `apps/prism`, producing `dist/` (carries `config.json` with the relative
  backend).
- **Stage 3 (runtime)** `debian:bookworm-slim`: carries only the compiled
  `kaleidoscope` binary plus the Prism `dist/` (e.g. at `/srv/prism`). `ENV
  KALEIDOSCOPE_PILLAR_ROOT=/data`, `ENV KALEIDOSCOPE_QUERY_STATIC_DIR=/srv/prism`,
  `EXPOSE 4317 4318 9090 9091 9092`, `VOLUME ["/data"]`, entrypoint the binary.

The three existing Dockerfiles (`Dockerfile`, `Dockerfile.gateway`,
`Dockerfile.query-api`) are untouched (W2 / US-07). DEVOPS authors the file.

### F2 — one runtime service + one named volume; a Makefile wrapper

Compose topology: **one** service (the consolidated runtime, built from
`Dockerfile.runtime`) plus **one** shared named volume mounted at `/data`
(`KALEIDOSCOPE_PILLAR_ROOT`). Ports `4317:4317 4318:4318 9090:9090 9091:9091
9092:9092` published. Env `KALEIDOSCOPE_TENANT=acme`, auth off. No separate Prism
service (F4 same-origin). A one-shot seed service is the only other compose entry
(F3, below). The runtime is the **sole writer** of the volume (one-writer
constraint, C1/W7) — compose must not co-run a separate gateway on the same
volume.

The wrapper is a **Makefile** (chosen over justfile: `make` is ubiquitous and
needs no extra tooling install; there is no existing `justfile`/`Makefile` at the
repo root, confirmed this run, so this is a fresh additive file). Targets:

| Target | Behaviour |
|--------|-----------|
| `make up` | `docker compose up -d` then wait for the query endpoints to answer |
| `make down` | `docker compose down` — **preserves** the named volume (durable telemetry survives) |
| `make demo` / `make seed` | run the telemetry generator once against the running stack |
| `make logs` | `docker compose logs -f` |
| `make clean` | `docker compose down -v` — **clears** the named volume (fresh empty stack) |

Idempotency comes from compose reconciling to the desired state (a second `make
up` is a no-op on a healthy stack). A required host port already in use surfaces
as a compose bind error; the Makefile presents it legibly and leaves no half-up
stack (US-03). DEVOPS authors the compose file and the Makefile.

### F3 — a new `kaleidoscope-telemetrygen` bin crate using `spark` (dogfooded OTLP); a marker-gated one-shot seed

The generator is **a new first-party bin crate `crates/kaleidoscope-telemetrygen`
built on `spark`** (DISCUSS option 2 — Luna's recommendation, and the one that
matches the "built from scratch, not assembled" principle). It dogfoods
Kaleidoscope's own SDK and emits real OTLP across all three signals.

Confirmed by reading `crates/spark/src/init.rs`: `spark::init` constructs all
three OTel SDK providers (`TracerProvider`, `LoggerProvider`, `SdkMeterProvider`)
over OTLP/gRPC, sets `tenant.id` as a resource attribute via
`SparkConfig::with_tenant_id`, honours `OTEL_EXPORTER_OTLP_ENDPOINT`
(default `http://localhost:4317`), and force-flushes every signal synchronously
when the guard drops. The generator therefore:

- calls `spark::init(SparkConfig::for_service("kaleidoscope-demo")
  .with_tenant_id("acme"))` with the endpoint pointed at the runtime's gRPC
  ingest (`4317`);
- emits, via the standard `opentelemetry` global API (spark deliberately
  re-exports no OTel types, so the crate depends on `opentelemetry` directly):
  a `request_count` counter, a log body `"checkout failed: card declined"`, and a
  coherent checkout-shaped span `"POST /api/v1/checkout"` (carrying that
  checkout failure as an Error status) under trace id
  `4bf92f3577b34da6a3ce929d0e0e4736` — the **C1 sample vocabulary**
  (`shared-artifacts-registry.md`), with the span name made checkout-coherent so
  the failing span and its checkout-failure cause tell one story;
- drops the guard to force-flush all three signals before exit.

**Earned Trust (principle 12) — the generator must probe, not assume.**
`spark::init` validates only that the endpoint URL parses; it does **not** probe
connectivity, and the OTLP batch exporter is fire-and-forget, so against a *down*
stack the export fails silently. US-04 AC explicitly requires the generator to
"fail with a clear, actionable message and not hang or exit silently" against a
stack that is not up. Therefore the generator **must perform an explicit
pre-flight reachability probe** (a TCP connect to the ingest port, or a cheap
HTTP GET against a query endpoint) before pushing, and exit non-zero with a clear
message naming the unreachable endpoint if the stack is down. This probe is a
first-class design responsibility, not an afterthought: every dependency the
generator does not probe is an act of faith made on the user's behalf, and the
running stack is exactly such a dependency. DELIVER implements the probe.

**Seed (US-05).** A one-shot compose service runs the generator once and exits,
gated by a **marker file on the shared volume** (e.g. `/data/.seeded`): if the
marker is absent, seed then create it; if present, skip. This satisfies both the
first-look-not-empty outcome and the once-only constraint (a restart does not
re-seed and does not accumulate duplicate copies — US-05 AC3). **Documented
fallback** (Luna's US-05 technical note): if DEVOPS finds the marker-gate too
heavy, fold US-05 into "run `make demo` as the documented first step" (US-06) —
this must be a recorded decision, never a silent drop of the not-empty-first-look
outcome. DELIVER builds the generator; DEVOPS wires the seed service + marker
gate (or the documented fallback).

### F5 — CI smokes the HTTP loop only; the browser render stays manual (W6)

CI exercises **the HTTP loop only**: bring the stack up headless, run the
generator, curl the three query endpoints, and assert the pushed telemetry comes
back (`request_count` from 9090, the log from 9091, the span from 9092 including
by-id). CI does **not** drive a browser to confirm Prism paints a chart (project
memory: Prism ECharts needs a CI-browser; `p95_wallclock_flakes_overnight` warns
off timing-shaped CI gates). The smoke is CI **feedback, not a hard gate** (pure
trunk-based; `kaleidoscope_pure_trunk_based`). Any automated bring-up avoids
binding the fixed 4317/4318/9090/9091/9092 where it can, and applies sweep/retry
(fixed-port flake discipline, `aperture_fixed_port_4317_flake`). The honest limit
— "Prism paints in the browser" is manual / smoke-verified, never a hard CI gate
— is stated here, in the getting-started docs (US-06), and in `outcome-kpis.md`.
DELIVER/DEVOPS author the smoke script.

### Additive constraint (W2 / US-07)

The whole feature is additive. The four standalone binaries
(`kaleidoscope-gateway`, `query-api`, `log-query-api`, `trace-query-api`), the
three existing Dockerfiles, and the CLI NDJSON quick start all keep building and
working. `Dockerfile.runtime`, the compose file, the Makefile, the generator
crate, and the getting-started docs are added, never replacing the existing
assets. The CLI quick start is preserved and remains discoverable (relabelled, if
the consolidated path becomes the primary quick start). Enforced by the existing
workspace build gate (`cargo build --workspace`) plus the manual/smoke Docker
builds.

## Alternatives considered

### A1 (F4) — serve Prism as a separate static web service with CORS (REJECTED)

Run Prism's `dist/` from its own static server on its own port (a second compose
service), set Prism's `backend.url` to the runtime's absolute metrics URL
(`http://localhost:9090/api/v1`), and configure CORS on the metrics router.
**Rejected**: it requires a Prism `config.json` edit (against the relative-URL
default shipped in `apps/prism/public/config.json`), introduces a CORS surface
the metrics router does not have today, and adds a second container — all to
reproduce what the already-shipped `static_dir` seam does for free. The
same-origin path (F4) is strictly simpler, CORS-free, config-change-free, and is
confirmed working in shipped code. A1 would only be revisited if a future Prism
needed to query *multiple* backends across origins (not a v0 concern).

### A2 (F3) — extend `kaleidoscope-cli` with an OTLP-over-the-wire push subcommand (REJECTED)

**Rejected**: the CLI today writes NDJSON directly to lumen/cinder; it is not an
OTLP client, so this is genuinely new client-side OTLP work, not a small flag —
and it would push the OTLP client dependency graph into the operator CLI, which
has no other reason to carry it. A focused generator crate keeps that dependency
where it belongs and dogfoods `spark`.

### A3 (F3) — wire the external `telemetrygen` (otel-collector-contrib) into compose (REJECTED)

**Rejected**: it adds an external image and sits squarely against the README's
"built from scratch, not assembled" principle. Defensible for a dev-only
generator, but it forgoes the dogfooding value of exercising Kaleidoscope's own
SDK over real OTLP, and adds a third-party supply-chain edge to the demo path.
The `spark`-based generator gives the same three-signal coverage with first-party
code.

### A4 (F3) — a curl-of-protobuf helper against 4318 (REJECTED)

**Rejected**: hand-encoding OTLP protobuf is brittle and unpleasant to maintain;
a poor newcomer and contributor experience for zero benefit over the `spark`
path.

### A5 (F2) — a justfile instead of a Makefile (REJECTED)

**Rejected**: `just` is a separate tool a newcomer must install first, which
works against the one-command minimal-friction goal; `make` is ubiquitous on the
target platforms. No functional difference for these five targets.

## Consequences

### Positive

- One command (`make up`) brings up a running, browser-reachable, non-empty stack
  with no auth ceremony — the Milestone 1 outcome.
- Same-origin Prism means zero CORS surface, zero Prism config drift, and one
  fewer container; it reuses a seam already proven in shipped code.
- The generator dogfoods `spark` and the platform's own OTLP path across all
  three signals, reusing the C1 sample vocabulary so "send" and "see" line up.
- Strictly additive: no existing binary, Dockerfile, or quick start regresses.
- The honest verification limit is stated in three places; the feature cannot
  overclaim the browser experience as CI-verified.

### Negative / trade-offs

- `Dockerfile.runtime` is a three-stage build (rust + node + debian), heavier than
  the single-language existing Dockerfiles; the Node/pnpm stage is new to the
  Docker build surface. Mitigated by `.dockerignore` discipline and layer caching;
  the alternative (a separate Prism service) trades this for a CORS surface and a
  config edit, which is worse.
- The seed's once-only behaviour depends on a marker file on the shared volume;
  if a user `make clean`s (clears the volume) the marker is also cleared and the
  next `up` re-seeds, which is the intended behaviour. The documented `make demo`
  fallback exists if the marker-gate proves fiddly.
- The generator's reachability probe is essential to the "fail clearly against a
  down stack" AC; without it the silent batch exporter would make a down-stack run
  look like a success. This is called out as a DELIVER must-implement, not left to
  chance.

### Earned Trust discharge

No new driven adapter with a new substrate is introduced for the runtime (F4/F1/F2
reuse C1's already-probed stores and the already-probed fsync substrate; the
composition-root "wire → probe → use" invariant is C1's, unchanged). The **one**
new probe obligation this feature creates is the generator's pre-flight
reachability probe against the running stack (F3) — specified above as a
first-class requirement with a clear fault scenario (stack down ⇒ clear non-zero
failure, never a silent success). The CI HTTP smoke (F5) is the behavioural proof
that the send→query loop actually closes against a real composed stack, which is
Earned Trust applied to the run story itself.

## External integrations

**None requiring contract tests.** The generator is a first-party OTLP client
pointing at Kaleidoscope's own ingest; Prism is first-party and served
same-origin. The only external substrates are the local filesystem (covered by
the reused fsync-honesty probe, ADR-0049) and Docker/compose as the orchestration
runtime. No consumer-driven contract test (Pact or similar) is recommended,
consistent with ADR-0076.

## Enforcement

- **Additive guardrail**: `cargo build --workspace` keeps the four standalone
  binaries compiling; the manual/smoke Docker builds of the three existing
  Dockerfiles confirm they still build (US-07).
- **The send→see loop**: the CI HTTP smoke (F5) is the executable proof the
  generator-then-query loop returns the pushed telemetry for all three signals.
- **Sample-vocabulary agreement**: the generator, the seed, the docs, and the
  smoke all use `acme`, `request_count`, the declined-checkout log, and the
  trace id `4bf92f3577b34da6a3ce929d0e0e4736` (shared-artifacts-registry); a drift
  shows as an empty chart, which the smoke catches for the HTTP-testable signals.
- **Honest-claim discipline**: the W6 limit is recorded in the ADR, the docs, and
  the KPIs; "Prism paints in the browser" is never asserted as a CI gate.

## References

- ADR-0076 (`consolidated-runtime-v0`, the C1 spine this wraps).
- DISCUSS: `docs/feature/experimentable-stack-v0/discuss/` (wave-decisions F1-F5
  + W1-W6, user-stories US-01..US-07, shared-artifacts-registry, outcome-kpis).
- Source read this run: `crates/query-api/src/lib.rs:122-180`;
  `crates/kaleidoscope-runtime/src/lib.rs:340-345`;
  `crates/kaleidoscope-runtime/src/main.rs:114`; `apps/prism/public/config.json`;
  `apps/prism/package.json`; `crates/spark/src/init.rs`; `crates/spark/src/lib.rs`;
  `Dockerfile.query-api`;
  `docs/feature/consolidated-runtime-v0/devops/environments.yaml`.
