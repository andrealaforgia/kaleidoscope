# Beacon v0 — Technology Choices

## Runtime

- **Language**: Rust 2024 edition. MSRV 1.78 (current workspace floor).
- **Runtime**: Tokio (MIT) for async. Same choice as Aperture and Sieve.
- **HTTP client**: `reqwest` (MIT/Apache-2.0). Same as Prism's
  `queryRange`. Shared shape via ADR-0033.

## Configuration / Rule Schema

- **CUE**: the schema language. Substrate decision per the
  architecture doc.
- **CUE parser**: TBD (slice-02 SPIKE outcome). Candidates:
  `cue-ast-rs` (Apache-2.0, hypothetical) or hand-written subset
  parser. Documented in ADR-0034.

## Sink Adapters

- **Webhook**: `reqwest` POST with `serde_json`.
- **SMTP**: `lettre` (MIT/Apache-2.0).
- **Mattermost**: `reqwest` POST with markdown payload (no
  Mattermost-specific crate needed; webhook semantics).
- **Zulip**: `reqwest` POST with Zulip API JSON.
- **OnCall (Grafana)**: `reqwest` POST with OnCall webhook JSON.

## Telemetry

- **`tracing`** (MIT) for structured logging.
- **`opentelemetry-otlp`** (Apache-2.0) for OTLP export.
- **`opentelemetry_sdk`** (Apache-2.0) for spans and metrics.

## Testing

- **`cargo test`** with the standard harness.
- **`mockito`** (MIT) for HTTP mocking.
- **`tokio::time::pause`** for virtual-clock tests of the scheduler.
- **`cargo mutants`** for mutation testing (Gate 5).
- **`cargo public-api`** (Gate 2) and **`cargo semver-checks`**
  (Gate 3) for public surface stability.

## CI

- **GitHub Actions** mirroring the existing workflow.
- New gate: **Gate 5 (beacon)** — mutation testing scoped to
  `crates/beacon`.

## Dependencies not chosen (and why)

| Dep | Why not |
|---|---|
| Prometheus Alertmanager | Excellent project, Apache-2.0, but its receiver protocol is rigidly notification-routing-shaped and Kaleidoscope needs CUE-defined routing. Re-implementing in Rust is cheaper than maintaining a fork. (Architecture doc §C.12.) |
| Sloth | YAML-shaped; Beacon's catalogue language is CUE. Static-file-shaped; Beacon needs in-memory rules. (ADR-0036.) |
| Grafana OnCall (embedded) | Beacon emits TO OnCall, does not embed it. The on-call UX is its own product. (Architecture doc §C.12.) |
| Prometheus crate `prometheus-http-query` | Possible, evaluated at slice 01 SPIKE. If it does not fit the evaluator's `fetch_fn` shape cleanly, we use `reqwest` directly + a small `serde` deserialiser for the response shape. |

## Workspace integration

Beacon adds two new crates to the existing Cargo workspace:

- `crates/beacon` (library, AGPL-3.0)
- `crates/beacon-server` (binary, AGPL-3.0)

Workspace `Cargo.toml` gains entries; the existing harness, aperture,
spark, sieve, codex crates are unaffected.
