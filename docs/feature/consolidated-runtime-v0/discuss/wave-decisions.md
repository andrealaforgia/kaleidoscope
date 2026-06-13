# Wave Decisions — `consolidated-runtime-v0` (DISCUSS)

> **Wave**: DISCUSS (nw-product-owner / Luna).
> **Date**: 2026-06-13.
> **Author**: Luna, single-pass, autonomous overnight run.
> **Feature**: `consolidated-runtime-v0` — item C1 (the spine) of the consolidation roadmap.
> **Companion documents**: `journey-experiment-loop-visual.md`, `shared-artifacts-registry.md`, `story-map.md`, `user-stories.md`, `outcome-kpis.md`, `dor-validation.md`.

---

## What this feature is

C1 is the **consolidated live runtime**: one process that builds ONE store per signal,
wraps it in `Arc`, and hands the SAME instance to BOTH the ingest sink AND the query
router — so telemetry ingested at time T is queryable at time T, not after a restart.

It fixes the single load-bearing consolidation gap named in
`docs/analysis/consolidation-state-2026-06.md` §4 and `docs/roadmap/consolidation-roadmap.md`:
today ingest (`kaleidoscope-gateway`) and the three query APIs are **separate OS processes
that share only a filesystem path**, and each query process loads its store snapshot+WAL
into an in-memory map ONCE at startup and never re-reads. So a query API started before
telemetry arrives shows nothing until it is restarted. The natural experiment loop — bring
up the stack, send a metric, look — **fails by construction**. C1 is what makes it work.

---

## Decisions agreed before the wave started (recorded for posterity)

Settled with Andrea before Luna began work; recorded so DESIGN does not re-litigate.

- **D1. Feature type**: Backend / infrastructure. The runnable consolidated system. No new
  UX surface; the "interface" is a command, an OTLP push, and an HTTP query.
- **D2. Walking skeleton**: No greenfield skeleton — this composes existing, individually
  proven components. BUT slice 1 (US-01) is a thin end-to-end proof and functions as the
  feature-level walking skeleton: ONE signal (metrics) ingested and immediately queried from
  the same live store in one process.
- **D3. UX research depth**: Lightweight. The journey is a three-beat experiment loop
  (run → push → query) with a flat-then-relieved emotional arc; there is no human-in-the-loop
  UI to research.
- **D4. JTBD analysis**: No. The motivation is uncontroversial and already validated by the
  roadmap: make the send-then-see loop work without a restart.

Output directory: `docs/feature/consolidated-runtime-v0/discuss/`.

---

## W1. ARCHITECTURE SHAPE — single-process consolidation (DECIDED, with Andrea-veto flag)

The roadmap (`consolidation-roadmap.md` "The decision that shapes the roadmap") identifies a
genuine architecture fork for giving the reader live sight of what the writer just wrote:

1. **Single process** over a shared in-memory `Arc<Store>` per signal — a write is instantly
   visible to a read. (CHOSEN.)
2. **Distributed / multi-process** with the query stores taught to re-read or watch the WAL
   as it grows.

**Decision: single-process consolidation.** It is the shortest path to "send a metric, see
it", it matches the word *consolidated*, and it removes a whole class of cross-process
freshness problems while we are still learning what the system should be. The distributed,
live-reload, horizontally-scaled shape is a real future, but it is a *scaling* concern, not an
*experimentation* one; committing to it now would buy distribution we cannot yet use at the
cost of the freshness we need today. This matches the roadmap recommendation and Andrea's
standing decide-don't-ask posture.

> ### ANDREA-VETO FLAG (prominent, single point of reversal)
>
> **This is a genuine architecture fork and it is Andrea's to veto.** The entire feature is
> shaped around a shared in-process `Arc<Store>`. **If Andrea prefers the
> distributed-with-live-reload shape instead, this feature reshapes**: the load-bearing work
> becomes a **WAL-watch / reload adapter** on the query stores (so a separate query process
> reflects post-startup appends), NOT a shared in-process store. The user-visible outcome
> (send a metric, immediately query it back, no restart) and every UAT scenario in
> `user-stories.md` stay identical — only the mechanism behind them changes. The story map,
> KPIs, run story, and docs that follow are unchanged. **Proceeding on single-process** per
> the roadmap and decide-don't-ask; one word from Andrea flips the mechanism.

Evidence the chosen shape is sound and cheap (read from `main`, 2026-06-13):

- `crates/pulse/src/store.rs:72` — `MetricStore::ingest(&self, ...)` and `query(&self, ...)`
  both take `&self`. The concrete `FileBackedMetricStore` carries its state behind a
  `Mutex<Inner>` (`crates/pulse/src/file_backed.rs:81`). So the SAME `Arc<dyn MetricStore>`
  can be written by the sink and read by the router concurrently; a write through the Mutex is
  immediately readable. **No concurrency change is required.** lumen and ray follow the same
  shape.
- `crates/kaleidoscope-gateway/src/main.rs:84-96` already builds `Arc::new(FileBackedMetricStore::open(...))`
  and `Arc::clone`s it into `StorageSink::with_all_stores(...)`. The only new move is to
  `Arc::clone` that SAME instance into the query router as well.
- The query routers ALREADY accept an injected store — this is the reuse seam:
  - `crates/query-api/src/lib.rs:122` `router(store: Arc<dyn MetricStore + Send + Sync>, tenant, static_dir)`
  - `crates/log-query-api/src/lib.rs:95` `router(store: Arc<dyn LogStore + Send + Sync>, tenant)`
  - `crates/trace-query-api/src/lib.rs:100` `router(store: Arc<dyn TraceStore + Send + Sync>, tenant)`
  - All three also expose `router_with_auth(..., auth, ...)` for the read-path-auth feature.

## W2. The consolidated runtime is ADDITIVE — the existing binaries stay

C1 adds a consolidated entry point; it does NOT delete `kaleidoscope-gateway`, `query-api`,
`log-query-api`, or `trace-query-api`. Those separate binaries remain valid (they are the
basis of the distributed future and of the existing Dockerfiles). Whether the consolidated
runtime is a NEW top-level binary or an extension of `kaleidoscope-gateway` to also host the
query routers is a DESIGN decision (see DESIGN handoff below). Requirements stay neutral on
that: the stories say "the user runs the consolidated runtime with one command", never which
binary provides it.

## W3. Minimal-friction local posture is a first-class requirement

The experiment must be one command + a push + a query, with no auth ceremony. Per the state
assessment §5, the local posture is: **auth off everywhere**; set
`KALEIDOSCOPE_DEFAULT_TENANT=acme` for ingest and `KALEIDOSCOPE_QUERY_TENANT` /
`KALEIDOSCOPE_LOG_QUERY_TENANT` / `KALEIDOSCOPE_TRACE_QUERY_TENANT=acme` for the query routers
(one shared default tenant), over one shared pillar root. No tokens, no TLS. This is captured
as a System Constraint, not re-argued per story.

## W4. Auth, tenancy, and durability MUST NOT regress

C1 reuses the existing seams unchanged. The optional per-request read-auth
(`router_with_auth`, ADR-0074) and the gateway's ingest-auth posture remain available and
fail-closed when configured; tenant isolation (aegis `TenantId`) still scopes every read; the
per-record fsync durability of the file-backed stores is unchanged because C1 shares the same
store instances rather than introducing new ones. Tenant isolation in the consolidated process
is given its own guardrail story (US-02) because "is it still safe to run ingest and query in
one process?" is a real adoption anxiety with its own decision.

## W5. Scope — two slices, capstone, single feature (right-sized)

See `story-map.md` §Scope Assessment. Five stories across two outcome slices. This is well
under every Elephant Carpaccio oversize signal (>10 stories, >3 contexts, >5 skeleton
integration points, >2 weeks). C1 touches only already-wired crates; it is composition, not
new product surface. **No split needed.**

## W6. The live-visibility observable (the heart)

The concrete, observable proof chosen: **a metric POSTed to the ingest endpoint at time T is
returned by the query endpoint at T+epsilon, in the same process, with no restart of
anything** — the exact loop that fails today. Stated as a falsifiable property in US-01 and
measured as the north-star KPI in `outcome-kpis.md`: the runtime is started with an empty
store BEFORE any telemetry; a metric ingested after startup is queryable within 1 second
(p95) without restarting any process. The old separate-process world cannot do this; that is
the regression C1 prevents, expressed as the post-startup-append-visible scenario rather than
as a second running process.

---

## Risks and mitigations

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| Andrea prefers the distributed/WAL-watch shape | Low | High (reshapes the mechanism) | Single, prominent veto flag (W1); user-visible outcome and UAT unchanged either way, so the reshape is contained to the mechanism. |
| Port co-binding conflict (ingest 4317/4318 + query 9090/9091/9092 on one process) | Low | Medium | US-01/US-05 make all-ports-bound an explicit scenario; the assessment notes the fixed-port 4317/4318 flake (`project_aperture_fixed_port_4317_flake`) — DESIGN/DISTILL should bind ephemeral ports in tests and sweep+retry. |
| Two writers to the same WAL would corrupt (state assessment §4 secondary hazard) | Low | High | Single-process means one writer by construction; the consolidated runtime must not be co-run against a separate gateway on the same pillar root. Flagged to DESIGN. |
| `tracing` subscriber double-install across composed components | Medium | Low | The gateway already guards its install with `OnceLock` + `try_init` (`main.rs:156`); DESIGN reconciles one install for the composed process. |
| DIVERGE artefacts absent (`recommendation.md`, `job-analysis.md`) | Certain | Low | Recorded below; the roadmap + state assessment + this brief are the authority. |

## Missing-DIVERGE note

No `docs/feature/consolidated-runtime-v0/diverge/recommendation.md` or `job-analysis.md`
exists. Per the brief, no DIVERGE wave was run; the job is validated upstream by
`docs/roadmap/consolidation-roadmap.md` (item C1) and `docs/analysis/consolidation-state-2026-06.md`.
Those two documents plus this brief are the upstream authority. DESIGN should not search for a
non-existent DIVERGE corpus.

---

## Flags for DESIGN (Morgan owns HOW)

DESIGN should decide, staying within the single-process shape (unless W1 is vetoed):

1. **New binary vs extend the gateway.** Is the consolidated runtime a NEW top-level binary
   (e.g. `kaleidoscope` / `kaleidoscope-runtime`) or an extension of `kaleidoscope-gateway` to
   also host the three query routers on the same tokio runtime? Requirements are neutral; pick
   the shape that keeps the composition root smallest and keeps the separate binaries intact
   (W2).
2. **Port layout on one process.** Ingest gRPC 4317 + HTTP 4318 (aperture) alongside query
   9090 (metrics) / 9091 (logs) / 9092 (traces) — confirm all five bind cleanly on one
   runtime; decide how they are configured and how tests pick ephemeral ports.
3. **Shared-store concurrency.** Confirm (Luna's read says yes) that NO concurrency change is
   needed: `MetricStore::ingest`/`query` take `&self`, the store serialises through its
   interior `Mutex`, and one `Arc` shared between sink and router is sufficient. If DESIGN
   finds a read-during-write hazard, that is a DESIGN-level store change, flagged here.
4. **Single `tracing` install** for the composed process (reconcile aperture's and the
   gateway's `OnceLock`-guarded installs).
5. **One composition root** that opens each store once and injects the same `Arc` into both
   the sink and the router, for all three signals.

## Handoff to DESIGN

Recipient: `nw-solution-architect` (Morgan). Required reading order:

1. `wave-decisions.md` (this file) — the single-process decision, the veto flag, the DESIGN questions.
2. `journey-experiment-loop-visual.md` — the run → push → query loop and emotional arc.
3. `story-map.md` — backbone, walking skeleton, two slices, priority rationale, scope assessment.
4. `user-stories.md` — five LeanUX stories (US-01..US-05), each with Elevator Pitch, Problem, Who, Solution, Domain Examples, UAT Scenarios, AC, KPIs, Technical Notes, Dependencies.
5. `shared-artifacts-registry.md` — the shared store instance, ports, tenant env vars, with sources/consumers/integration risk.
6. `outcome-kpis.md` — the live-visibility north-star KPI plus guardrails.
7. `dor-validation.md` — the 9-item gate, passed for every story with evidence.

## Handoff to DISTILL / DEVOPS

- `acceptance-designer` (Quinn): the per-story Gherkin in `user-stories.md` is the SSOT for
  scenarios; the integration points and the live-visibility property are the acceptance core.
- `platform-architect`: `outcome-kpis.md` carries the freshness/latency target and the run
  story dependency (C2 follows C1).

## Definition-of-Ready status

All five user stories pass the 9-item DoR hard gate. Evidence in `dor-validation.md`. Peer
review next.
