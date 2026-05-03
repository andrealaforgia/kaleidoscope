# Kaleidoscope

> **An OpenTelemetry-compatible observability platform that is free, open source,
> and will always be free and open source.**

Kaleidoscope refracts every telemetry signal — logs, metrics, traces, profiles —
into a single coherent view. It is built to do the work of Datadog, New Relic,
Splunk, Dynatrace, BetterStack, Honeycomb, Grafana Cloud, Chronosphere, and the
LGTM and ELK stacks combined — and to do it without a per-host bill, a per-GB
bill, a per-cardinality bill, a per-user bill, or a "contact sales" page.

---

## The promise

**Kaleidoscope is free and open source software, and that will not change.**

- **Permissively or copyleft-licensed forever.** Every component is shipped under an
  OSI-approved license. The project will never re-license to SSPL, BSL, "Source
  Available", or any other shared-source-but-not-actually-open arrangement. The
  pattern of "open until the VCs need an exit, then closed" — Elastic 2021, MongoDB
  2018, Redis 2024, HashiCorp 2023, Cockroach 2024, Sentry 2019, Confluent 2018 — is
  the precise pattern Kaleidoscope is designed to refuse.
- **No "open core" with locked enterprise features.** Multi-tenancy, SSO, audit
  logs, RBAC, alerting, SLOs, retention controls, dashboards-as-code — *all of it*
  is in the free product. There is no paid tier where the features that make the
  platform usable in production live.
- **No telemetry-on-telemetry.** Kaleidoscope does not phone home. The platform does
  not collect anonymous usage data, license-key validation, "anonymous crash
  reports", or any other vendor-side telemetry. What the platform learns about
  itself, it learns through itself.
- **No "free for non-commercial use" weasel-words.** Run Kaleidoscope at a startup,
  inside a Fortune 500, as a consultancy, as a managed SaaS reseller, in
  air-gapped government compute — same software, same license, same rights.
- **Any future hosted offering will run the same code.** If the maintainers
  ever offer a managed Kaleidoscope, it will be packaged from the same Git tags
  you can `git clone` and run on your own hardware. There is no closed-source
  "control plane" hiding the differentiating features.

These commitments will be encoded in:

- The **LICENSE** files in this repository (one per component, all OSI-approved).
- A **GOVERNANCE.md** that documents how license-changing decisions can — and
  *cannot* — be made.
- A **CLA**-free contribution model (DCO sign-off only). No CLA means no future
  maintainer can unilaterally re-license contributed code.

---

## Why this exists

Modern observability has a cost problem. The tools that watch production are
themselves a recurring six- or seven-figure line item for any non-trivial
business. The pricing models — per-host, per-GB, per-custom-metric,
per-cardinality, per-seat — punish exactly the engineering practices the same
vendors evangelize: rich instrumentation, high-fidelity tracing, long
retention, broad team access.

Open-source alternatives exist (the LGTM stack, the ELK stack, ClickHouse-based
projects like SigNoz and Uptrace, OpenTelemetry itself). They are excellent.
But they are also fragmented: many projects, many query languages, many
operational paradigms, many storage engines. Adopting them well is itself a
specialist skill.

Kaleidoscope is the integration. It takes the best of what is already free and
unifies it under a single platform, a single operational story, and a single
permissive license, so that the cost of running production observability
trends toward the cost of the storage it consumes — and not a dollar more.

---

## Status

**Pre-implementation.** This repository currently contains the design corpus:

| Document | What it is |
|----------|------------|
| [`docs/research/observability/otel-compatible-observability-platform-comprehensive-research.md`](docs/research/observability/otel-compatible-observability-platform-comprehensive-research.md) | Comprehensive, evidence-driven research on building a production-grade OTel-compatible observability platform. 35 cited sources, full appendix, decision worksheet. |
| [`docs/roadmap/kaleidoscope-from-scratch-roadmap.md`](docs/roadmap/kaleidoscope-from-scratch-roadmap.md) | A 10-phase, ~40-month roadmap to build every component in-house. Includes data-flow diagram, build-order DAG, Gantt timeline, per-phase exit criteria, risk register. |
| [`docs/roadmap/kaleidoscope-tech-stack.md`](docs/roadmap/kaleidoscope-tech-stack.md) | The recommended technical stack for each component, with alternatives considered and license notes. |

Code lands when the design is settled.

---

## Components at a glance

Every component is named for a part of an optical instrument. The metaphor is the
contract: light enters, reflects, refracts, emerges as a coherent spectrum.

| Codename       | Role                                                  | Replaces                                 |
| -------------- | ----------------------------------------------------- | ---------------------------------------- |
| **Spark**      | Auto-instrumentation SDKs                             | Datadog APM agents, NR APM agents        |
| **Aperture**   | OTLP-compatible ingest gateway                        | Datadog Agent, Splunk UF, OTel Collector |
| **Sluice**     | Durable ingest buffer                                 | Datadog's internal queues                |
| **Sieve**      | Sampling & filtering                                  | Datadog Live Search filters, Honeycomb Refinery |
| **Codex**      | Schema registry + semantic conventions                | Datadog tags taxonomy                    |
| **Pulse**      | Time-series metrics engine                            | Datadog Metrics, NR Metrics, Cloud Monitoring |
| **Lumen**      | Log storage & search                                  | Datadog Logs, Splunk, Loki, Elastic      |
| **Filament**   | Distributed trace storage & query                     | Datadog APM, NR Distributed Tracing, Tempo |
| **Strata**     | Continuous profiling                                  | Datadog Profiler, NR Code-Level Metrics  |
| **Cinder**     | Cold-tier object-storage adapter                      | Datadog Flex Logs, S3 Archives           |
| **Prism**      | Unified query & visualization frontend                | Datadog dashboards, NR One, Grafana      |
| **Beacon**     | Alerting + SLO burn-rate engine                       | Datadog Monitors, NR Alerts, PagerDuty   |
| **Augur**      | Anomaly detection / AIops                             | Datadog Watchdog, NR AI                  |
| **Aegis**      | AuthN/Z, multi-tenancy, audit                         | Datadog RBAC, NR User Management         |
| **Loom**       | Dashboards-as-code, alert-rules-as-code               | Terraform Datadog provider               |

See [`docs/roadmap/kaleidoscope-from-scratch-roadmap.md`](docs/roadmap/kaleidoscope-from-scratch-roadmap.md)
for the architecture diagram and the build sequence.

---

## How Kaleidoscope defeats the cost model

The big vendors charge for things that, in a well-built FOSS platform, are not
expensive:

| The vendor charges for…                                  | Kaleidoscope's answer                                                  |
| -------------------------------------------------------- | ---------------------------------------------------------------------- |
| Per-host agent licenses                                  | Spark is an SDK. There is no per-host fee, ever.                       |
| Per-GB log ingest, with surge pricing                    | Lumen runs on ClickHouse on your object storage. You pay the cloud bill. |
| Custom metrics over a low free quota                     | Pulse has no metric-count surcharge. Your TSDB has whatever cardinality your hardware supports. |
| Per-million-span APM                                     | Filament charges nothing per span; Sieve drops what you don't need.    |
| Continuous profiling as a top-tier add-on                | Strata is included.                                                    |
| Long-term retention as a separate "Flex" / "Archive" SKU | Cinder's tiering is built in; cold storage is just S3 / GCS / R2.      |
| Per-user dashboard seats                                 | Prism has no seat licensing.                                           |
| SSO, RBAC, audit log, SAML/SCIM as "Enterprise" tier     | Aegis is in the free product. Always.                                  |
| AIops / anomaly detection as an upsell                   | Augur is included; you bring your own model if you want a fancier one. |
| "Contact sales" for compliance reports                   | The compliance dashboards in Prism are open templates.                 |

The structural cost of running observability on Kaleidoscope is the cost of the
underlying compute and storage — which is the cloud bill the vendors are also
paying, plus their margin. Removing the margin is the entire economic thesis.

---

## What Kaleidoscope is **not**

- **Not a Datadog clone.** It does not aim to copy Datadog's UX or feature surface
  pixel-for-pixel. It aims to make the *job* Datadog does available without the
  *bill* Datadog charges.
- **Not a magic bullet.** Self-hosting observability is a real operational
  commitment. The roadmap is honest about this — see Phase 0 of the build plan.
  For many teams the right answer is still a SaaS until the bill becomes
  unbearable, then Kaleidoscope.
- **Not a single binary.** It is a platform of cooperating components. Each one
  can be replaced, ignored, or run standalone — that's the *point* of the
  OTLP-at-every-seam architecture.

---

## Documentation

- [Research: OTel-compatible observability platforms](docs/research/observability/otel-compatible-observability-platform-comprehensive-research.md) — the comprehensive evidence base.
- [Roadmap: build it all yourself](docs/roadmap/kaleidoscope-from-scratch-roadmap.md) — the 10-phase plan.
- [Tech stack: recommended technologies per component](docs/roadmap/kaleidoscope-tech-stack.md) — what to build it with.

---

## License

License files will land alongside the first code commit. The intent — captured in
the promise above — is **AGPL-3.0** for the platform services (so any hosted
fork must contribute changes back) and **Apache-2.0** for the SDK and protocol
libraries (so anyone, including commercial vendors, can adopt them without
friction). This mirrors the Grafana Labs model and is the most battle-tested
arrangement for keeping infrastructure software free against vendor pressure.

If you have a strong opinion before that LICENSE lands, open an issue.

---

## Contributing

Contribution guidelines will arrive with the first code. Until then, the
research, roadmap, and tech-stack documents are open for review — issues,
critiques, and PRs against them are very welcome. The design is more easily
changed now than later.
