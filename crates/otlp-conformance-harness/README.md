# `otlp-conformance-harness`

A CC0-1.0 Rust crate that validates byte sequences against the
OpenTelemetry OTLP wire specification. Phase-0 leaf dependency for
Kaleidoscope.

## Status

DISTILL wave complete. The crate's public API is locked (see
[`docs/product/architecture/adr-0001-public-api-surface-and-crate-layout.md`](../../docs/product/architecture/adr-0001-public-api-surface-and-crate-layout.md)).
Implementation is intentionally absent at this point — every
`validate_*` function returns `unimplemented!()`. The acceptance tests
under `tests/slice_*.rs` define the contract; the DELIVER wave's
`nw-software-crafter` agent replaces each `unimplemented!()` panic with
real production code, one slice at a time.

## Public API (locked, US-06 AC 5)

```rust
pub fn validate_logs(
    bytes: &[u8],
    framing: Framing,
) -> Result<opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest, OtlpViolation>;

pub fn validate_traces(
    bytes: &[u8],
    framing: Framing,
) -> Result<opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest, OtlpViolation>;

pub fn validate_metrics(
    bytes: &[u8],
    framing: Framing,
) -> Result<opentelemetry_proto::tonic::collector::metrics::v1::ExportMetricsServiceRequest, OtlpViolation>;
```

Plus `Framing`, `SignalType`, `OtlpViolation`, `Rule`, `WireTypeRule`,
`ByteOffset`, and the `OTLP_SPEC_VERSION` constant.

## Conformance test-vector corpus

`tests/vectors/{signal}/{verdict}/{vector}.{bin,expected.json}` per
[ADR-0004](../../docs/product/architecture/adr-0004-conformance-test-vector-layout.md).
Each `.bin` has a sibling `.expected.json` declaring the expected
verdict, the asserted signal/framing, the rule (for reject vectors), a
SHA-256 content hash, and the OTLP spec version under which the vector
was captured.

The corpus runner (`tests/slice_07_lock_the_contract.rs`) walks the
directory recursively, verifies each `.bin`'s hash before validating,
runs the appropriate `validate_*` function, asserts the verdict matches
the descriptor, and enumerates every `Rule` variant to confirm at least
one defending reject vector exists.

To regenerate the corpus from the upstream `opentelemetry-proto` types:

```sh
cargo run --example capture_corpus_vectors -p otlp-conformance-harness
```

The capture program is deterministic — the same upstream version
produces the same bytes — and idempotent against existing files.

## CI gates (per [ADR-0005](../../docs/product/architecture/adr-0005-ci-contract.md))

Every commit affecting `crates/otlp-conformance-harness/**` must pass:

1. `cargo test -p otlp-conformance-harness --all-targets --locked`
2. `cargo public-api --diff-git-checkouts main HEAD -p otlp-conformance-harness`
3. `cargo semver-checks check-release -p otlp-conformance-harness --baseline-rev main`
4. `cargo deny check`
5. `cargo mutants --package otlp-conformance-harness --check`

The CI workflow runner is owned by the DEVOPS wave; the contract above
is runner-agnostic.

## Licence

CC0-1.0. See `LICENSE` at the repository root.
