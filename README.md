# Kaleidoscope

> **An OpenTelemetry-compatible observability platform that is free, open source,
> and will always be free and open source.**

Kaleidoscope refracts every telemetry signal — logs, metrics, traces, profiles —
into a single coherent view. It is built to do the work of Datadog, New Relic,
Splunk, Dynatrace, BetterStack, Honeycomb, Grafana Cloud, Chronosphere, and the
LGTM and ELK stacks combined, and to do it without a per-host bill, a per-GB
bill, a per-cardinality bill, a per-user bill, or a "contact sales" page.

---

## The promise

**Kaleidoscope is free and open source software, and that will not change.**

- **Permissively or copyleft-licensed forever.** Every component is shipped under an
  OSI-approved licence. The project will never re-license to SSPL, BSL, "Source
  Available", or any other shared-source-but-not-actually-open arrangement. The
  pattern of "open until the VCs need an exit, then closed" — Elastic 2021, MongoDB
  2018, Redis 2024, HashiCorp 2023, Cockroach 2024, Sentry 2019, Confluent 2018 — is
  the precise pattern Kaleidoscope is designed to refuse.
- **No "open core" with locked enterprise features.** Multi-tenancy, SSO, audit
  logs, RBAC, alerting, SLOs, retention controls, dashboards-as-code: *all of it*
  is in the free product. There is no paid tier where the features that make the
  platform usable in production live.
- **No telemetry-on-telemetry.** Kaleidoscope does not phone home. The platform does
  not collect anonymous usage data, license-key validation, "anonymous crash
  reports", or any other vendor-side telemetry. What the platform learns about
  itself, it learns through itself.
- **No "free for non-commercial use" weasel words.** Run Kaleidoscope at a startup,
  inside a Fortune 500, as a consultancy, as a managed SaaS reseller, in
  air-gapped government compute: same software, same licence, same rights.
- **Any future hosted offering will run the same code.** If the maintainers
  ever offer a managed Kaleidoscope, it will be packaged from the same Git tags
  you can `git clone` and run on your own hardware. There is no closed-source
  "control plane" hiding the differentiating features.

These commitments are encoded structurally, not just promised. See
[How Kaleidoscope is protected](#how-kaleidoscope-is-protected) below.

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
the [implementation roadmap](docs/roadmap/kaleidoscope-foss-implementation-roadmap.md).

### How Kaleidoscope is built

Three architectural commitments define the project and rule out a wide class of
shortcuts most observability projects take.

**Built from scratch, not assembled.** Kaleidoscope's fifteen components are
first-party Kaleidoscope code, not thin wrappers around peer projects. Pulse is
not a re-skinned Mimir. Lumen is not a re-skinned Loki. Ray is not a
re-skinned Tempo. Prism is not a re-skinned Grafana. Each component is a service
Kaleidoscope owns, ships, and is solely responsible for. This is the only way
the FOSS-forever promise survives contact with the temptation to vendor a "small
bit" of an upstream platform that has already been re-licensed once.

**Built on FOSS libraries, not on FOSS platforms.** A library is code Kaleidoscope
embeds; a platform is a service Kaleidoscope would have to depend on. Apache
Arrow, Apache Parquet, Apache DataFusion, Apache Iceberg, Tokio, Hyper, Tonic,
RocksDB, FoundationDB, NATS JetStream, and Apache Kafka are libraries (or
self-contained engines) that Kaleidoscope embeds and re-implements behaviour
around. ClickHouse, Mimir, Loki, Tempo, Prometheus, Grafana, and Elasticsearch
are *peers* that Kaleidoscope competes with and therefore cannot consume.

**Implements OpenTelemetry standards everywhere.** The wire contract between
every external component and Kaleidoscope, and between every internal component
of Kaleidoscope, is an OpenTelemetry-defined format. Ingest is OTLP (gRPC and
HTTP). Resource and instrumentation attributes follow OpenTelemetry Semantic
Conventions. Profiles use the pprof format and the emerging OpenTelemetry
Profiles signal. Where OpenTelemetry has not yet specified a contract,
Kaleidoscope follows the closest OpenTelemetry pattern and contributes back
upstream.

These three commitments together are the reason Kaleidoscope exists as a
distinct project and not as another distribution of an existing OSS bundle.

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
specialist skill, and several of them are licensed under terms (BSL, SSPL, "open
core") that make them unreliable foundations for a project that promises to stay
open forever.

Kaleidoscope is the integrated alternative. It owns its fifteen components end
to end, depends only on FOSS libraries with OSI-approved licences, exposes
OpenTelemetry standards at every external surface, and is licensed and governed
to make re-licensing structurally hard. The cost of running production
observability on Kaleidoscope trends toward the cost of the underlying compute
and storage, and not a dollar more.

---

## Status

**Pre-implementation.** This repository currently contains the design corpus.
Code lands when the design is settled.

| Document | What it is |
|----------|------------|
| [`docs/research/observability/otel-compatible-observability-platform-comprehensive-research.md`](docs/research/observability/otel-compatible-observability-platform-comprehensive-research.md) | Comprehensive, evidence-driven research on building a production-grade OTel-compatible observability platform. 35+ cited sources. Reviewed and approved by `nw-researcher-reviewer`. |
| [`docs/roadmap/kaleidoscope-foss-implementation-roadmap.md`](docs/roadmap/kaleidoscope-foss-implementation-roadmap.md) | The canonical implementation roadmap. FOSS-strict, build-from-scratch, OpenTelemetry-everywhere. Includes per-component build-vs-vendor decisions, ten-phase plan, licence audit, FOSS replacement table, anti-pattern register. |

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
| **Ray**   | Distributed trace storage and query                   | Datadog APM, NR Distributed Tracing, Tempo |
| **Strata**     | Continuous profiling                                  | Datadog Profiler, NR Code-Level Metrics  |
| **Cinder**     | Cold-tier object-storage adapter                      | Datadog Flex Logs, S3 Archives           |
| **Prism**      | Unified query and visualisation frontend              | Datadog dashboards, NR One, Grafana      |
| **Beacon**     | Alerting + SLO burn-rate engine                       | Datadog Monitors, NR Alerts, PagerDuty   |
| **Augur**      | Anomaly detection / AIops                             | Datadog Watchdog, NR AI                  |
| **Aegis**      | AuthN/Z, multi-tenancy, audit                         | Datadog RBAC, NR User Management         |
| **Loom**       | Dashboards-as-code, alert-rules-as-code               | Terraform Datadog provider               |

See the [implementation roadmap](docs/roadmap/kaleidoscope-foss-implementation-roadmap.md)
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
| Per-million-span APM                                     | Ray charges nothing per span; Sieve drops what you don't need.    |
| Continuous profiling as a top-tier add-on                | Strata is included.                                                    |
| Long-term retention as a separate "Flex" / "Archive" SKU | Cinder's tiering is built in; cold storage is just S3 / GCS / R2.      |
| Per-user dashboard seats                                 | Prism has no seat licensing.                                           |
| SSO, RBAC, audit log, SAML/SCIM as "Enterprise" tier     | Aegis is in the free product. Always.                                  |
| AIops / anomaly detection as an upsell                   | Augur is included; bring your own model if you want a fancier one.     |
| "Contact sales" for compliance reports                   | The compliance dashboards in Prism are open templates.                 |

The structural cost of running observability on Kaleidoscope is the cost of the
underlying compute and storage, which is the cloud bill the vendors are also
paying, plus their margin. Removing the margin is the entire economic thesis.

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
  Kaleidoscope competes with and therefore cannot consume.

---

## Licensing

Kaleidoscope adopts a deliberate three-licence split that mirrors the most
battle-tested arrangement in the FOSS observability ecosystem (the Grafana Labs
model, in continuous use under vendor pressure since 2021).

| Concern                                                                                       | Licence                | Rationale                                                                                                  |
| --------------------------------------------------------------------------------------------- | ---------------------- | ---------------------------------------------------------------------------------------------------------- |
| Platform services (Aperture, Sluice, Sieve, Codex server, Pulse, Lumen, Ray, Strata, Cinder, Prism, Beacon, Augur, Aegis, Loom) | **AGPL-3.0**           | Network-use-as-distribution closes the SaaS loophole. A vendor that runs a hosted Kaleidoscope must publish its modifications. This is the precise safeguard that SSPL and BSL tried but failed to achieve in OSI-acceptable form. |
| SDKs (Spark) and protocol/format libraries (OTLP libraries, Codex client)                      | **Apache-2.0**         | SDKs run inside customer applications, including closed-source applications. Apache-2.0 grants explicit patent licences and is the standard expectation for code that links into third-party binaries. |
| Specifications (the on-disk format documents, Codex schema spec, OpenTelemetry contributions Kaleidoscope authors) | **CC-BY-4.0**          | Documents are not code. Specifications must be implementable by anyone, including commercial competitors, without copyleft contagion. |
| Trademarks ("Kaleidoscope" name, the logo)                                                    | **Trademark-protected, separate from code licence** | The licence guarantees freedom of code. The trademark guarantees the project name is not used to endorse a fork that has departed from the FOSS contract. |

`LICENSE`, `LICENSE-APACHE`, `LICENSE-CC-BY` files will land at the repository
root alongside the first code commit. Each component subdirectory will carry a
`LICENSE` symlink or stub naming its specific licence.

The full licence audit, including every transitive dependency in the project's
recommended stack with primary-source verification, is in
[Section F of the implementation roadmap](docs/roadmap/kaleidoscope-foss-implementation-roadmap.md#f-licence-audit-appendix).

---

## How Kaleidoscope is protected

Promises in a README are not enforcement. The protections below are the
structural mechanisms that make the FOSS-forever promise hold against the
specific failure modes that have re-licensed every comparable project in the
last decade.

| Mechanism                              | What it prevents                                                                 | Where it lives                                                                                                                                           |
| -------------------------------------- | -------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **DCO, not CLA**                       | A single corporate steward holding copyright assignment over all contributions and using it to re-license. | `CONTRIBUTING.md` requires `Signed-off-by:` on every commit. No CLA bot, no copyright-assignment form. |
| **Supermajority licence-change rule**  | A simple majority vote, hostile acquirer, or single-board takeover re-licensing the project. | `GOVERNANCE.md` requires unanimous active-maintainer consent and a six-month public consultation before any licence change. |
| **No single-employer maintainer dominance** | One company packing the maintainer roster and steering the project to its commercial interest. | `GOVERNANCE.md` caps any single employer at one-third of active maintainer voting weight, after the CNCF TOC pattern. |
| **Foundation-transfer as canonical exit** | Project being sold to a commercial entity. | `GOVERNANCE.md` names the Linux Foundation, the CNCF, or the Apache Software Foundation as the only acceptable destinations if a steward beyond the current maintainers is ever needed. |
| **Trademark held separately from code** | A fork using the project name to mislead users about lineage or compliance. | The "Kaleidoscope" mark is held by a non-profit governance entity, not by any individual or commercial sponsor. |
| **CI-enforced licence audit**          | A transitive dependency under BSL, SSPL, or another disqualifying licence sneaking into the build. | Every component's CI runs `cargo-deny`, `go-licenses`, or equivalent on every PR; the build fails if any disqualifying licence appears anywhere in the dependency tree. |
| **No telemetry-on-telemetry**          | Hidden vendor-side data collection that the project later monetises or weaponises. | No phone-home, no usage tracking, no analytics on the project website (self-hosted analytics only). |
| **Specifications as documents, not code** | A specification getting tangled in code copyleft and becoming unimplementable for downstream third parties. | Specifications are CC-BY-4.0; anyone, including commercial competitors, can implement them. |

The full structural rationale for each mechanism is in
[Section A of the implementation roadmap](docs/roadmap/kaleidoscope-foss-implementation-roadmap.md#a-the-foss-contract).

---

## Documentation

- [Research: OTel-compatible observability platforms](docs/research/observability/otel-compatible-observability-platform-comprehensive-research.md) — the comprehensive evidence base.
- [Implementation roadmap (FOSS-strict)](docs/roadmap/kaleidoscope-foss-implementation-roadmap.md) — the canonical phased plan, build-vs-vendor decisions, licence audit, FOSS replacement table.

---

## Contributing

Contribution guidelines will arrive with the first code commit. The contribution
model is **DCO sign-off only, no CLA**: every commit must include a
`Signed-off-by:` line under the Developer Certificate of Origin (version 1.1,
[developercertificate.org](https://developercertificate.org/)). No future
maintainer, no future board, no future acquirer can unilaterally re-license a
DCO-governed codebase.

Until the first code lands, the research and roadmap documents are open for
review. Issues, critiques, and PRs against them are welcome. The design is more
easily changed now than later.
