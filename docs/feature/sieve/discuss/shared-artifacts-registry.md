# Sieve v0 — shared artefacts registry

Every `${variable}` referenced in stories or scenarios is registered
here with source-of-truth, consumers, owner, integration risk, and CI
validation per the nWave DISCUSS mandate.

## High-risk artefacts

| Artefact | Source of truth | Displayed as | Consumers | Integration risk | Validation |
|---|---|---|---|---|---|
| `SIEVE_NON_ERROR_TRACE_RATE` | deployment manifest env var | a float in `[0.0, 1.0]`, default `0.1` | Sieve at process start; INFO summary repeats it; operator dashboards | HIGH — misconfiguration is silent unless surfaced in the summary | startup parse fails fast on out-of-range or non-numeric; `slice_03_non_error_rate` enforces the rate-band invariant |
| `status.code` | application-side OTel SDK | OpenTelemetry span field, ENUM { OK, UNSET, ERROR } | Sieve `is_error_bearing` check; downstream UIs | HIGH — SDK that doesn't set ERROR on failures loses error-bias retention | `slice_02_error_bias` asserts retention across rates; KPI 1 guardrails |
| `Sampler` trait | `crates/sieve/src/lib.rs` (DESIGN's call on exact shape) | Rust trait with `sample(...) -> Decision` | `HeadSampler` implementation; Aperture's pipeline integration | HIGH — public-API surface; ADR-equivalent lock | `cargo public-api -p sieve` (Gate 2); `cargo semver-checks` (Gate 3) |
| `Decision` enum | `crates/sieve/src/lib.rs` | `Decision::Keep` / `Decision::Drop` | every test assertion; pipeline router | HIGH — closed enum; non-exhaustive may be appropriate at DESIGN time | public-API and semver gates |

## Medium-risk artefacts

| Artefact | Source of truth | Displayed as | Consumers | Integration risk | Validation |
|---|---|---|---|---|---|
| `trace_id` | OpenTelemetry SDK | 128-bit identifier per OTel spec | Sieve `xxh3_64` hash; Aperture's batch correlator; downstream UIs | MEDIUM — must be deterministic across batches | `slice_04_trace_id_determinism` asserts; `xxh3_64` is deterministic by construction |
| `xxh3_64` hash function | `xxhash-rust` crate | the deterministic mapping `trace_id → [0.0, 1.0]` | rate-decision in `HeadSampler::sample` | MEDIUM — locking the choice means later perf or interop changes are breaking | locked in `wave-decisions.md > Q7` |

## Low-risk artefacts (namespacing, conventions)

| Artefact | Source of truth | Displayed as | Consumers | Integration risk | Validation |
|---|---|---|---|---|---|
| `target = "sieve"` tracing target | Sieve's observability module | the `target` field on tracing events | operator's tracing_subscriber; log aggregator filter | LOW — namespacing convention | `slice_06_observability` asserts events arrive with the right target |
| Per-decision DEBUG event vocabulary | Sieve's observability module | "kept (error-bearing)" / "kept (sampled, rate=0.10)" / "dropped (rate=0.10)" | operator on `RUST_LOG=sieve=debug` | LOW — operator-readable text | `slice_06_observability` asserts the vocabulary |
| Periodic INFO summary vocabulary | Sieve's observability module | "kept N traces (E error-bearing, S sampled at 0.10 rate), dropped M traces over the last 60s" | operator at default verbosity | LOW — operator-readable text; the format is the contract | `slice_06_observability` asserts the fields appear |

## CI invariants protecting these artefacts

| Invariant | Owner | Mechanism |
|---|---|---|
| `SIEVE_NON_ERROR_TRACE_RATE` is parsed safely or fails fast | Sieve crate | unit test on the parse function plus startup smoke test |
| Error-bearing traces always yield `Decision::Keep` | Sieve crate | `slice_02_error_bias` integration test, parameterised over rates |
| Configured rate honoured to within ±3% on a 10000-trace fixture | Sieve crate | `slice_03_non_error_rate` integration test |
| Same `trace_id` yields the same decision across calls | Sieve crate | `slice_04_trace_id_determinism` integration test |
| Logs and metrics passthrough unchanged | Sieve crate | `slice_05_logs_metrics_passthrough` integration test |
| Per-decision DEBUG events and periodic INFO summary present | Sieve crate | `slice_06_observability` integration test |
| Public surface stable | DEVOPS workflow | `cargo public-api -p sieve` Gate 2; `cargo semver-checks` Gate 3 |
| Apache supply chain | DEVOPS workflow | `cargo deny check` Gate 4 |
| 100% mutation kill rate per slice | DEVOPS workflow | `cargo mutants --package sieve --in-diff` Gate 5 |
| AGPL containment | DEVOPS workflow | the crate's `license = "AGPL-3.0-or-later"` plus `cargo deny check` |
