# Kaleidoscope

> **An OpenTelemetry-compatible observability platform, structurally protected against vendor capture.**

Kaleidoscope refracts every telemetry signal — logs, metrics, traces, profiles —
into a single coherent view. It is built to do the work of Datadog, New Relic,
Splunk, Dynatrace, BetterStack, Honeycomb, Grafana Cloud, Chronosphere, and the
LGTM and ELK stacks combined, and to do it without a per-host bill, a per-GB
bill, a per-cardinality bill, a per-user bill, or a "contact sales" page.

Kaleidoscope is licensed in two classes by component role: platform components
under [AGPL-3.0-or-later](LICENSE-AGPL-3.0), SDKs and protocol libraries under
[Apache-2.0](LICENSE-APACHE-2.0). Contributions are accepted under the
Developer Certificate of Origin; there is no Contributor Licence Agreement and
there will be no Contributor Licence Agreement. The name and logo are reserved
trademarks. See [`LICENSING.md`](LICENSING.md) for the full rationale.

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
every external surface, and is licensed under AGPL-3.0-or-later (platform) and
Apache-2.0 (SDKs) with a no-CLA contribution model — so anyone can use it,
nobody can re-license it later, and the SaaS loophole is closed inside the
OSI-approved perimeter.

---

## Status

**Implementation in progress.** Twenty-six features shipped across the platform
plane. One hundred and thirty-four test suites GREEN on `main`. All six storage
pillars now ship a durable v1 adapter behind the same v0 trait
(`FileBackedLogStore`, `FileBackedQueue`, `FileBackedTieringStore`,
`FileBackedMetricStore`, `FileBackedTraceStore`, `FileBackedProfileStore`), and
the alerting pillar's rule state is durable too (`FileBackedRuleStateStore`), so
a firing alert survives a restart instead of re-paging.

The platform now runs end to end. The `kaleidoscope-gateway` binary receives OTLP
over gRPC and HTTP through the Aperture gateway, validates it against the
conformance harness, and persists each signal into its durable pillar via a
storage `OtlpSink` (logs to Lumen, traces to Ray, metrics to Pulse), so telemetry
sent to the gateway is queryable from the pillars and survives a restart. A
second runnable binary, `kaleidoscope-cli`, wires Lumen v1 + Cinder v1 +
self-observability into an operator-facing ingest / read pipeline.

The read loop closes too. A third binary, `query-api`, serves a
Prometheus-compatible `/api/v1/query_range` HTTP endpoint over the durable
Pulse store, so a metric written through the gateway can be queried back and
plotted by the Prism frontend. The loop is complete: ingest, store, query, see.
`query-api` can also serve Prism's built bundle from the same origin (point
`KALEIDOSCOPE_QUERY_STATIC_DIR` at `apps/prism/dist`), so the whole read side
runs from one binary with no separate web server and no CORS.

The methodology is nWave (DISCUSS → DESIGN → DEVOPS → DISTILL → DELIVER) by Di
Gioia and Brissoni at nWave.ai. Andrea adopts it; the project is the
dogfooding worked example. The long-form narrative companion to the video
series lives in
[`docs/presentation/narrative.md`](docs/presentation/narrative.md); the slide
deck is [`docs/presentation/slides.md`](docs/presentation/slides.md).

**Quick start** with the v1 storage plane behind the CLI:

```bash
cargo build --release -p kaleidoscope-cli

# Ingest NDJSON LogRecord lines from stdin into a durable store.
echo '{"observed_time_unix_nano":100,"severity_number":9,"severity_text":"INFO","body":"hello","attributes":{},"resource_attributes":{"service.name":"checkout"},"trace_id":null,"span_id":null}' \
  | ./target/release/kaleidoscope-cli ingest acme ./data

# Read the records back. Survives process restart.
./target/release/kaleidoscope-cli read acme ./data
```

Or via Docker, with no local Rust toolchain required:

```bash
docker build -t kaleidoscope-cli .

mkdir -p ./data
echo '{"observed_time_unix_nano":100,"severity_number":9,"severity_text":"INFO","body":"hello","attributes":{},"resource_attributes":{"service.name":"checkout"},"trace_id":null,"span_id":null}' \
  | docker run --rm -i -v "$(pwd)/data:/data" kaleidoscope-cli ingest acme /data

docker run --rm -v "$(pwd)/data:/data" kaleidoscope-cli read acme /data
```

The image is a multi-stage build. `rust:1.88-slim-bookworm` compiles the binary
in release mode; `debian:bookworm-slim` carries only the compiled binary, no
toolchain, no source. See [`Dockerfile`](Dockerfile) for details.

| Document | What it is |
|----------|------------|
| [`docs/architecture/kaleidoscope-architecture.md`](docs/architecture/kaleidoscope-architecture.md) | The architectural model. Three views (system context, container view with port boundaries, architectural strata) plus the phasing layer and a glossary. *How* Kaleidoscope is structured. |
| [`docs/roadmap/kaleidoscope-implementation-roadmap.md`](docs/roadmap/kaleidoscope-implementation-roadmap.md) | The implementation roadmap. Per-phase deliverables, exit criteria, dependency graph. *When* Kaleidoscope is built. |
| [`docs/presentation/narrative.md`](docs/presentation/narrative.md) | Long-form narrative of every shipped wave. Companion to the video series. |
| [`docs/presentation/slides.md`](docs/presentation/slides.md) | Slide deck for the video series. |
| [`docs/research/observability/otel-compatible-observability-platform-comprehensive-research.md`](docs/research/observability/otel-compatible-observability-platform-comprehensive-research.md) | Comprehensive, evidence-driven research on building a production-grade OTel-compatible observability platform. 35+ cited sources. |

---

## Components at a glance

Every named component is a part of an optical instrument. The metaphor is the
contract: light enters, reflects, refracts, emerges as a coherent spectrum. The
**Status** column reflects the state of `main`: v0 = in-memory port adapter
shipped behind a stable trait; **v1** = file-backed durable adapter shipped
behind the same trait, surviving process restart. Crates without a v0 yet are
named but not implemented.

| Codename       | Role                                                  | Replaces                                 | Status |
| -------------- | ----------------------------------------------------- | ---------------------------------------- | ------ |
| **Harness**    | OTLP conformance test suite                           | (internal)                               | shipped |
| **Spark**      | Auto-instrumentation SDKs                             | Datadog APM agents, NR APM agents        | v0 |
| **Aperture**   | OTLP-compatible ingest gateway                        | Datadog Agent, Splunk UF, OTel Collector | v0 |
| **Sluice**     | Durable ingest buffer                                 | Datadog's internal queues                | **v1** |
| **Sieve**      | Sampling and filtering                                | Datadog Live Search filters, Honeycomb Refinery | v0 |
| **Codex**      | Schema registry + semantic conventions                | Datadog tags taxonomy                    | v0 |
| **Pulse**      | Time-series metrics engine                            | Datadog Metrics, NR Metrics, Cloud Monitoring | **v1** |
| **Lumen**      | Log storage and search                                | Datadog Logs, Splunk, Loki, Elastic      | **v1** |
| **Ray**        | Distributed trace storage and query                   | Datadog APM, NR Distributed Tracing, Tempo | **v1** |
| **Strata**     | Continuous profiling                                  | Datadog Profiler, NR Code-Level Metrics  | **v1** |
| **Cinder**     | Tier-metadata governor / cold-tier coordinator        | Datadog Flex Logs, S3 Archives           | **v1** |
| **Prism**      | Unified query and visualisation frontend              | Datadog dashboards, NR One, Grafana      | v0 |
| **Beacon**     | Alerting + SLO burn-rate engine                       | Datadog Monitors, NR Alerts, PagerDuty   | **v1** |
| **Augur**      | Anomaly detection / AIops                             | Datadog Watchdog, NR AI                  | v0 |
| **Aegis**      | AuthN/Z, multi-tenancy, audit                         | Datadog RBAC, NR User Management         | v0 |
| **Loom**       | Dashboards-as-code, alert-rules-as-code               | Terraform Datadog provider               | v0 |

Plus six cross-cutting crates: `integration-suite` (cross-crate composition
tests pinning that the platform behaves as one thing), `self-observe`
(`MetricsRecorder` bridges so Kaleidoscope observes itself via its own
primitives), `aperture-storage-sink` (the storage `OtlpSink` translating OTLP
into the durable pillars), `kaleidoscope-cli` (operator-facing ingest / read
binary), `kaleidoscope-gateway` (the runnable OTLP gateway that persists
received telemetry into the pillars), and `query-api` (the
Prometheus-compatible `/api/v1/query_range` read service over Pulse that the
Prism frontend queries).

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

Kaleidoscope is licensed in two classes by component role.

**Platform components — [AGPL-3.0-or-later](LICENSE-AGPL-3.0).** The
server-side components (`aperture`, the future `sieve` / `sluice` / storage
engines / query / alerting / etc.) are released under AGPL-3.0-or-later.
Anyone may use, modify, and redistribute them. Anyone offering them as a
network service to others must publish their modifications under the same
licence. AGPL closes the SaaS loophole that drove Elastic, MongoDB, Redis, and
HashiCorp to abandon open source — inside the OSI-approved perimeter.

**SDKs and protocol libraries — [Apache-2.0](LICENSE-APACHE-2.0).** The
client-side and protocol code (`otlp-conformance-harness`, future `spark`,
generated code, the on-disk format spec) is released under Apache-2.0 so it
can be embedded in proprietary application code without copyleft contamination.
Apache-2.0 also gives an explicit patent grant.

**Contributions — Developer Certificate of Origin.** There is no Contributor
Licence Agreement and there will be no Contributor Licence Agreement. With
many contributors and no concentrated copyright assignment, no future
maintainer or entity can unilaterally re-license Kaleidoscope, because nobody
will own enough of the copyright to legally do it. That is the structural
protection. The licence text alone is necessary but not sufficient.

**Trademark.** The name **Kaleidoscope** and the logo are reserved trademarks
of the project. The code is free; the name and logo are not. This prevents
bad-faith forks claiming to be the original.

The split is the same arrangement Grafana Labs used before AGPL across the
board, and that MongoDB used before they moved to SSPL. It is the most
battle-tested arrangement for keeping infrastructure software free against
vendor pressure.

For the full rationale and the per-crate licence table, see
[`LICENSING.md`](LICENSING.md).

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

When contribution opens, the model is simple: contributions are accepted under
the [Developer Certificate of Origin](https://developercertificate.org/). Each
commit is signed off (`Signed-off-by: Name <email>`) which asserts the
contributor has the right to submit the work under the project's licence.
There is no Contributor Licence Agreement, no copyright assignment, and there
will not be one. The contribution licence is the same as the file's licence:
AGPL-3.0-or-later for platform components, Apache-2.0 for SDKs and protocol
libraries.

---

*Made with ❤️ with [nWave](https://nwave.ai).*
