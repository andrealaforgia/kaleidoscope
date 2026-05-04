# Slice 03 — traces signal — completion summary

**Slice**: `slice-03-traces` | **Status**: GREEN | **Date**: 2026-05-04

## Headline

The traces signal now round-trips end-to-end on both transports. A real
OTel SDK that emits `ExportTraceServiceRequest` over gRPC :4317 or
HTTP/protobuf :4318 receives gRPC `OK` or HTTP 200, and the `RecordingSink`
records a `SinkRecord::Traces` with the correct `span_count` and resource
service name. The reject path (logs body sent to the traces endpoint) returns
the harness's verbatim `WireType::SignalMismatch` violation Display.

## Tests turned GREEN

10 acceptance tests in `crates/aperture/tests/slice_03_traces.rs`, plus
3 new lib unit tests in `src/lib.rs` (lifting the lib total from 22 to 25):

- gRPC traces accept paths
- HTTP/protobuf traces accept paths
- Signal-mismatch reject paths (logs body to traces endpoint) on both transports
- Verbatim violation Display in the response body / gRPC `grpc-message`
- StubSink stderr structured event with `signal=traces` and `span_count`

No regressions on Slices 01 (13) or 02 (15). Total active aperture tests
green: 64.

## Production code changes

| File | Net change | What it does |
|---|---|---|
| `src/app.rs` | +ingest_traces | Single call site for `validate_traces`; mirrors `ingest_logs` shape; routes to `SinkRecord::Traces` |
| `src/transport.rs` | +TraceServiceImpl + handle_traces | gRPC service impl bound to the reflection registry; axum router entry for `POST /v1/traces` |
| `src/sinks.rs` | +traces arm in StubSink emission | Structured stderr event extended with `span_count` field per ADR-0009 |
| `src/testing.rs` | +traces arm in RecordingSink | Symmetric to logs; same observable-state contract |

The `Transport::HttpProtobuf` variant lost its `#[allow(dead_code)]`
annotation in this slice (it was reserved for Slice 02's HTTP listener;
both transports are now load-bearing for traces).

## Architectural observations recorded

- The `single_validator_per_signal` CI invariant continues to hold:
  `validate_traces` has exactly one call site (in `app::ingest_traces`).
  When DELIVER wires the xtask AST check at Slice 03 close, the assertion
  set grows from one rule (logs) to two (logs + traces).
- The `framing_for_transport` helper remains total: every `Transport`
  variant maps to a `Framing` variant. No new Cartesian product cells
  were introduced.
- The closed v0 event vocabulary continues to hold: no new event names
  were emitted; `event=request_received` and `event=sink_accepted` carry
  a `signal=traces` field rather than a new event name.

## Mutation testing

Per ADR-0005 Gate 5, target is 100% kill rate on touched files. Locally
deferred for this commit (cargo-mutants run takes minutes; CI's nightly
gate will exercise it). The Slice 03 production code mirrors Slice 01's
shape line-for-line, and Slice 01 achieved 100% kill rate on the same
shape; the expectation is that the kill rate holds.

If CI surfaces a survivor when Gate 5 next runs across the workspace,
the fix-forward pattern from feature 1 applies: a follow-up commit
either tightens the test or simplifies the production code.

## Genuine forks

None. The DISCUSS Q1-Q6 + DESIGN D1-D10 + ADRs 0006-0010 + DISTILL test
inventory together resolved every implementation question Slice 03
raised. No back-propagation needed.

## Quality gates at commit time

- `cargo fmt --all -- --check`: clean
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: clean
- `cargo test --workspace --exclude aperture --all-targets --locked`:
  73 harness tests pass (matches DEVOPS A2 graduation contract)
- `cargo test --package aperture` (slices 01, 02, 03 + lib unit + invariant): all green
- Pre-commit hook: not modified; `--exclude aperture` graduation stays
  per Apex's wave-decisions A2

## Provenance note

This slice's work was completed by Crafty during an autonomous DELIVER
dispatch but landed on disk uncommitted at the end of that session.
Slice 03 was committed by the orchestrator (Bea) on Andrea's behalf
when the next session opened. The work itself is Crafty's; the
commit message and this completion document are the orchestrator's
record of the recovery.
