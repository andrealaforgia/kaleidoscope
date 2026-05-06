# Sieve v0 — technology choices and licence audit

## Runtime closure

| Crate | Pin | Licence | Role | ADR |
|-------|-----|---------|------|-----|
| `aperture` | `=0.1.0` (path dep) | AGPL-3.0-or-later | Sieve consumes the `OtlpSink`, `Probe`, `SinkRecord`, `SinkError`, `ProbeError` types from `aperture::ports` | ADR-0019 §2 |
| `async-trait` | caret `0.1` | MIT OR Apache-2.0 | Macro that lets `SamplingSink<S, N>` implement Aperture's `#[async_trait]` `OtlpSink` and `Probe` | ADR-0019 §3 |
| `opentelemetry-proto` | `=0.27.0` (workspace) | Apache-2.0 | Span / ExportTraceServiceRequest / ExportLogsServiceRequest / ExportMetricsServiceRequest types | ADR-0019 §8 |
| `prost` (transitive) | caret `0.13` (workspace) | Apache-2.0 | Required by `opentelemetry-proto`'s generated code | inherited |
| `thiserror` | caret `2` | MIT OR Apache-2.0 | `SieveConfigError` derive | ADR-0019 §7 |
| `tokio` | caret `1.40` features `macros, rt, sync, time` | MIT | Async runtime + `tokio::time::interval` for the summary task | ADR-0019 §4 |
| `tokio-util` | caret `0.7` | MIT | `CancellationToken` for cooperative timer-task cancellation | ADR-0019 §5 |
| `tonic` (transitive) | caret (workspace) | MIT | Required by `opentelemetry-proto`'s gen-tonic-messages feature | inherited |
| `tracing` | caret `0.1` | MIT | DEBUG per-decision events + INFO periodic summary | ADR-0019 §6 |
| `xxhash-rust` | `=0.8` feature `xxh3` | BSL-1.0 OR MIT | xxh3_64 hash for the trace_id-keyed sampling decision | ADR-0019 §1 |

## Dev-dependency closure (test-only; not in runtime)

| Crate | Pin | Licence | Role |
|-------|-----|---------|------|
| `tokio` | caret `1.40` features `full, test-util` | MIT | `tokio::time::pause()` / `advance()` for deterministic timer testing in slice 06 |
| `tracing-subscriber` | caret `0.3` features `fmt, env-filter, registry` | MIT | Captures `target="sieve"` events for the slice-06 assertion |

## Licence-policy compliance

The Sieve runtime closure consists of:

- **AGPL-3.0-or-later**: `sieve` (this crate) and `aperture` (path
  dep). Sieve is itself AGPL; symmetric. No compliance issue.
- **MIT or Apache-2.0** (and `MIT OR Apache-2.0` dual licences):
  `async-trait`, `opentelemetry-proto`, `prost`, `thiserror`, `tokio`,
  `tokio-util`, `tonic`, `tracing`. Permissive; allowed under the
  workspace's `cargo deny` policy.
- **BSL-1.0** (Boost Software Licence): `xxhash-rust`. Permissive,
  OSI-approved, dual-licensed alongside MIT. Adds one `allow` entry
  to `deny.toml`. ADR-0019 §1 documents the rationale.

No GPL-2.0-only, LGPL, SSPL, or proprietary licences in the runtime
closure. `cargo deny check` Gate 4 enforces the policy on every
commit.

## Why xxh3_64 over alternatives

ADR-0019 §1.1 documents the choice. Summary:

- **`xxh3_64` chosen**: aligns with the OTel collector's
  TailSamplingProcessor; 64 bits of entropy is sufficient for
  rate-comparison against a `[0.0, 1.0]` float; current-generation
  algorithm with better distribution than `xxh64`.
- `xxh3_128`: rejected — overkill for rate decisions.
- `xxh64`: rejected — previous-generation; weaker distribution.
- `SipHasher` (std-lib): rejected — slower; cryptographic quality
  not needed; OTel community has converged on xxh3.

## Why no `rand` crate

ADR-0018 §"`HeadSampler::sample` mechanism" and ADR-0019 §"Why no
rand". Summary: the deterministic `xxh3_64(trace_id)` mapping IS
the probability source. Slice 04's determinism requirement and
slice 03's distribution requirement are both satisfied by the
same mechanism. No separate RNG abstraction is needed.

## Why no `serde`

ADR-0019 §"Why no serde". Summary: the only configuration is two
env vars (`SIEVE_NON_ERROR_TRACE_RATE`, `SIEVE_SUMMARY_TICK_MS`),
both parsed via `f64::from_str` / `u64::from_str` from the
standard library. No structured config = no `serde` dependency.

## Operational override env vars

| Env var | Default | Type | Purpose | Documented |
|---------|---------|------|---------|------------|
| `SIEVE_NON_ERROR_TRACE_RATE` | `0.1` | float in `[0.0, 1.0]` | Non-error trace sample rate | DISCUSS Q5; ADR-0018 §"`HeadSampler::from_env`" |
| `SIEVE_SUMMARY_TICK_MS` | `60000` | positive integer (ms) | Summary tick interval; integration tests override to ≤100ms | ADR-0020 §"Tick interval" |

`SIEVE_SUMMARY_TICK_MS` is **not** part of the consumer-facing public
contract. It is a test-infrastructure and operational-override knob.
Operators in production should leave it at the default.
