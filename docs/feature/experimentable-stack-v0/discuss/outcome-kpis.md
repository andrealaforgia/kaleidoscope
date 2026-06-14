# Outcome KPIs - `experimentable-stack-v0`

> Milestone 1 completion (C2 + C3 + C4) on top of the DONE C1 consolidated runtime. British
> English, no em-dashes. Framework: Who / Does what / By how much / Measured by / Baseline
> (Gothelf-Seiden), leading-vs-lagging, with guardrails that must not degrade.

## Objective

A newcomer goes from a fresh clone to seeing telemetry in minutes, locally, with one command:
bring the stack up, send (or auto-seed) sample telemetry, see a metric in Prism and query logs and
traces, guided by honest getting-started docs.

## North Star: time-to-first-telemetry-seen

The wall-clock from running the one bring-up command on a fresh checkout to a sample metric being
visible (in Prism, or returned by a metrics query). This is the single metric that captures "one
command, send, see" working for a stranger.

- **Who**: a newcomer on a fresh checkout.
- **Does what**: reaches a visible metric.
- **By how much**: under about 5 minutes on a cold checkout (first run is dominated by the docker
  image build), and under about 60 seconds on a warm checkout (images cached); both following only
  the getting-started docs.
- **Measured by**: a timed manual walkthrough following the README verbatim, plus a CI-runnable
  smoke that times `bring-up to generator to metrics-query-returns-a-point` for the HTTP-verifiable
  portion (the Prism browser render is the manual part, per the honesty limit).
- **Baseline**: effectively unbounded / impossible today. There is no compose, Makefile, or run
  script; the only documented path is the CLI NDJSON demo, and the manual five-binary route needs a
  restart dance a newcomer would not discover.

## Outcome KPIs

| # | Who | Does What | By How Much | Baseline | Measured By | Type |
|---|-----|-----------|-------------|----------|-------------|------|
| 1 | Newcomer (fresh checkout) | sees a sample metric after one command | time-to-first-telemetry-seen < ~5 min cold / < ~60 s warm | impossible (no run story) | timed walkthrough + CI smoke of the HTTP loop | Leading (north star) |
| 2 | Newcomer / experimenter | brings up a reachable stack with a loaded UI from one command | 100% of bring-ups reach a loaded UI and answering endpoints | n/a (no run story) | smoke: curl 9090/9091/9092 after `make up`; manual browser check | Leading |
| 3 | Newcomer / experimenter | gets all three signals back after the generator | 3 of 3 signals queryable; a metric painted in Prism | 0 (empty stack, no generator) | curl the three query endpoints after the generator | Leading |
| 4 | Newcomer | sees data on the very first look (not empty) | first-look-not-empty in 100% of fresh bring-ups | empty first look | manual first-run; curl a metric right after `make up` | Leading (secondary) |
| 5 | Cold reader of the docs | completes "one command, send, see" unaided | reaches "see a metric" from the docs alone | only CLI demo documented | dry-run following the docs verbatim | Leading (secondary) |
| 6 | Existing user | continues to build/run the pre-existing paths | 0 regressions across 4 binaries, 3 Dockerfiles, CLI demo | all build/run today | build/run checks; CI gate-1 compiles the workspace | Guardrail |

## Metric Hierarchy

- **North Star**: time-to-first-telemetry-seen (KPI 1).
- **Leading Indicators**: one-command bring-up success to a loaded UI (KPI 2); all-three-signals
  queryable after the generator (KPI 3).
- **Secondary Leading**: first-look-not-empty (KPI 4); docs-followability (KPI 5).
- **Guardrail Metrics (must NOT degrade)**:
  - existing binaries, Dockerfiles, and the CLI demo still build and run (KPI 6 / US-07);
  - no secrets/tokens/TLS required for the local experiment (minimal-friction posture, W3);
  - the C1 live-visibility property and tenant isolation are unchanged (the run story only wraps the
    runtime; it must not weaken it);
  - per-record fsync durability of pulse/lumen/ray unchanged (run story is additive wiring).

## Measurement Plan

| KPI | Data Source | Collection Method | Frequency | Owner |
|-----|------------|-------------------|-----------|-------|
| 1 | timed walkthrough + CI smoke | time `make up` to first metric returned (HTTP portion in CI; browser manual) | per release of the run story | DEVOPS + DISTILL |
| 2 | smoke script | curl the three query endpoints after `make up`; manual browser open | per bring-up under test | DEVOPS |
| 3 | acceptance/smoke | run the generator, then curl metrics/logs/traces | per generator run under test | DISTILL |
| 4 | first-run check | curl a metric immediately after `make up` | per fresh bring-up | DISTILL |
| 5 | docs dry-run | follow the getting-started section verbatim on a clean checkout | per docs change | Luna / reviewer |
| 6 | build/run checks | build each Dockerfile + run each binary; CI gate-1 workspace compile | per push (CI) + manual Docker | DEVOPS |

## Honesty Note (verification limit)

The browser-render parts of KPIs 1-4 (Prism actually painting a metric on screen) are verified by
bringing the stack up and looking, not by a browser-driven CI test: CI is headless ubuntu and the
project has no CI-browser harness for Prism's ECharts today (project memory:
`project_four_quadrants_reports_are_stale` notes prism ECharts needs a CI-browser;
`p95_wallclock_flakes_overnight` cautions against timing-shaped CI gates). The HTTP loop
(bring-up to query-returns-rows; generator to query-returns-rows) IS CI-testable via curl and is the
honest, gateable core. The north-star time is therefore reported as a manual measurement with a
CI-measured HTTP lower bound, not asserted as a hard CI gate.

## Handoff to DEVOPS (instrumentation)

- **Data to capture**: bring-up success/failure; the three query endpoints' reachability after
  bring-up; generator success and the post-generator query results; Docker build success for the
  existing three Dockerfiles plus any new `Dockerfile.runtime`.
- **Smoke vs gate**: the HTTP loop is a candidate CI smoke (feedback, not a hard gate, per the
  trunk-based posture); the browser render and the north-star timing stay manual.
- **No new metric stack** at this milestone; a runtime-emitted freshness metric remains a later
  concern (carried over from C1's `outcome-kpis` posture).

## Hypothesis

We believe that a one-command run story plus a one-command telemetry generator plus an honest
getting-started doc, all over the existing consolidated runtime, will let a newcomer reach a visible
metric in minutes. We will know this is true when a cold reader, following only the docs, brings the
stack up and sees `request_count` in Prism within about five minutes on a fresh checkout, with all
three signals queryable, and no pre-existing path regressed.
