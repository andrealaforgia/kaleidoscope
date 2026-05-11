# Slice 01 — Walking skeleton (US-BE-01)

> **Wave**: DISCUSS — Phase 2.5.
> **Companion story**: US-BE-01.
> **Companion ADRs**: 0033 (PromQL HTTP client — shared shape with
> Prism ADR-0027), 0034 (CUE rule schema, single-rule slice).

## Goal

One CUE rule loaded from disk, evaluated against a real Prometheus
HTTP API on a 30 s tick, with one webhook emission on `Firing` and
one on `Resolved`. This is the minimum slice that proves Beacon's
load → evaluate → emit loop works end-to-end against a real
backend.

## IN scope

- `beacon-server` binary entry point with `--rules <dir>` argument
- CUE loader for a single canonical rule shape:
  `{name, query, for, interval, severity, sinks: [{kind: "webhook", url}]}`
- Rule evaluator that runs on a `tokio::time::interval` and queries
  the Prometheus HTTP API at `/api/v1/query`
- State machine: `Inactive → Pending(since) → Firing(since) → Resolved`
- One webhook sink that POSTs JSON to the configured URL
- AGPL-3.0 file headers on every `.rs` and `.cue` file
- A `crates/beacon/tests/slice_01_walking_skeleton.rs` integration
  test that runs the binary against a real `prometheus:v2.55`
  container fixture (digest-pinned, same posture as Prism's
  Playwright Prom container)

## OUT scope (deferred)

- More than one rule (slice 02)
- More than one sink kind (slice 04)
- Inhibition, grouping (slice 03)
- SLO synthesis (slice 05)
- Tenant scoping (Aegis-v0 dependency, post-v0)
- Reload on SIGHUP (slice 02)
- OTLP telemetry of Beacon itself (slice 04 in scope; slice 01 ships
  with `tracing` stdout only)

## Learning hypothesis

This slice disproves "Beacon can complete one load → evaluate →
emit cycle against a real Prometheus HTTP API". If it fails, the
candidate failure modes are:

- The `prometheus-http-query` crate's API does not fit Beacon's
  evaluator shape → rewrite the client
- The CUE loader (`cue-rs` or hand-rolled) produces an unworkable
  `Rule` struct → choose a different deserialiser
- The webhook sink's retry behaviour conflicts with the
  evaluator's tick loop → re-architect the sink trait

## Acceptance criteria

(From US-BE-01: AC-1.1 through AC-1.6.)

- AC-1.1: Load failure produces an operator-readable diagnostic
- AC-1.2: A single rule produces a `Rule` struct with canonical fields
- AC-1.3: The evaluator's state machine transitions are correct
- AC-1.4: Exactly one POST on `Firing` transition
- AC-1.5: Exactly one POST on `Resolved` transition
- AC-1.6: Webhook payload is byte-identical for repeat firings
  except for `started_at` / `resolved_at`

## Dependencies

- Apache-2.0 Rust crates: `tokio`, `tokio-tungstenite` (unused at
  v0, listed because Mattermost slice will use it), `reqwest` for
  HTTP, `serde` + `serde_json` for sink payloads, `cue` (rust
  binding) or hand-rolled CUE parsing
- The `kaleidoscope/docker-compose.test.yml`-style Prom container
  digest will be added to `crates/beacon/tests/fixtures/`

## Effort estimate

≤ 1 day of crafter dispatch. The shape mirrors Aperture v0 slice 01
(load config → run service → emit telemetry) — well-trodden ground.

## Reference class

- Aperture v0 slice 01: ingest one OTLP message, log it
- Prism v0 slice 01: query Prometheus, render a chart

Both succeeded in ≤ 1 day. Beacon's walking skeleton is structurally
the same shape (load config, fetch data, emit).

## Pre-slice SPIKE (if any)

Spike a 30-line standalone binary that uses `prometheus-http-query`
to fetch one PromQL query against a real Prom container, and emits
the result. This validates the crate ergonomics before committing
to the slice 01 architecture.
