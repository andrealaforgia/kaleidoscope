# Wave Decisions - `experimentable-stack-v0` (DISCUSS)

> **Wave**: DISCUSS (nw-product-owner / Luna).
> **Date**: 2026-06-14.
> **Author**: Luna, single-pass, autonomous overnight run.
> **Feature**: `experimentable-stack-v0` - the rest of Milestone 1 of the consolidation roadmap
> (items C2 + C3 + C4) on top of the DONE, CI-green C1 consolidated runtime.
> **Companion documents**: `user-stories.md`, `story-map.md`, `outcome-kpis.md`,
> `dor-validation.md`, `shared-artifacts-registry.md`, `journey-experiment-loop-visual.md`.
> British English, no em-dashes.

---

## What this feature is

The outcome Andrea asked for: "a consolidated version we can start experimenting with". C1 made the
runtime exist (one process: OTLP ingest + the three stores + the three query routers over shared
live state, a metric sent at T queryable at T). This feature wraps that runtime into the
newcomer-facing "one command, send, see" loop that the roadmap calls Milestone 1's remainder:

- **C2** - a one-command run story (compose + a thin make/just wrapper) that brings up the
  consolidated runtime and Prism over a shared volume with the minimal env.
- **C3** - a telemetry generator + a little sample data, so the stack is not empty.
- **C4** - honest getting-started docs for the consolidated path.

At the end of this feature the Milestone 1 objective is met: one command, send, see, locally, in
minutes.

---

## Decisions agreed before the wave started (pre-decided, recorded for posterity)

Supplied in the brief; recorded so DESIGN does not re-litigate.

- **D1. Feature type**: Infrastructure (the run/experiment story). No new product surface; the
  "interface" is a command, a browser URL, and HTTP queries, plus docs.
- **D2. Walking skeleton**: No greenfield skeleton - this composes the existing C1 runtime + Prism.
  Slice 1 (US-01) is the thin end-to-end spine and functions as the feature-level walking skeleton.
- **D3. UX research depth**: Lightweight. The journey is the newcomer's three-beat experiment loop
  (run, send, see) with a curious-to-delighted arc; the only UI to consider is Prism's first-look
  states.
- **D4. JTBD analysis**: No. The job is validated upstream by the roadmap and the state assessment.

Output directory: `docs/feature/experimentable-stack-v0/discuss/`.

---

## W1. Build on the consolidated runtime, not the old five binaries (DECIDED)

The run story brings up the C1 `kaleidoscope` binary (crate `kaleidoscope-runtime`): one process,
ports 4317/4318 + 9090/9091/9092, one shared pillar root, one tenant, auth off. It does NOT compose
the old `kaleidoscope-gateway` + three query-API binaries. Rationale: C1's whole point is live
shared state in one process; composing the separate binaries would reintroduce the frozen-snapshot
gap (a query process started before telemetry sees nothing until restarted, assessment section 4).
The roadmap is explicit that C2 builds the run story ON the consolidated bin.

## W2. The feature is ADDITIVE (DECIDED)

Compose, the generator, the wrapper, and the docs are added; the existing `Dockerfile`,
`Dockerfile.gateway`, `Dockerfile.query-api`, the four standalone binaries, and the CLI quick start
are NOT removed or broken. US-07 is the explicit guardrail story. A new `Dockerfile.runtime` (if
DESIGN adds one) sits alongside the existing three, never replacing them.

## W3. Minimal-friction local posture is a first-class requirement (DECIDED)

One command + (a seed or one generator command) + a browser look, with no auth ceremony: one tenant
(`KALEIDOSCOPE_TENANT=acme`), auth off everywhere, no tokens, no TLS, one shared pillar volume.
Captured as a System Constraint, not re-argued per story. This matches the C1 minimal posture
(environments.yaml tenant/auth posture).

## W4. Three thin slices, one per roadmap item (DECIDED)

Slice 1 = C2 (run story: US-01/02/03). Slice 2 = C3 (generator + seed: US-04/05). Slice 3 = C4
(docs + additive guardrail: US-06/07). Each slice delivers a working behaviour a newcomer can
verify; each ships in sequence as Milestone 1 completion. See `story-map.md` for the walking
skeleton and priority rationale.

## W5. Scope is right-sized (DECIDED)

See `story-map.md` Scope Assessment: 7 stories, 3 contexts, under every Elephant Carpaccio oversize
signal. The runtime is reused unchanged; this is wiring, a generator, and docs. No split needed.

## W6. The honest verification limit (DECIDED, load-bearing for honest-claim discipline)

A docker-compose-up plus a real browser rendering Prism is verified by bringing it up and looking.
CI is headless ubuntu and trunk-based (feedback, not a gate); it can curl the query endpoints and
assert the generator-then-query loop, but it does NOT drive a browser to confirm Prism paints a
chart (project memory: prism ECharts needs a CI-browser; `p95_wallclock_flakes_overnight` warns off
timing-shaped CI gates). Therefore:

- The HTTP loop (bring-up to endpoints answer; generator to query returns rows) is the CI-testable,
  honest, gateable core.
- "Prism paints a metric in the browser" and the north-star time-to-first-telemetry-seen are
  reported as manual / smoke-verified, never asserted as a hard CI gate.
- The getting-started docs (US-06) state this limit plainly so a reader knows what is and is not
  automatically checked.

This is the one place this feature could overclaim; it is stated here, in the docs, and in
`outcome-kpis.md` so the claim stays honest.

---

## Per-slice scope

| Slice | Roadmap | Stories | Entry point(s) | Observable proof |
|-------|---------|---------|----------------|------------------|
| 1 | C2 | US-01, US-02, US-03 | `make up` / `docker compose up`; open the documented URL | stack up; Prism loads; 9090/9091/9092 answer; clean/idempotent; port-conflict clear error |
| 2 | C3 | US-04, US-05 | `make demo` / generator command; first look after `make up` | metrics/logs/traces queries return the sample telemetry; Prism paints `request_count`; first look not empty |
| 3 | C4 | US-06, US-07 | the getting-started section; the pre-existing binaries/Dockerfiles/CLI demo | a cold reader completes the loop; the 4 binaries + 3 Dockerfiles + CLI demo still build/run |

---

## Telemetry-generator options surfaced for DESIGN (flag F3)

The generator (US-04) and the seed (US-05) need an implementation. DISCUSS stays neutral; the
options, with trade-offs:

1. **Extend `kaleidoscope-cli` with an OTLP-over-the-wire push subcommand.** Pro: in-repo, no new
   crate, a natural home. Con: the CLI today writes NDJSON directly to lumen/cinder (it is NOT an
   OTLP client), so this is genuinely new client-side OTLP work, not a small flag.
2. **A small new generator using `spark` (the manual-init OTel SDK wrapper).** Pro: dogfoods the
   "built from scratch" ethos, emits real OTLP across all three signals, exercises Kaleidoscope's
   own SDK. Con: a new bin crate; `spark` is v0. **Luna's recommendation** - best fit for the
   project's identity and covers all three signals over real OTLP.
3. **Wire an external OTel generator (`telemetrygen` from otel-collector-contrib) into compose as a
   one-shot service.** Pro: zero first-party code, battle-tested, covers all three signals. Con:
   adds an external image, sits against the README's "built from scratch, not assembled" principle
   (defensible for a dev-only demo generator, but a values call).
4. **A curl-of-protobuf helper against 4318 (HTTP/protobuf).** Pro: no toolchain. Con: hand-encoding
   OTLP protobuf is brittle and ugly; poor experience to maintain.

DESIGN owns the call. Whatever is chosen must satisfy: pushes metrics + logs + traces for `acme`;
clear failure against a down stack; safe re-run; and (US-05) a once-only seed path (or a documented
"run the generator first" if DESIGN folds the seed into US-04 + US-06).

---

## Flags for DESIGN / DEVOPS (they own HOW)

1. **F1 - new `Dockerfile.runtime` vs reuse.** The three existing Dockerfiles build
   cli/gateway/query-api; NONE builds `kaleidoscope-runtime`. Recommendation: add a new
   `Dockerfile.runtime` mirroring the existing multi-stage pattern (rust:1.88-slim-bookworm builder
   to debian:bookworm-slim runtime), `cargo build --release -p kaleidoscope-runtime --locked`,
   `EXPOSE 4317 4318 9090 9091 9092`, `ENV KALEIDOSCOPE_PILLAR_ROOT=/data`, `VOLUME ["/data"]`. The
   existing three stay (W2/US-07). DESIGN/DEVOPS confirm.
2. **F2 - compose + wrapper shape.** One runtime service (+ Prism, see F4) and a shared named
   volume. The thin wrapper: Makefile (no extra tooling) vs justfile. Recommendation: a Makefile
   with `up`, `down`, `demo`/`seed`, `logs`, `clean` targets. Define idempotency and whether `down`
   preserves the volume (a `clean` target clears it) - US-03's reliability AC.
3. **F3 - telemetry generator + seed.** See the options section above. Recommendation: option 2
   (`spark`-based). Decide the seed mechanism (auto-seed on first `up` vs a one-shot compose service
   vs documented `make demo` first step) under the once-only constraint.
4. **F4 - Prism serving topology + config.** Prism's `config.json` `backend.url` is the relative
   `/api/v1`. Two topologies: (a) **same-origin** - serve Prism's built `dist/` from the metrics
   router via `KALEIDOSCOPE_QUERY_STATIC_DIR`, the relative backend just works, no CORS, no Prism
   config change (the path is already supported per the query-api docs and C1 environments.yaml
   `static_dir`); or (b) **separate service** - a static server for Prism on its own port, requiring
   `backend.url` to be set to the runtime's absolute metrics URL and CORS handled.
   **Recommendation: (a) same-origin** - simplest, CORS-free, no config edit, likely no separate
   Prism compose service (Prism is a build artifact baked into / mounted by the runtime image).
   Decide where Prism's `dist/` is built (a compose build stage vs the runtime image vs a checked-in
   build) - DEVOPS.
5. **F5 - what CI exercises.** Per W6, recommend a CI smoke of the HTTP loop (bring-up to endpoints
   answer; generator to query returns rows) as feedback (not a hard gate), binding ephemeral ports
   (fixed-port flake discipline). The browser render stays manual. Decide whether the smoke runs in
   `docker compose` in CI or against an in-process runtime.

---

## Risks and mitigations

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| Prism same-origin vs separate-service decision drifts; the relative `/api/v1` backend breaks under a separate-service topology | Medium | High (user reaches "see" and sees nothing) | F4 with a clear same-origin recommendation; the registry pins the Prism-backend integration checkpoint. |
| The generator pushes a different tenant/metric name than the docs/Prism query | Medium | High (empty chart reads as broken) | `shared-artifacts-registry.md` pins tenant `acme` and the sample-data vocabulary across generator, seed, docs, and smoke; reuses C1's sample data. |
| Overclaiming the browser experience as CI-verified | Medium | Medium (honesty regression) | W6 states the limit; docs (US-06) state it; KPIs report it as manual with a CI HTTP lower bound. |
| The external-generator option (F3.3) pulls in tooling against the "built from scratch" ethos | Low | Medium | Recommendation is the `spark`-based option; the trade is surfaced for DESIGN to weigh. |
| Port conflicts / fixed-port flake in any CI smoke | Medium | Low | US-03 makes port-conflict a clear-error scenario; F5 recommends ephemeral ports + sweep/retry (project memory `aperture_fixed_port_4317_flake`). |
| Two writers on the shared volume (a stray gateway co-run) | Low | High | One-writer constraint (W7-equivalent from C1); compose runs only the consolidated runtime as writer; registry checkpoint. |
| DIVERGE artefacts absent | Certain | Low | Recorded below; the roadmap + state assessment + C1 artefacts + this brief are the authority. |

## Missing-DIVERGE note

No `docs/feature/experimentable-stack-v0/diverge/recommendation.md` or `job-analysis.md` exists.
Per the brief, no DIVERGE wave was run; the job is validated upstream by
`docs/roadmap/consolidation-roadmap.md` (items C2/C3/C4, Milestone 1),
`docs/analysis/consolidation-state-2026-06.md` (the run-story gap), and the DONE C1 artefacts under
`docs/feature/consolidated-runtime-v0/`. DESIGN should not search for a non-existent DIVERGE corpus.

---

## Handoff to DESIGN

Recipient: `nw-solution-architect` (Morgan). Required reading order:

1. `wave-decisions.md` (this file) - the slice scope, the generator options, the F1-F5 flags, the
   honest verification limit (W6).
2. `journey-experiment-loop-visual.md` - the newcomer's run, send, see loop and emotional arc.
3. `story-map.md` - backbone, walking skeleton, three slices, priority rationale, scope assessment.
4. `user-stories.md` - seven LeanUX stories (US-01..US-07), each with Elevator Pitch, Problem, Who,
   Solution, Domain Examples, UAT, AC, KPIs, Technical Notes, Dependencies.
5. `shared-artifacts-registry.md` - tenant, ports, pillar volume, Prism backend, sample-data
   vocabulary, with sources/consumers/integration risk.
6. `outcome-kpis.md` - time-to-first-telemetry-seen north star plus guardrails and the honesty note.
7. `dor-validation.md` - the 9-item gate, passed for all seven stories with evidence.

DESIGN decides, within the W1 consolidated-runtime shape: F1 (Dockerfile.runtime), F2 (compose +
wrapper), F3 (generator + seed), F4 (Prism topology + config), F5 (what CI exercises).

## Handoff to DEVOPS

`platform-architect` (Apex): `outcome-kpis.md` carries the time-to-first-telemetry-seen target and
the HTTP-smoke-vs-browser-manual split (F5/W6); F1 (Dockerfile.runtime), F2 (compose volume +
wrapper), and F4 (where Prism's `dist/` is built and served) are the DEVOPS-flavoured calls. There
is no deploy target; Kaleidoscope deploys nothing, operators run the stack locally.

## Handoff to DISTILL

`acceptance-designer` (Quinn): the per-story Gherkin in `user-stories.md` is the SSOT for scenarios.
The CI-testable acceptance core is the HTTP loop (bring-up to endpoints answer; generator to query
returns rows for all three signals; empty/error states); the browser render is the manual part per
W6. The integration checkpoints in `shared-artifacts-registry.md` are the cross-context acceptance
points.

## Definition-of-Ready status

All seven user stories pass the 9-item DoR hard gate. Evidence in `dor-validation.md`. Peer review
next.
