# `otlp-conformance-harness`

A Rust crate that performs **structural decode-level** validation of
byte sequences as OTLP protobuf messages. Phase-0 leaf dependency for
Kaleidoscope. Apache-2.0 licensed (SDK / protocol-library class per
the workspace's `LICENSING.md`).

## Validation depth

Validation is structural decode-level: the bytes must decode as the
expected `ExportFooServiceRequest` protobuf for the asserted signal. It
does NOT perform the OTLP semantic checks — no trace_id/span_id length
check, no timestamp validation, no attribute validation, no
semantic-convention enforcement. A structurally-valid but
semantically-bogus message is accepted.

`Framing::GrpcProtobuf` is an inert label at v0: it is echoed into
violations, not branched on. The caller strips the 5-byte gRPC length
prefix before invoking the harness; a body that still carries its
length prefix fails to decode.

## Status

Delivered and green. The crate's public API is locked (see
[`docs/product/architecture/adr-0001-public-api-surface-and-crate-layout.md`](../../docs/product/architecture/adr-0001-public-api-surface-and-crate-layout.md)).
The three `validate_*` functions are implemented and green; the
acceptance tests under `tests/slice_*.rs` lock their behaviour (empty
input, malformed protobuf, signal mismatch, and the accept paths for
logs, traces and metrics).

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

Apache-2.0. See [`LICENSE-APACHE-2.0`](../../LICENSE-APACHE-2.0) at the
repository root and [`LICENSING.md`](../../LICENSING.md) for the per-crate
table.
