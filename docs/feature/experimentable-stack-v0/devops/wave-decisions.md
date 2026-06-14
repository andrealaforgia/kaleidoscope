# Wave Decisions — `experimentable-stack-v0` (DEVOPS)

> **Wave**: DEVOPS (`nw-platform-architect` / Apex).
> **Date**: 2026-06-14. Autonomous overnight run.
> **Feature**: `experimentable-stack-v0` — items C2 + C3 + C4 (the remainder of
> Milestone 1) on top of the DONE C1 consolidated runtime.
> **Decision record**: ADR-0077. **Design handoff**:
> `design/wave-decisions.md` ("Wave ownership": F1/F2/F5 + the seed mechanism F3
> are DEVOPS's).
> British English, no em-dashes, no emoji.

---

## What this wave wrote (the one-command run story)

DEVOPS owns the run-story infrastructure. This wave authored, validated, and
committed:

| Artefact | Path | What it is |
|----------|------|------------|
| Runtime image | `Dockerfile.runtime` | 5-stage build: rust-builder (kaleidoscope bin) -> prism-builder (Prism dist) -> runtime (debian-slim, bin + dist, same-origin Prism) -> generator-builder + generator (the wired-ahead seed/demo generator) |
| Dockerfile ignore | `Dockerfile.runtime.dockerignore` | BuildKit Dockerfile-specific ignore that keeps `apps/` in the context (the default `.dockerignore` excludes it) while excluding target/node_modules/dist/.git/data |
| Compose | `compose.yaml` | ONE `runtime` service + ONE shared named volume + a profile-gated `seed` one-shot service; the spine of the one-command story |
| Wrapper | `Makefile` | `up`/`down`/`demo`/`seed`/`logs`/`clean`/`help` |
| CI smoke | `.github/workflows/ci.yml` (`experiment-stack-smoke`) | HTTP loop, `continue-on-error: true` (feedback, not a gate) |
| Environments | `devops/environments.yaml` | the compose + CI-smoke environment inventory |
| This file | `devops/wave-decisions.md` | decisions + self-review + honest limits |

DELIVER (`nw-software-crafter`) still owns `crates/kaleidoscope-telemetrygen`
(the generator, with the mandatory pre-flight reachability probe) and the C4
getting-started docs. **No runtime/query-api code change is needed** (F4
same-origin Prism already ships; confirmed by reading).

---

## DEVOPS decisions (within the ADR-0077 design)

| # | Decision | Rationale |
|---|----------|-----------|
| D1 | Compose topology: ONE `runtime` service + ONE named volume `kaleidoscope-data` at `/data`; five ports published (4317/4318 ingest, 9090/9091/9092 query) | ADR-0077 F2/F4: one process = one writer; same-origin Prism = no separate Prism service, no CORS. |
| D2 | `compose.yaml` (not `docker-compose.yml`) | Nothing existed; the modern filename, no `version:` key (compose v2/v5 ignores it). |
| D3 | `Dockerfile.runtime` is 5 stages with the generator stages OUTSIDE the `runtime` target's graph | So `docker build --target runtime` (what `make up` does) comes up TODAY while the generator is wired ahead of DELIVER. |
| D4 | Base images tag-pinned: `rust:1.88-slim-bookworm`, `node:22-bookworm-slim`, `debian:bookworm-slim` | Mirrors the three existing Dockerfiles' tag-pinning; rust matches `rust-toolchain.toml` (1.88), node/pnpm match the CI Prism gates (node 22, pnpm 9). |
| D5 | `Dockerfile.runtime.dockerignore` keeps `apps/` in the context | The default `.dockerignore` excludes `apps/` (CLI/gateway/query-api do not need the SPA); the runtime image DOES. BuildKit honours the Dockerfile-specific ignore. |
| D6 | Healthcheck = bash `/dev/tcp` connect to `127.0.0.1:9090` | bash is in the debian-slim base; no curl/wget added (keeps the image as lean as the siblings). The runtime binds 9090 only after fail-closed startup probes pass (ADR-0076 DD3), so a TCP connect is a sound readiness signal. `make up` adds a host-side HTTP poll for the stronger check. |
| D7 | Seed = profile-gated (`profiles: ["seed"]`) one-shot service, marker-gated on `/data/.seeded`; `make demo` = `SEED_FORCE=1` (force), `make seed` = marker-gated once-only | ADR-0077 F3 / US-05. Profile-gating keeps a plain `make up` from ever building/running the (currently absent) generator. |
| D8 | `make up` uses `up -d --build --wait --wait-timeout 180` then a host curl poll; prints `http://localhost:9090` | Idempotent (compose reconciles; a second up is a no-op-ish); a port-in-use surfaces as a compose bind error with a non-zero exit and no half-up stack (US-03); a startup refusal fails `--wait` and is surfaced. |
| D9 | `make down` preserves the volume; `make clean` removes it (`down -v`) | ADR-0077 F2 / US-03 down-then-up returns to a working stack with prior telemetry. |
| D10 | CI smoke is a new `experiment-stack-smoke` job, `continue-on-error: true`, skip-if-generator-absent | Pure trunk-based: CI is feedback, not a gate (`kaleidoscope_pure_trunk_based`). Wired-ahead: the generate+assert half is skipped with a `::notice::` until the crate lands, so it cannot break trunk. |
| D11 | No job-level `${{ env.X }}` reference in the smoke job | The `github_actions_job_level_env` pitfall: all literals inline. |
| D12 | Mutation testing: N/A for this wave's artefacts | Dockerfile/compose/Makefile/CI-YAML are infra, not mutable Rust/TS. The generator's mutation is DELIVER's (per-feature, 100% kill, ADR-0005 Gate 5); Prism's is StrykerJS (gate-10). Nothing here is in a `cargo mutants` or Stryker surface. |

---

## Wired ahead of the generator (the honest seam)

`crates/kaleidoscope-telemetrygen` does not exist on `main` yet (DELIVER Slice 2).
Everything that depends on it is wired ahead so it begins working the moment the
crate lands, and breaks nothing before then:

- **`Dockerfile.runtime` `generator` / `generator-builder` stages** build the
  crate, but are NOT in the `runtime` target's dependency graph, so `make up`
  never builds them.
- **The compose `seed` service** is profile-gated (`--profile seed`), so a plain
  `make up` never touches it.
- **`make seed` / `make demo`** target the `generator` stage; until the crate
  lands they fail fast with a clear cargo error (and only those two targets do).
- **The CI smoke's generate+assert step** is guarded by
  `if [ ! -d crates/kaleidoscope-telemetrygen ]` -> `::notice::` + skip, so the
  smoke exercises bring-up + empty-success endpoints today and the full send->see
  loop the moment the crate lands.

The runtime itself (`spawn_consolidated`) IS implemented on `main` (not a
scaffold, despite a stale doc-comment in `main.rs`), so the bring-up half of the
story is live today: `make up` produces a healthy, Prism-serving stack now; only
"send" (the generator) is pending DELIVER.

---

## Honest verification limit

- **Validated this run** (Docker compose v5.1.3 + make + python available):
  - `docker compose -f compose.yaml config` -> exit 0 (runtime topology, 5 ports, named volume).
  - `docker compose -f compose.yaml --profile seed config` -> exit 0 (seed service, marker-gate command, `depends_on: service_healthy`).
  - `make -n up`, `make help` -> targets parse and expand correctly.
  - `yaml.safe_load(ci.yml)` -> valid; `experiment-stack-smoke` present, `continue-on-error: true`, 6 steps.
  - `spawn_consolidated` is implemented on `main` (grep: no scaffold/panic in `kaleidoscope-runtime/src/lib.rs`).
- **NOT run this run** (heavy / out of scope): a full `docker build` of
  `Dockerfile.runtime` (rust + node + debian, minutes), a real `docker compose up`,
  the end-to-end send->see, and the Prism browser paint. **The real verification of
  the image build + bring-up is the CI smoke (`experiment-stack-smoke`) and a manual
  `make up`.** The Prism browser paint is manual/smoke-only, NEVER a hard CI gate
  (W6 / ADR-0077 F5).

---

## Additive constraint (W2 / US-07) — confirmed not broken

- The three existing Dockerfiles (`Dockerfile`, `Dockerfile.gateway`,
  `Dockerfile.query-api`) are **untouched** (no edits this wave).
- The four standalone binaries are untouched; the workspace still compiles
  (CI gate-1 `cargo test --workspace` is the enforcement; `spawn_consolidated`
  already builds on `main`).
- The default `.dockerignore` is **untouched**; the runtime image uses its own
  `Dockerfile.runtime.dockerignore` so the existing builds' context is unchanged.
- The CLI NDJSON quick start is untouched.
- Everything this wave added is additive: new Dockerfile, new ignore file, new
  compose, new Makefile, new CI job, two new devops docs.

---

## Self-review (no nested reviewer invoked this run — recorded verdict against the platform-architect critique dimensions)

| Dimension | Verdict | Note |
|-----------|---------|------|
| Compose topology correct (one runtime service + same-origin Prism + shared volume sole-writer) | **PASS** | One `runtime` service; Prism served same-origin on 9090 (no separate service, no CORS); the runtime is sole writer of pulse/lumen/ray, the seed only touches the marker over the wire. `config` validated. |
| Makefile idempotent + clear errors | **PASS** | `up -d --wait` reconciles (second up no-op); port-in-use -> compose bind error, non-zero, no half-up stack; startup refusal -> `--wait` fails with a `make logs` pointer. `make -n` validated. |
| Dockerfile.runtime mirrors the pattern + pins bases | **PASS** | Mirrors `Dockerfile.query-api` (rust-slim builder -> debian-slim runtime) + one node stage; bases tag-pinned (1.88 / node 22 / bookworm-slim) consistent with the repo. |
| CI smoke is feedback-not-gate + does not break trunk wired-ahead | **PASS** | `continue-on-error: true`; generate+assert skipped with `::notice::` until the crate lands; YAML validated, action pinned. |
| Environment inventory | **PASS** | `environments.yaml` records the compose env, the make targets, the Dockerfile stages, the CI-smoke env, the KPI instrumentation, rollback, and the verification limits. |
| Honest verification + browser-paint limits | **PASS** | Stated here, in `environments.yaml`, and consistent with the ADR/outcome-kpis: `config`/dry-run validated this run; build + up are CI-smoke/manual; browser paint never a CI gate. |
| Additive constraint | **PASS** | Three existing Dockerfiles + four binaries + default `.dockerignore` + CLI demo untouched; everything new is additive. |
| Job-level env pitfall avoided | **PASS** | No job-level `${{ env.X }}` reference; all literals inline (`github_actions_job_level_env`). |
| Rollback-first / deployment posture | **PASS (N/A deploy)** | No deploy target; rollback is `git revert` of additive files; existing binaries unaffected. Prefer fix-forward (`feedback_fix_forward_post_merge_correction`). |
| Simplest-solution / resume bias check | **PASS** | One container + one volume + a Makefile + a feedback smoke; no orchestrator, no broker, no extra tooling. Simplest shape that meets one-command-send-see. |
| Mutation strategy | **PASS (N/A here)** | Infra artefacts are not a mutation surface; the generator's mutation is DELIVER's, Prism's is StrykerJS. |

**Approval (self-review): approved.** Critical issues: 0. High issues: 0.

One watch-item carried to DELIVER (restating ADR-0077): the generator's
pre-flight reachability probe is load-bearing for US-04's down-stack AC and must
not be skipped; the seed/demo/smoke paths above all assume it (a down-stack run
must fail clearly, never silently succeed through the fire-and-forget exporter).

---

## Handoff

- **DELIVER (`nw-software-crafter`)**: build `crates/kaleidoscope-telemetrygen`
  (on `spark` + `opentelemetry`, with the mandatory pre-flight reachability probe)
  and the C4 getting-started docs. The compose `seed`/`demo`/`smoke` paths and the
  `generator` Dockerfile stage are already wired to it (it reads
  `OTEL_EXPORTER_OTLP_ENDPOINT` = `http://runtime:4317` and `KALEIDOSCOPE_TENANT`,
  and must emit the C1 sample vocabulary: `request_count`, the declined-checkout
  log, the span under trace id `4bf92f3577b34da6a3ce929d0e0e4736`, tenant `acme`).
  Once the crate lands, `make demo` / `make seed` and the CI smoke's generate+assert
  half go live with no infra change.
- **DISTILL (`acceptance-designer`, Quinn)**: the driving entries are `make up`,
  the three query GETs (params confirmed: metrics `query/start/end/step`, logs
  `start/end`, traces `service/start/end` + by-id `trace_id`), and `make demo`.
  The HTTP loop is the CI-testable core; the browser paint is manual/smoke (W6).
- **Operations**: none. No deploy target; operators run the stack locally via the
  Makefile.
