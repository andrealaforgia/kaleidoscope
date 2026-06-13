# Kaleidoscope consolidation roadmap

Toward a consolidated version we can run with one command and experiment with.

Author: Bea. Date: 2026-06-11. Grounded in
`docs/analysis/consolidation-state-2026-06.md`.

## The objective

A single command brings up Kaleidoscope. You send OTLP telemetry at it and
immediately see and query the result: metrics painted in Prism, logs and traces
through the query APIs, with the minimal local configuration. That is the bar
for "we can start experimenting with it".

## Where we are

Nineteen features have landed and the components are individually solid: durable
stores with per-record fsync, ingest on both transports for all three signals, a
gateway that fans telemetry to the stores, three query APIs, Prism painting
metrics, per-request auth on both the write and the read door. What does not yet
exist is a system. The evidence is in the state assessment, and it reduces to
two hard facts.

The first and load-bearing one. Ingest and query are separate OS processes that
share only a filesystem path. Each file-backed store loads its snapshot and WAL
into an in-memory map once, at startup, and the query handlers read that frozen
map with no re-read or watch. So a query API started before telemetry arrives
shows nothing until it is restarted. The natural experiment loop, bring up the
stack then send a metric then look, fails by construction. This is not a bug in
any one crate, it is the absence of shared live state between the writer and the
reader.

The second. There is no one-command run story. No compose file, no Makefile, no
run script. Three Dockerfiles exist but nothing composes them, and the README
quick start covers only the CLI demo, not the gateway-plus-query-plus-Prism
stack. You can launch five binaries by hand over a shared pillar root, but that
is not experimenting, that is plumbing.

Everything else (logs and traces absent from the Prism UI, the tiering and queue
and SLO crates sitting unwired on the shelf) is secondary to those two.

## The decision that shapes the roadmap

There are two ways to give the reader live sight of what the writer just wrote.
Either run ingest, the stores, and query in one process over a shared in-memory
store, so a write is instantly visible to a read, or keep them as separate
processes and teach the query stores to re-read or watch the WAL as it grows.

The roadmap recommends the single-process route for the consolidated
experimentable version. It is the shortest path to "send a metric, see it", it
matches the word consolidated, and it removes a whole class of cross-process
freshness problems while we are still learning what the system should be. The
multi-process, live-reload, horizontally-scaled shape is a real future, but it is
a scaling concern, not an experimentation one, and committing to it now would buy
distribution we cannot yet use at the cost of the freshness we need today.

This is a genuine architecture fork and it is Andrea's to veto. The roadmap
proceeds on single-process unless he prefers the distributed-with-reload shape,
in which case Milestone 1 reshapes around a WAL-watch adapter instead of a shared
store, and the run story and docs that follow are unchanged.

## Milestone 1: a live, one-command, experimentable Kaleidoscope

The minimum that makes experimenting real. Nothing here is new product surface,
it is wiring what exists into a running whole.

C1, the consolidated live runtime. A single process runs OTLP ingest, the
pulse/lumen/ray stores, and the three query routers over one shared store per
signal, so telemetry ingested at time T is queryable at time T, not after a
restart. The query routers already accept an injected store, so the work is a
composition root that builds each store once and hands the same instance to both
the ingest sink and the query router, on one runtime. This is the spine. Until it
exists, nothing else makes the experiment work.

C2, the one-command run story. A compose file, and a thin Makefile or justfile
over it, that brings up the consolidated runtime and Prism with a shared volume
and the one required tenant variable. `docker compose up`, then a working stack.

C3, a telemetry generator. A small tool, or an extension of the existing CLI,
that pushes sample OTLP metrics, logs, and traces so there is something to look
at within seconds, plus a tiny seed so a fresh stack is not empty.

C4, getting started. A README section that is honestly the gateway path, not the
CLI demo: one command up, send telemetry, see metrics in Prism and query logs and
traces, here is the minimal config and nothing more.

At the end of Milestone 1 the objective is met: one command, send, see.

## Milestone 2: complete the visible experiment

C5, logs and traces in Prism. Today Prism paints metrics only. Surface log and
trace query results in the UI, or at minimum document and smooth the query-API
path for them, so the experiment is the whole signal set, not a third of it.

C6, query completeness. Honour `step` in the metrics query and close the smaller
query-API gaps the assessment noted, so what the experimenter asks for is what
they get.

## Milestone 3: pull the shelf into the running system

As experiments demand it, not before. Each is a full feature in its own right.

C7, wire cinder tiering into the live path. C8, wire the sluice ingest buffer and
sieve where they earn their place. C9, self-observe inside the runtime so
Kaleidoscope watches itself. C10, beacon SLO, augur, strata, loom, codex as the
system grows past a first experiment.

## Sequencing and what is a must-have

Milestone 1 (C1 to C4) is the must-have for a first experiment and is the whole
focus until it is done. C1 is first and is the gate for the rest. Milestone 2
makes the experiment complete rather than partial. Milestone 3 is iterative and
demand-driven once the thing actually runs.

Each item is delivered as a full nWave feature. The first is C1.
