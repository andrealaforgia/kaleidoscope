---
marp: true
theme: default
paginate: true
size: 16:9
title: Building Kaleidoscope with nWave
author: Andrea Laforgia
---

<!--
SLIDE DECK — Kaleidoscope x nWave

Living document. Grows one block per closed nWave wave per feature.

Audience: technical engineers (LinkedIn / Substack readership; familiar
with TDD, trunk-based development, BDD, mutation testing as terms; new to
OTLP and observability internals).

Framing: nWave-centric. Andrea uses nWave (the AI-amplified delivery
framework by Alessandro Di Gioia and Michele Brissoni at nWave.ai)
on Kaleidoscope as the worked example. nWave is the framework Andrea
adopts and dogfoods; T*D (TDD + trunk-based + team-focused) is
Andrea's own thesis, separate from nWave but tightly aligned with it.

Format: Marp Markdown. Each `---` is a slide boundary. Keep slides
sparse — two or three lines each, one idea per slide.

Pair file: narrative.md holds the long-form text the slides are extracted
from, with links to artefacts.
-->

# Building Kaleidoscope with nWave

## A live worked example of AI-amplified software delivery

Andrea Laforgia · Public domain · CC0-1.0

---

# Why this talk exists

I am building an observability platform from scratch.

Every line of code is written by AI agents.

I want to show you how, and why it does not feel reckless.

---

# What is Kaleidoscope

End-to-end observability platform.

Logs, metrics, traces, profiles.

The work of Datadog, the LGTM stack, ELK, BetterStack — combined into one tool.

Public domain. CC0-1.0. No vendor can take it away.

---

# The rug-pull problem

Elastic. MongoDB. Redis. HashiCorp.

Each one was open source.

Each one re-licensed once it became valuable.

The pattern is structural, not accidental.

---

# The Kaleidoscope answer

Built from scratch on Apache Foundation substrate.

No commercial dependencies.

Released under CC0-1.0 (or AGPL-3.0 for platform components, Apache-2.0 for SDKs — under review).

Governance designed to make re-licensing structurally impossible.

---

# The fifteen optical instruments

Spark · Aperture · Sieve · Sluice · Codex

Pulse · Lumen · Ray · Strata · Cinder

Prism · Beacon · Augur · Aegis · Loom

A caleidoscope refracts grey light into a clean spectrum. Same job: refract telemetry into the four signals.

---

# The two planes

**Integration plane** — the parts users touch first. SDK, gateway, alerts, dashboards. Ships in roughly six months. Useful immediately on top of any existing backend.

**Storage plane** — the parts that replace the backend. Engines for logs, metrics, traces, profiles. Ships afterwards. One engine at a time, opt-in.

---

# Why two planes matter

Most observability rewrites die because the storage engines are decade-class engineering, and nothing useful ships until they are done.

Splitting the planes means month six already delivers a usable platform.

The hardest engineering arrives only when the easier work has proved the methodology.

---

# What is nWave

An AI-amplified delivery framework by Alessandro Di Gioia and Michele Brissoni at nWave.ai.

Five waves per feature: DISCUSS, DESIGN, DISTILL, DELIVER, DEVOPS.

A specialised AI agent leads each wave.

A specialised reviewer agent critiques each wave.

Nothing ships until both pass.

I dogfood it on every project.

---

# nWave wave by wave

**DISCUSS** — Luna, the product owner. Stories, journeys, acceptance criteria.

**DESIGN** — Morgan, the solution architect. C4 diagrams, ADRs, technology choices.

**DISTILL** — Scholar, the acceptance designer. Executable acceptance tests, RED.

**DELIVER** — Crafty, the software crafter. Outside-in TDD turns RED into GREEN.

**DEVOPS** — Apex, the platform architect. CI/CD, observability, deployment readiness.

---

# Why peer review at every gate

AI-generated work is not trusted work.

Speed of generation does not reduce the need for verification.

A second specialist agent reads the first one's output with a different brief.

Either both pass, or the wave does not close.

---

# What this enables

A solo author can dogfood the discipline of a high-functioning engineering team.

Without the team.

Without the bus factor.

Without the ceremony that exists only to coordinate humans.

---

# Case study: feature 1

The OTLP conformance harness.

A small Rust library.

Single job: validate that a byte sequence is a valid OpenTelemetry message.

The smallest thing that exercises the full nWave loop end to end.

---

# Why this feature first

It is the leaf dependency.

Every other Kaleidoscope component will consume it.

If we cannot run a feature this small through the methodology cleanly, we cannot run anything.

Walking skeleton for the methodology, not for the product.

---

# DISCUSS — what Luna produced

Seven user stories with Elevator Pitches and acceptance criteria.

Seven Elephant Carpaccio slices, each shipping end-to-end value.

A journey map and outcome KPIs.

Definition of Ready validated on all nine items.

Sentinel approved on iteration 2 after a substantive rework.

---

# DESIGN — what Morgan produced

C4 system context, container, component diagrams.

Five Architecture Decision Records:

ADR-0001 public API. ADR-0002 error type. ADR-0003 dependency pinning. ADR-0004 test vector layout. ADR-0005 CI contract.

Atlas approved on iteration 1.

---

# DISTILL — what Scholar produced

Fifty-two acceptance tests in Rust integration-test format.

Each test maps to a user story and a slice.

All RED on day one — no implementation existed.

Hexagonal boundary mandate enforced: tests import only the public surface.

Sentinel approved on iteration 2.

---

# DELIVER — what Crafty produced

Seven slices implemented outside-in, one at a time.

Each slice: red → green → refactor.

Seventy-three tests green at close.

One hundred per cent mutation kill rate.

Crafty in review mode approved on iteration 1.

---

# DEVOPS — what Apex produced

GitHub Actions workflow with five blocking gates per ADR-0005:

cargo deny · cargo test · cargo public-api · cargo semver-checks · cargo mutants.

Local pre-commit and pre-push hooks mirroring the CI gates.

Forge approved on iteration 1.

---

# The first end-to-end CI run

Seven minutes fifty-five seconds wall-clock.

All five gates green.

Mutation kill rate confirmed on real Linux infrastructure, not on a developer Mac.

The harness shipped at tag `otlp-conformance-harness/v0.1.0`.

---

# What the harness wave taught us

The methodology survives operational reality.

Every reviewer agent caught real problems.

Post-merge corrections were small and recoverable.

The artefact-vs-reality gap was the most important learning, and it surfaced exactly where it should: at the first real CI run.

---

# Case study: feature 2

Aperture — the OTLP receiver.

The first network-facing component.

Listens on gRPC port 4317 and HTTP/protobuf port 4318.

Validates every incoming payload through the harness.

Hands accepted records to a pluggable sink.

---

# What changes from a library to a service

Aperture is a long-lived process. It has runtime concerns the harness did not.

Backpressure, graceful shutdown, observability of itself, configuration with forward-compat knobs.

The methodology absorbs the difference without ceremony.

---

# Aperture — DISCUSS through DEVOPS

Six locked scope decisions in one round-trip with Andrea.

Eight Elephant Carpaccio slices.

Eighty-four RED acceptance tests via the same Strategy C "real local" pattern as the harness.

Five DESIGN ADRs continuing from ADR-0006 through ADR-0010.

Three new CI invariants surfaced for DEVOPS.

---

# Aperture — DELIVER closed

Eight slices, all green.

Slices 01-04: walking skeleton, HTTP and readiness, the three signals.

Slice 05: per-transport semaphore backpressure.

Slice 06: ForwardingSink and the Earned-Trust probe gold-test.

Slice 07: TLS and SPIFFE config knobs reserved for Phase 2.

Slice 08: deadline-bounded drain on SIGTERM, /readyz flips to 503 in 100 ms.

176 active tests. 100% mutation kill rate. Tag `aperture/v0.1.0`.

---

# Aperture v0 — graduation

After the eighth slice closed, three CI gates that had been scoped to the harness during DELIVER were graduated to cover both crates.

Gate 1 (cargo test) → `--workspace`.

Gate 5 (cargo mutants) → both packages.

Local pre-commit hook → workspace coverage.

One commit. Then `aperture/v0.1.0` is canonical.

---

# What is consistent across both features

Discipline, not heroics.

Small commits.

Trunk-based development with no required-status-checks gate.

CI as feedback, not as a blocker.

Fix-forward when reality contradicts the artefact.

---

# What I want you to take away

AI agents do not replace engineering discipline. They amplify it.

The methodology is the load-bearing structure. The agents are the cheap labour that lets you afford the methodology.

Without the discipline, the speed becomes recklessness very quickly.

---

# Where to follow along

Repository: github.com/andrealaforgia/kaleidoscope

Public domain. CC0-1.0.

Every artefact is in the repo. Every commit is on `main`.

Read the wave-decisions documents. They are the primary source.

---

# Thank you

Questions are welcome.

Pushback is more welcome.
