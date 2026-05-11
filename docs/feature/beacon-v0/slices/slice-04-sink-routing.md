# Slice 04 — Multi-sink routing (US-BE-04)

## Goal

Five sink adapters: webhook (already from slice 01), SMTP,
Mattermost, Zulip, OnCall. Per-rule routing. Independent delivery
with per-sink retry. Header-redaction invariant.

## IN scope

- `Sink` trait: `async fn emit(&self, incident: &Incident) -> Result<(), SinkError>`
- Five implementations: `WebhookSink`, `SmtpSink`, `MattermostSink`,
  `ZulipSink`, `OnCallSink`
- Per-rule `sinks` field in CUE: list of sink references with the
  adapter kind + adapter-specific config
- Secret material via environment variables named in CUE (e.g.
  `password_env: "SMTP_PASSWORD"`); never inline in CUE
- Retry on transient failure: 3 attempts with exponential backoff
  1s / 5s / 30s; permanent failure (4xx / config error) records and
  moves on
- Per-sink format: Mattermost Markdown, SMTP multipart, OnCall JSON,
  Zulip topic-keyed, webhook canonical JSON
- Header redaction invariant: 5-arm property test asserting no
  configured auth header value reaches any sink emission body
- OTLP telemetry of Beacon itself: per-sink emission spans with
  `sink.kind`, `sink.status`, `sink.latency_ms` attributes
- Integration test `slice_04_sink_routing.rs` with five fake sinks
  exercising the 60-incident burst

## OUT scope

- SLO burn-rate (slice 05)
- Tenant scoping (post-v0)

## Learning hypothesis

Disproves "the `Sink` trait can abstract five different protocols
without leaking adapter-specific concerns into the canonical
incident shape". Risk: SMTP's multipart requirement may force a
trait method addition for "format-as-string" + "format-as-html",
breaking the clean signature. If so, the trait grows; the slice
still ships, with an ADR documenting the shape change.

## Acceptance criteria

US-BE-04 AC-4.1 through AC-4.5.
