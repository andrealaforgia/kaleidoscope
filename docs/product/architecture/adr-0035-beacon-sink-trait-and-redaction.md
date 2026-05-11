# ADR-0035 — Beacon sink trait and header redaction

**Status**: Accepted
**Date**: 2026-05-11
**Author**: Bea (autonomous DESIGN dispatch)

## Context

Beacon v0 ships five sink adapters: webhook, SMTP, Mattermost,
Zulip, Grafana OnCall. Each accepts a canonical `Incident` and
formats it for the target protocol. The sink trait must:

1. Abstract the five protocols cleanly without leaking
   adapter-specific concerns into the canonical incident shape
2. Allow independent per-sink failure handling (a slow Mattermost
   does not block webhook delivery)
3. Honour the header-redaction discipline established by Prism
   ADR-0027 §6: no auth header value flows into any sink
   emission's body
4. Accept secret material via environment variables named in CUE,
   not inline CUE values

## Decision

The sink trait is:

```rust
#[async_trait::async_trait]
pub trait Sink: Send + Sync {
    /// Deliver an incident to this sink. Implementations are
    /// independent — a failure here does not affect other sinks.
    async fn emit(&self, incident: &Incident) -> Result<(), SinkError>;

    /// The kind discriminator for telemetry + routing.
    fn kind(&self) -> SinkKind;
}

#[derive(Debug, Clone, Copy)]
pub enum SinkKind {
    Webhook,
    Smtp,
    Mattermost,
    Zulip,
    OnCall,
}

#[derive(Debug)]
pub enum SinkError {
    Transient { retry_after: Duration, cause: Box<dyn Error + Send + Sync> },
    Permanent { cause: Box<dyn Error + Send + Sync> },
}
```

The canonical `Incident`:

```rust
#[derive(Debug, Clone, Serialize)]
pub struct Incident {
    pub name: String,
    pub query: String,
    pub severity: Severity,
    pub labels: BTreeMap<String, String>,
    pub annotations: BTreeMap<String, String>,
    pub started_at: SystemTime,
    pub resolved_at: Option<SystemTime>,
    pub inhibitor: Option<String>,
    pub inhibited: Vec<String>,
}
```

The `Incident` is the contract: every sink emits this shape, with
per-sink formatting applied by the adapter. The contract is
language-neutral — the incident's JSON is the operator-readable
audit trail.

## Retry discipline

The binary's emitter loop interprets `SinkError`:

- `Transient`: retry up to 3 times with exponential backoff
  (1s / 5s / 30s). After exhausting retries, record the failure
  to Beacon's OTLP telemetry and continue.
- `Permanent`: record the failure immediately, no retry.

The classification is the sink's responsibility. HTTP 5xx is
`Transient`; HTTP 4xx is `Permanent` (config error). SMTP transient
errors (e.g. greylisting) are `Transient`; SMTP permanent
rejections (5.x.x codes) are `Permanent`.

## Header redaction (load-bearing invariant)

The sink layer redacts any value of any configured outbound HTTP
header (or SMTP authentication credential) from every string
flowing into an emission body. The redaction tokenises header
values on whitespace and replaces every token of length ≥ 4 with
`***`. Same algorithm as Prism's `redactHeaderValues` in
`queryRange.ts` (ADR-0027 §6).

The invariant is verified by a 5-arm property test exercising every
sink kind with a fake adapter that echoes outbound headers in its
captured emission body. The test asserts `JSON.stringify(emission)`
never contains the configured header value.

## Secret material via environment variables

Adapter-specific secrets are referenced in CUE by environment
variable name, never inline:

```cue
sinks: [{
    kind: "smtp"
    host: "smtp.acme.internal"
    port: 587
    from: "beacon@acme.internal"
    password_env: "ACME_SMTP_PASSWORD"
}]
```

At adapter construction time, the binary reads the named env var
and stores the secret in the adapter instance. The CUE file never
contains the secret. The operator-readable rule catalogue is
publishable in Git without leaking credentials.

## Format per sink kind

| Sink | Format | Notes |
|---|---|---|
| Webhook | JSON (canonical `Incident`) | The most direct format. POST with `Content-Type: application/json`. |
| SMTP | Multipart (plain-text + HTML) | Subject = `[{severity}] {name}`. Plain-text body is the incident as YAML. HTML body is a styled summary table. |
| Mattermost | Markdown via webhook URL | Code block for the query. Severity badge. Runbook link if present. |
| Zulip | Topic-keyed plain-text | Topic = `{name}`. Body is plain text. |
| OnCall | OnCall webhook JSON shape | The published OnCall webhook schema; `alert_uid`, `title`, `state`, `severity` mapped from the incident. |

## Consequences

- Five adapters, one trait. Adding a sixth sink (e.g. PagerDuty —
  but PagerDuty is SaaS so out of scope) is one new file
  implementing `Sink`, no changes to the evaluator.
- Header redaction is uniform across all sinks; the property test
  catches regressions when a future adapter forgets to redact.
- Operator-readable rules can be Git-tracked without leaking
  secrets — the env-var indirection is the contract.

## Alternatives considered

- **One sink, configurable**. Rejected: protocol differences are
  large enough that a configurable single sink would push protocol
  logic into the configuration layer, making the catalogue harder
  to read.
- **External notification routing service**. Rejected at v0:
  Beacon owning the sink topology keeps the v0 deployment
  self-contained. A future v2 could delegate to an Alertmanager-
  shaped router, but the architecture doc §C.12 explains why
  Beacon owns this layer.
