# Kaleidoscope

> **An OpenTelemetry-compatible observability platform, dedicated to the public domain.**

Kaleidoscope refracts every telemetry signal — logs, metrics, traces, profiles —
into a single coherent view. It is built to do the work of Datadog, New Relic,
Splunk, Dynatrace, BetterStack, Honeycomb, Grafana Cloud, Chronosphere, and the
LGTM and ELK stacks combined, and to do it without a per-host bill, a per-GB
bill, a per-cardinality bill, a per-user bill, or a "contact sales" page.

Kaleidoscope is dedicated to the public domain under [CC0-1.0](LICENSE). Use
it. Modify it. Run it commercially. Fork it and sell hosted versions. Brand it
as your own. Attribute the project or do not. There is no licence to comply
with and no obligations to track.

---

## What Kaleidoscope is

Kaleidoscope is an end-to-end observability platform built around the
**OpenTelemetry** project's wire formats and semantic conventions. Applications
emit telemetry through the OpenTelemetry SDKs; Kaleidoscope receives it as OTLP,
processes it, stores it in its own first-party storage engines, and exposes it
through query, alerting, and visualisation services that Kaleidoscope owns from
top to bottom.

It is composed of fifteen named components, each named after a part of an
optical instrument. Together they implement the four pillars of observability
(logs, metrics, traces, profiles) plus the cross-cutting concerns of ingest,
buffering, sampling, schema, alerting, anomaly detection, identity, cold storage,
and configuration as code. The architecture is summarised in the
[Components at a glance](#components-at-a-glance) section below and detailed in
the [implementation roadmap](docs/roadmap/kaleidoscope-implementation-roadmap.md).

### How Kaleidoscope is built

Three architectural commitments define the project.

**Built from scratch, not assembled.** Kaleidoscope's fifteen components are
first-party Kaleidoscope code, not thin wrappers around peer projects. Pulse is
not a re-skinned Mimir. Lumen is not a re-skinned Loki. Ray is not a re-skinned
Tempo. Prism is not a re-skinned Grafana. Each component is a service
Kaleidoscope owns, ships, and is solely responsible for.

**Built on FOSS libraries, not on FOSS platforms.** A library is code Kaleidoscope
embeds; a platform is a service Kaleidoscope would have to depend on. Apache
Arrow, Apache Parquet, Apache DataFusion, Apache Iceberg, Tokio, Hyper, Tonic,
RocksDB, FoundationDB, NATS JetStream, and Apache Kafka are libraries (or
self-contained engines) that Kaleidoscope embeds. ClickHouse, Mimir, Loki, Tempo,
Prometheus, Grafana, and Elasticsearch are *peers* that Kaleidoscope competes
with and therefore does not consume.

**Implements OpenTelemetry standards everywhere.** The wire contract between
every external component and Kaleidoscope, and between every internal component
of Kaleidoscope, is an OpenTelemetry-defined format. Ingest is OTLP. Resource
and instrumentation attributes follow OpenTelemetry Semantic Conventions.
Profiles use the pprof format and the emerging OpenTelemetry Profiles signal.

---

## Why this exists

Modern observability has a cost problem. The tools that watch production are
themselves a recurring six- or seven-figure line item for any non-trivial
business. The pricing models — per-host, per-GB, per-custom-metric,
per-cardinality, per-seat — punish exactly the engineering practices the same
vendors evangelise: rich instrumentation, high-fidelity tracing, long retention,
broad team access.

Open-source alternatives exist (the LGTM stack, the ELK stack, ClickHouse-based
projects like SigNoz and Uptrace, OpenTelemetry itself). They are excellent.
But they are also fragmented: many projects, many query languages, many
operational paradigms, many storage engines. Adopting them well is itself a
specialist skill.

Kaleidoscope is the integrated alternative. It owns its fifteen components end
to end, depends only on FOSS libraries, exposes OpenTelemetry standards at
every external surface, and is dedicated to the public domain so anyone can use
it without negotiation.

---

## Status

**Pre-implementation.** This repository currently contains the design corpus.
Code lands when the design is settled.

| Document | What it is |
|----------|------------|
| [`docs/architecture/kaleidoscope-architecture.md`](docs/architecture/kaleidoscope-architecture.md) | The architectural model. Three views (system context, container view with port boundaries, architectural strata) plus the phasing layer and a glossary. *How* Kaleidoscope is structured. |
| [`docs/roadmap/kaleidoscope-implementation-roadmap.md`](docs/roadmap/kaleidoscope-implementation-roadmap.md) | The implementation roadmap. Per-phase deliverables, exit criteria, dependency graph. *When* Kaleidoscope is built. |
| [`docs/research/observability/otel-compatible-observability-platform-comprehensive-research.md`](docs/research/observability/otel-compatible-observability-platform-comprehensive-research.md) | Comprehensive, evidence-driven research on building a production-grade OTel-compatible observability platform. 35+ cited sources. |

---

## Components at a glance

Every component is named after a part of an optical instrument. The metaphor is
the contract: light enters, reflects, refracts, emerges as a coherent spectrum.

| Codename       | Role                                                  | Replaces                                 |
| -------------- | ----------------------------------------------------- | ---------------------------------------- |
| **Spark**      | Auto-instrumentation SDKs                             | Datadog APM agents, NR APM agents        |
| **Aperture**   | OTLP-compatible ingest gateway                        | Datadog Agent, Splunk UF, OTel Collector |
| **Sluice**     | Durable ingest buffer                                 | Datadog's internal queues                |
| **Sieve**      | Sampling and filtering                                | Datadog Live Search filters, Honeycomb Refinery |
| **Codex**      | Schema registry + semantic conventions                | Datadog tags taxonomy                    |
| **Pulse**      | Time-series metrics engine                            | Datadog Metrics, NR Metrics, Cloud Monitoring |
| **Lumen**      | Log storage and search                                | Datadog Logs, Splunk, Loki, Elastic      |
| **Ray**        | Distributed trace storage and query                   | Datadog APM, NR Distributed Tracing, Tempo |
| **Strata**     | Continuous profiling                                  | Datadog Profiler, NR Code-Level Metrics  |
| **Cinder**     | Cold-tier object-storage adapter                      | Datadog Flex Logs, S3 Archives           |
| **Prism**      | Unified query and visualisation frontend              | Datadog dashboards, NR One, Grafana      |
| **Beacon**     | Alerting + SLO burn-rate engine                       | Datadog Monitors, NR Alerts, PagerDuty   |
| **Augur**      | Anomaly detection / AIops                             | Datadog Watchdog, NR AI                  |
| **Aegis**      | AuthN/Z, multi-tenancy, audit                         | Datadog RBAC, NR User Management         |
| **Loom**       | Dashboards-as-code, alert-rules-as-code               | Terraform Datadog provider               |

See the [implementation roadmap](docs/roadmap/kaleidoscope-implementation-roadmap.md)
for the data-flow diagram, the build-order DAG, and the phased build plan.

---

## How Kaleidoscope defeats the cost model

The big vendors charge for things that, in a well-built FOSS platform, are not
expensive:

| The vendor charges for…                                  | Kaleidoscope's answer                                                  |
| -------------------------------------------------------- | ---------------------------------------------------------------------- |
| Per-host agent licences                                  | Spark is an SDK. There is no per-host fee, ever.                       |
| Per-GB log ingest, with surge pricing                    | Lumen is a first-party log engine on Apache Parquet in your object storage. You pay the cloud storage bill. |
| Custom metrics over a low free quota                     | Pulse has no metric-count surcharge. Your TSDB has whatever cardinality your hardware supports. |
| Per-million-span APM                                     | Ray charges nothing per span; Sieve drops what you don't need.         |
| Continuous profiling as a top-tier add-on                | Strata is included.                                                    |
| Long-term retention as a separate "Flex" / "Archive" SKU | Cinder's tiering is built in; cold storage is just S3 / GCS / R2.      |
| Per-user dashboard seats                                 | Prism has no seat licensing.                                           |
| SSO, RBAC, audit log, SAML/SCIM as "Enterprise" tier     | Aegis is in the free product. Always.                                  |
| AIops / anomaly detection as an upsell                   | Augur is included; bring your own model if you want a fancier one.     |
| "Contact sales" for compliance reports                   | The compliance dashboards in Prism are open templates.                 |

The structural cost of running Kaleidoscope is the cost of the underlying
compute and storage, which is the cloud bill the vendors are also paying, plus
their margin. Removing the margin is the entire economic thesis. Kaleidoscope
itself is free; the cloud underneath is not.

---

## What Kaleidoscope is **not**

- **Not a Datadog clone.** It does not aim to copy Datadog's UX or feature surface
  pixel-for-pixel. It aims to make the *job* Datadog does available without the
  *bill* Datadog charges.
- **Not a magic bullet.** Self-hosting observability is a real operational
  commitment. The roadmap is honest about this. For many teams the right answer
  is still a SaaS until the bill becomes unbearable, then Kaleidoscope.
- **Not a single binary.** It is a platform of cooperating components. Each one
  can be replaced, ignored, or run standalone — that is the *point* of the
  OTLP-at-every-seam architecture.
- **Not a wrapper around an existing OSS stack.** Kaleidoscope is not Mimir +
  Loki + Tempo + Pyroscope + Grafana with a new logo. Those are peer projects
  Kaleidoscope competes with and therefore does not consume.

---

## Licensing

Kaleidoscope is dedicated to the public domain under
[CC0-1.0](LICENSE) (Creative Commons Zero, version 1.0).

You may use, copy, modify, distribute, and run the project for any purpose,
commercial or non-commercial, without permission and without attribution. CC0
includes a permissive-licence fallback for jurisdictions where public-domain
dedication is not legally recognised (most of continental Europe, parts of the
UK), so the practical result is the same everywhere: no obligations.

Code dedicated to the public domain under CC0 cannot be un-dedicated. Whatever
happens to this project in the future, the existing code remains permanently
in the public domain. Forks may continue under any licence the fork chooses,
including restrictive ones; the original Kaleidoscope code does not become
restricted with them.

---

## Documentation

- [Research: OTel-compatible observability platforms](docs/research/observability/otel-compatible-observability-platform-comprehensive-research.md) — the comprehensive evidence base.
- [Implementation roadmap](docs/roadmap/kaleidoscope-implementation-roadmap.md) — the canonical phased plan, build-vs-vendor decisions, port-and-adapter architecture, integration-plane-first phasing.

---

## Contributing

Kaleidoscope is currently a single-author project. External contributions,
including pull requests, are not yet accepted. The repository is public so the
design can be observed and read. Star or watch the repository to be notified
when contribution opens.

When contribution opens, the model is simple: contributions to a public-domain
project are themselves dedicated to the public domain on submission. There is
no CLA, no DCO, no copyright assignment. By submitting work to the repository
you are simply releasing it to the public domain alongside the rest.

---

*Made with ❤️ with [nWave](https://nwave.ai).*
