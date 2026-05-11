# Beacon v0 — C4 Context

```mermaid
flowchart TB
    Sasha[Sasha<br/>platform engineer] -->|authors rules in CUE| RulesDir["rules/*.cue<br/>(filesystem)"]
    RulesDir -->|loaded on start + SIGHUP| Beacon[Beacon<br/>alerting engine]
    Beacon -->|PromQL queries<br/>HTTP GET /api/v1/query| Prom["Prometheus / Mimir<br/>(operator-existing)"]
    Beacon -->|incident emission| Webhook["webhook endpoint<br/>(Slack, etc.)"]
    Beacon -->|incident emission| SMTP[SMTP server]
    Beacon -->|incident emission| Mattermost
    Beacon -->|incident emission| Zulip
    Beacon -->|incident emission| OnCall[Grafana OnCall]
    OnCall -->|page| Riley[Riley<br/>SRE on-call]
    SMTP -->|email| Riley
    Mattermost -->|chat| Riley
    Zulip -->|chat| Riley
    Webhook -->|chat / ticket| Riley
    Beacon -.->|OTLP telemetry<br/>self-observability| Aperture[Aperture<br/>(operator-existing)]

    style Beacon fill:#dfd
    style Sasha fill:#eef
    style Riley fill:#eef
```

## System boundary

Beacon v0 sits between the operator's existing PromQL backend
(Prometheus / Mimir) and the operator's existing notification
infrastructure. It does not own data storage and it does not own
notification routing UX — those are the responsibilities of the
backend (Prometheus's retention, Pulse/Lumen later) and Grafana
OnCall (the recommended pager-side UX). Beacon owns the rule
catalogue, the evaluation engine, and the per-sink emission
contract.

## External dependencies

| Dependency | Role | Substitutable? |
|---|---|---|
| Prometheus HTTP API | Data source for query evaluation | Yes — any backend that speaks the `/api/v1/query` shape. Pulse will be the first-party substitute in a later phase. |
| CUE schema engine | Parse + validate rule definitions | The CUE language is the contract; the parser is a substrate dependency the operator does not pick. |
| Sink endpoints (webhook, SMTP, etc.) | Notification delivery | Yes — operator-owned. Beacon's sink trait is the abstraction layer. |
| Aperture | OTLP ingestion of Beacon's own telemetry | Optional. Beacon ships with a no-op telemetry config that can be enabled by environment variable. |

## In-scope at v0

- Rule loading from a `--rules <dir>` directory
- PromQL evaluation against one backend (no multi-backend at v0)
- Five sink adapters with per-rule routing
- Inhibition + grouping primitives
- SLO MWMBR synthesis from a CUE SLO declaration
- Beacon's own OTLP telemetry (configurable, no-op by default)

## Out-of-scope at v0

- Multi-tenancy (Aegis dependency, post-v0)
- Git-backed rule authority (Loom dependency, post-v0)
- Web UI for rule authoring (Loom + Prism dependency, post-v0)
- Auto-discovery of services to alert on (post-v0)
- Hot reloading without operator action (`SIGHUP` is the boundary)
