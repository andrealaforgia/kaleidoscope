# ADR-0033 — Beacon public API and crate layout

**Status**: Accepted
**Date**: 2026-05-11
**Author**: Bea (autonomous DESIGN dispatch)
**Companion feature**: `docs/feature/beacon-v0/`

## Context

Beacon is the rule-evaluation engine that reads from any
OTel-compatible PromQL backend, evaluates CUE-defined alert and SLO
burn-rate rules, and emits incidents to standard sinks. Its v0
shape needs to support:

1. Embedding in a thin `beacon-server` binary that wires the
   evaluator to a real HTTP backend, a `tokio::time::interval`
   scheduler, and `SIGHUP` reload
2. Future embedding by host processes that want to drive the
   evaluator directly without the daemon wrapping (e.g. a
   monitoring sidecar)
3. Property testing of the pure components (evaluator, inhibition,
   SLO synthesiser) without IO

The pattern is established by Aperture (library + service), Sieve
(library + integration), Codex (library), and Prism (pure cores +
Scheduler seam). Beacon follows the same shape.

## Decision

Beacon ships as a Rust workspace with two crates:

- `crates/beacon` — the library. Exposes:
  - `pub fn load_rules(dir: &Path) -> Result<RuleSet, LoadError>`
  - `pub fn load_slo(path: &Path) -> Result<Slo, LoadError>`
  - `pub fn synthesise_slo(slo: &Slo) -> Vec<Rule>`
  - `pub async fn evaluate(rules: &RuleSet, fetch: F, now: SystemTime) -> EvaluationResult`
    where `F: Fn(&Rule) -> Future<Output=Result<PromResponse, FetchError>>`
  - `pub trait Sink { async fn emit(&self, incident: &Incident) -> Result<(), SinkError>; }`
  - `pub struct WebhookSink { ... }`, `SmtpSink`, `MattermostSink`,
    `ZulipSink`, `OnCallSink` — all implementing `Sink`
  - `pub struct EvaluationResult { pub fired: Vec<Incident>, pub
     resolved: Vec<Incident>, pub inhibited: Vec<Incident> }`

- `crates/beacon-server` — the binary. Wires `beacon` to:
  - a `tokio` runtime
  - `reqwest` for the PromQL HTTP client
  - `tokio::time::interval` for the scheduler
  - a `SIGHUP` handler that reloads the rule set
  - optional OTLP telemetry exporter (env-gated)

The library cannot depend on `tokio` runtime types in its public
API. The evaluator is `async fn` and works with any executor; the
`Sink` trait is `async fn` and uses no Tokio-specific types. The
binary owns the runtime choice.

## Consequences

- The library is unit-testable without spinning up a runtime
  beyond the test's own. Property tests of the evaluator,
  inhibition, and SLO synthesiser run as pure-function tests.
- The binary is small (≤ 300 lines) and re-implementable by
  embedders who want to substitute their own scheduler or HTTP
  client.
- Public surface is locked by `cargo public-api` (Gate 2) at the
  library boundary, mirroring Codex and Aperture.
- SemVer-checks (Gate 3) enforce that breaking changes to the
  public API bump the major version, mirroring every prior
  feature.

## Alternatives considered

- **Single crate.** Rejected: mixing the runtime-owning binary
  with the library forces every consumer to pull in `tokio` and
  `reqwest`. The two-crate split makes the library substitutable.
- **Multiple sink crates.** Rejected at v0: five sink adapters fit
  cleanly inside one crate as a `sinks/` module. A future v1 may
  promote them to separate crates if the dependency surface
  (e.g. `lettre` for SMTP) becomes a build-time concern.
