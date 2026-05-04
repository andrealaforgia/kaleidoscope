<!-- markdownlint-disable MD024 -->

# User Stories — `otlp-conformance-harness-v0`

> Persona note: per the brief, the consumers of the harness are Kaleidoscope component authors, third-party OTel implementers, and Kaleidoscope CI. The harness is built by AI agents; the personas in these stories are *consumers* of the harness, not its builders. House style: British English, no human-effort estimation.

---

## System Constraints

These constraints apply to every story below and are not repeated in each:

1. **Library only**: the harness is a Rust crate at `crates/otlp-conformance-harness/`. It exposes a public API. It is not a service, has no network surface, opens no ports, and writes no telemetry.
2. **`opentelemetry-proto` is the source of OTLP types**: the harness depends on the upstream `opentelemetry-proto` crate (Apache-2.0). It does not redefine, wrap, or re-export OTLP message types under a harness-local name; it returns the upstream types unchanged on the accept path.
3. **Closed set of violation rules**: every rejection is one of a small, named set of rules. Adding a rule requires a minor version bump.
4. **No telemetry from the harness itself**: the harness does not write to stdout, stderr, or any logger. Diagnostic information is carried by the `OtlpViolation` value, not emitted as a side effect. (Per the project's no-telemetry-on-telemetry commitment in the roadmap.)
5. **No panics on invalid input**: every error path returns `Result::Err`. Panicking is reserved for true invariant violations of the harness's own internal logic, not for handling malformed or empty input.
6. **License**: CC0-1.0, like the rest of Kaleidoscope.
7. **Signal type is asserted, not inferred**: the caller chooses which `validate_*` function to invoke based on its own routing context. Signal-type inference is explicitly out of scope for v0.

---

## US-01 — Reject empty input with a structured violation

### Elevator Pitch

- **Before**: A Kaleidoscope component author writing the boundary check for Aperture has no way to validate even the most trivial form of malformed OTLP input — an empty body — without writing the validation themselves and inventing their own error type.
- **After**: They invoke `validate_logs(&[], Framing::HttpProtobuf)` and receive `Err(OtlpViolation { rule: EmptyInput, ... })` with the asserted signal echoed back, ready to be returned to the upstream client as a 400-class HTTP response or a gRPC `INVALID_ARGUMENT`. The test command that demonstrates this is `cargo test -p otlp-conformance-harness slice_01_empty_rejected`; the function call that demonstrates it at runtime is `otlp_conformance_harness::validate_logs(bytes, framing)`.
- **Decision enabled**: The component author decides whether to embed the harness in their boundary check. They see a working slice that exercises the public API end to end against the simplest possible failure case.

### Problem

A Kaleidoscope component receives bytes from an external source. Before doing anything else with those bytes, the component must reject obvious garbage. The simplest piece of garbage — and the one a misconfigured client is most likely to produce first — is a zero-length body. There is no Phase-0 component today that can perform this check without the component author writing the validation logic from scratch and inventing their own error shape. That fragmentation is exactly what the harness exists to prevent.

### Who

- **Aperture v0 author** (Phase 1, Kaleidoscope component): writes the OTLP gateway's boundary check; needs a one-call validation that already names empty input as a distinct violation rule.
- **Third-party observability engineer** (e.g. an SRE at a small company validating their own emitter against a Kaleidoscope cluster): wants to confirm that their emitter never sends empty bodies, and wants the same error shape as Kaleidoscope's own components.
- **Kaleidoscope CI**: runs this rule's vector on every commit to defend the rule's stability.

### Solution

A public function `validate_logs(bytes: &[u8], framing: Framing) -> Result<ExportLogsServiceRequest, OtlpViolation>` that, when handed `&[]`, returns `Err(OtlpViolation { rule: Rule::EmptyInput, locus: ByteOffset(0), expected: "non-empty OTLP body", observed: "0 bytes", signal_asserted: SignalType::Logs, framing_asserted: framing })`. The caller can pattern-match on the rule.

### Domain Examples

#### 1: Aperture rejects an empty POST body

A misconfigured Spark client emits an HTTP POST with `Content-Type: application/x-protobuf` to Aperture's `/v1/logs` endpoint, but with a zero-length body. Aperture calls `validate_logs(&[], Framing::HttpProtobuf)` and receives `Err(OtlpViolation { rule: Rule::EmptyInput, ... })`. Aperture returns HTTP 400 with the violation's diagnostic fields embedded in the response.

#### 2: Third-party CI run flags a regression in an emitter

A third-party engineer at a small company (`acme-observability`) wires the harness into their emitter's CI suite. After a refactor, their emitter starts producing zero-length bodies on the empty-batch path. Their CI run fails the next morning because `validate_logs(emitter_output, Framing::HttpProtobuf)` returns `Err(rule: Rule::EmptyInput)`. They identify the regression and revert before the commit reaches production.

#### 3: Kaleidoscope CI runs the slice-01 vector on every commit

The corpus contains `tests/vectors/logs/reject/empty.bin` (a 0-byte file) with a sibling `empty.expected.json` declaring `{ "rule": "EmptyInput", "signal": "logs" }`. The corpus runner asserts that `validate_logs(read_vector("empty.bin"), Framing::HttpProtobuf)` returns the expected violation. A future change that accidentally accepts empty input fails the CI build at this assertion, before the change can be merged.

### UAT Scenarios (BDD)

#### Scenario: Empty input is rejected with the EmptyInput rule

```
Given a zero-length byte sequence
And the caller asserts the signal type is logs
When the caller invokes `validate_logs(&[], Framing::HttpProtobuf)`
Then the call returns `Err(OtlpViolation { rule: Rule::EmptyInput, ... })`
And the violation's `signal_asserted` equals `SignalType::Logs`
And the violation's `framing_asserted` equals `Framing::HttpProtobuf`
```

#### Scenario: Empty input rejection is the same shape across all signal types

```
Given a zero-length byte sequence
When the caller invokes `validate_logs(&[], _)`
And the caller invokes `validate_traces(&[], _)`
And the caller invokes `validate_metrics(&[], _)`
Then all three calls return an Err carrying `Rule::EmptyInput`
And each violation's `signal_asserted` matches the function the caller invoked
```

#### Scenario: The harness produces no side effects when rejecting empty input

```
Given a zero-length byte sequence
And a process whose stdout, stderr, and logging facade are observed
When the caller invokes `validate_logs(&[], Framing::HttpProtobuf)`
Then the call returns the EmptyInput violation
And the process has not written to stdout
And the process has not written to stderr
And no log record has been emitted by any logging facade
```

### Acceptance Criteria

- [ ] `validate_logs(&[], _)` returns `Err(OtlpViolation { rule: Rule::EmptyInput, ... })`.
- [ ] The `Rule` enum contains the `EmptyInput` variant and is `pub`.
- [ ] The `OtlpViolation` carries the asserted signal and framing, the byte locus, and human-readable expected/observed strings.
- [ ] The harness writes nothing to stdout, stderr, or any logging facade when handling empty input (assertion observed across all three channels).
- [ ] The slice-01 corpus vector (`tests/vectors/logs/reject/empty.bin`, 0 bytes) passes the corpus runner with the expected verdict (corpus runner introduced in US-07; for US-01 the test is a hand-written Cargo test).
- [ ] `cargo test -p otlp-conformance-harness slice_01_empty_rejected` is green.

### Outcome KPIs

- **Who**: the harness, against its own reject-path corpus.
- **Does what**: produces the `EmptyInput` violation rule for every empty-body vector.
- **By how much**: 100% of empty-body vectors (one in v0; more may be added).
- **Measured by**: the corpus runner asserts `OtlpViolation::rule == Rule::EmptyInput` for each empty-body vector.
- **Baseline**: greenfield (n/a until the slice ships).

### Technical Notes

- Depends on the harness crate scaffolding (Cargo.toml, lib.rs, the `Framing` and `SignalType` enums, the `OtlpViolation` struct). All of those are introduced by this slice.
- No external dependencies beyond `opentelemetry-proto` (declared in `Cargo.toml` for use in later slices, not exercised in this slice).
- This is the walking skeleton. Every later slice presupposes the surface introduced here.

### Dependencies

None. This is the first slice.

---

## US-02 — Reject malformed protobuf with a structured violation

### Elevator Pitch

- **Before**: After US-01, the harness rejects empty input but accepts every non-empty byte sequence as if it were valid OTLP. A truncated or corrupted body would cause downstream callers to either crash on decode or invent ad-hoc decode-error handling.
- **After**: The caller invokes `validate_logs(corrupted_bytes, Framing::HttpProtobuf)` and receives `Err(OtlpViolation { rule: Rule::WireType(WireTypeRule::ProtobufDecode), locus: ByteOffset(N), ... })` with N pointing at the byte where decoding failed. The test command is `cargo test -p otlp-conformance-harness slice_02_malformed_protobuf_rejected`; the function call is unchanged from US-01.
- **Decision enabled**: The Aperture author decides that the harness's reject path is rich enough to power a good 400-class response. The third-party engineer decides whether their emitter's decode-error reporting is consistent with the harness's.

### Problem

`opentelemetry-proto` will refuse to decode malformed bytes, but the decode error it surfaces is a `prost::DecodeError` — a generic protobuf decode error that does not carry any OTLP-specific context. Every consumer would otherwise have to translate that error into something useful for its own users. Centralising the translation in the harness is the entire point of a conformance gate.

### Who

- **Aperture v0 author**: needs a 400-class response body that names the rule and the byte offset where decoding failed.
- **Third-party observability engineer**: writes an emitter test suite that asserts their emitter never produces malformed protobuf; needs the same error shape as Kaleidoscope's own components.
- **Kaleidoscope CI**: runs slices 02's reject-path vectors on every commit.

### Solution

Extend the harness so that, after the empty-input check, it attempts to decode the byte sequence using the `opentelemetry-proto` descriptor for the asserted signal. On `prost::DecodeError`, the harness translates the error into `OtlpViolation { rule: Rule::WireType(WireTypeRule::ProtobufDecode), locus: ByteOffset(N), expected: "valid protobuf wire bytes per opentelemetry-proto descriptor", observed: <prost diagnostic>, ... }`. The byte locus is best-effort; if `prost` does not provide an offset for a particular failure mode, the harness records `ByteOffset::Unknown` and notes the limitation in the diagnostic.

### Domain Examples

#### 1: Aperture rejects a truncated logs export request

A flaky network drops the last 200 bytes of a 1.2 KB OTLP logs body. Aperture's TCP reassembly hands the truncated buffer to `validate_logs(buffer, Framing::HttpProtobuf)`. The harness returns `Err(rule: ProtobufDecode, locus: ByteOffset(1004), observed: "unexpected EOF in length-delimited field")`. Aperture returns HTTP 400 with the diagnostic embedded; the upstream sender retries the full body.

#### 2: Third-party engineer detects a varint corruption bug

An engineer at `acme-observability` ships a refactor that introduces a bug: their custom serialiser writes varint tags in little-endian rather than the protobuf-required big-endian-ish format. Their CI run fails because `validate_traces(emitter_output, _)` returns `Err(rule: ProtobufDecode, locus: ByteOffset(7), observed: "invalid varint")`. The byte offset points them straight at the bug.

#### 3: Kaleidoscope CI runs the malformed-bytes vectors

The corpus contains three reject vectors under `tests/vectors/logs/reject/`: `truncated.bin` (a real export request truncated at byte 50), `bad_varint.bin` (a hand-crafted sequence with an invalid varint), and `bad_tag.bin` (a sequence with a tag pointing at an undefined field number for the OTLP logs message). All three pass the corpus runner with `rule: ProtobufDecode`.

### UAT Scenarios (BDD)

#### Scenario: A truncated OTLP body's byte locus points near the truncation boundary

```
Given a real OTLP logs export request truncated at byte 50
When the caller invokes `validate_logs(truncated_bytes, Framing::HttpProtobuf)`
Then the call returns `Err(OtlpViolation { rule: Rule::WireType(WireTypeRule::ProtobufDecode), ... })`
And the violation's locus is a `ByteOffset` whose value is between 40 and 60 inclusive
```

#### Scenario: A truncated OTLP body's `observed` field names a recognisable decode-error category

```
Given a real OTLP logs export request truncated at byte 50
When the caller invokes `validate_logs(truncated_bytes, Framing::HttpProtobuf)`
Then the call returns `Err(rule: WireType::ProtobufDecode)`
And the violation's `observed` field contains one of: "unexpected EOF", "wire type error", "missing length-delimited data"
```

#### Scenario: An invalid varint is rejected with ProtobufDecode

```
Given a byte sequence containing an invalid varint at byte 7
When the caller invokes `validate_logs(bad_varint, Framing::HttpProtobuf)`
Then the call returns `Err(rule: WireType::ProtobufDecode)`
And the violation's locus identifies a position within the input (best-effort byte offset)
And the violation's `observed` field contains one of: "unexpected EOF", "wire type error", "missing length-delimited data", "invalid varint"
```

#### Scenario: A protobuf with a bad tag is rejected with ProtobufDecode

```
Given a byte sequence whose first tag references an undefined field for ExportLogsServiceRequest
When the caller invokes `validate_logs(bad_tag, Framing::HttpProtobuf)`
Then the call returns `Err(rule: WireType::ProtobufDecode)`
```

#### Scenario: The decode failure does not leak the prost error type into the public API

```
Given any byte sequence that fails protobuf decoding
When the caller pattern-matches on the violation
Then the violation's rule is `WireType::ProtobufDecode`
And the caller does not need to know about `prost::DecodeError` to interpret the violation
```

### Acceptance Criteria

- [ ] `validate_logs(malformed_bytes, _)` returns `Err(rule: WireType::ProtobufDecode)`.
- [ ] The violation carries a best-effort byte locus; if the underlying decoder does not provide one, the violation records `ByteOffset::Unknown` and the diagnostic explains the limitation.
- [ ] The violation does not expose `prost::DecodeError` directly in its public type; the violation's public type is harness-owned.
- [ ] All three slice-02 reject vectors in the corpus produce the expected `ProtobufDecode` rule.
- [ ] `cargo test -p otlp-conformance-harness slice_02_malformed_protobuf_rejected` is green.

### Outcome KPIs

- **Who**: the harness, against its own reject-path corpus for malformed protobuf.
- **Does what**: produces the `ProtobufDecode` rule for every malformed-bytes vector.
- **By how much**: 100% of malformed-bytes vectors (three in v0).
- **Measured by**: corpus runner.
- **Baseline**: greenfield.

### Technical Notes

- Depends on US-01.
- The mapping from `prost::DecodeError` to a byte locus is best-effort; this is the slice's primary uncertainty. If the mapping is unsatisfactory, the harness records `ByteOffset::Unknown` and the v0 contract is honoured, with a follow-up story for richer locus reporting deferred to a later release.

### Dependencies

US-01.

---

## US-03 — Reject valid protobuf of the wrong signal type

### Elevator Pitch

- **Before**: After US-02, the harness can decode well-formed protobuf, but it cannot tell whether the decoded message is actually the signal the caller asserted. A correctly-encoded `ExportTraceServiceRequest` handed to `validate_logs` would round-trip cleanly and silently pollute the logs pipeline.
- **After**: The caller invokes `validate_logs(traces_bytes, Framing::HttpProtobuf)` and receives `Err(OtlpViolation { rule: Rule::WireType(WireTypeRule::SignalMismatch { observed: SignalType::Traces, asserted: SignalType::Logs }), ... })`. The test command is `cargo test -p otlp-conformance-harness slice_03_signal_mismatch_rejected`.
- **Decision enabled**: The Aperture author decides whether the harness's signal-mismatch detection is reliable enough to act as the only check between the gateway's HTTP routing layer and the storage engines downstream.

### Problem

The OTLP wire spec uses three distinct top-level message types — `ExportLogsServiceRequest`, `ExportTraceServiceRequest`, `ExportMetricsServiceRequest` — and routes them to three distinct endpoints. A misconfigured client that sends, say, traces to `/v1/logs` produces bytes that decode cleanly as a different signal but represent a routing error. Without an asserted-type check, the routing error becomes a data-corruption error in the storage engine, which is much harder to diagnose.

### Who

- **Aperture v0 author**: needs the harness to defend the signal contract that Aperture's own routing layer establishes.
- **Third-party engineer**: writes an emitter that mistakenly sends metrics to the logs endpoint after a misconfiguration; wants the harness to catch the mistake at validation time rather than after ingestion.
- **Kaleidoscope CI**: runs the signal-mismatch vector on every commit.

### Solution

After successful protobuf decoding, the harness verifies that the decoded message is the asserted signal type. The simplest implementation is to attempt decoding into the asserted type only; if that succeeds, the harness has a typed record. If it fails with a decode error, the harness *also* attempts decoding into the other two signal types. If one of the alternatives succeeds, the violation surfaced is `WireType::SignalMismatch { observed, asserted }`. If none of the alternatives succeed, the violation remains `WireType::ProtobufDecode` (US-02).

### Domain Examples

#### 1: Aperture catches a misrouted traces body on the /v1/logs endpoint

A misconfigured Spark client posts traces bytes to Aperture's `/v1/logs` endpoint. Aperture, having routed by URL, calls `validate_logs(bytes, Framing::HttpProtobuf)`. The harness returns `Err(rule: WireType::SignalMismatch { observed: SignalType::Traces, asserted: SignalType::Logs })`. Aperture returns HTTP 400 with a diagnostic suggesting `/v1/traces` instead.

#### 2: Third-party engineer catches a copy-paste error in their emitter

An engineer at `acme-observability` accidentally wires their metrics serialiser into the logs export path. Their emitter test suite calls `validate_logs(metrics_bytes, _)`. The harness returns `Err(rule: SignalMismatch { observed: Metrics, asserted: Logs })`. The error message names exactly which two paths got swapped.

#### 3: Kaleidoscope CI runs the signal-mismatch vector

The corpus contains `tests/vectors/logs/reject/traces_misrouted.bin` (a real `ExportTraceServiceRequest`) with `traces_misrouted.expected.json` declaring `{ "rule": "SignalMismatch", "asserted_signal": "logs", "observed_signal": "traces" }`. The corpus runner verifies the verdict.

### UAT Scenarios (BDD)

#### Scenario: Traces bytes handed to validate_logs produce SignalMismatch

```
Given a byte sequence that decodes cleanly as ExportTraceServiceRequest
When the caller invokes `validate_logs(bytes, Framing::HttpProtobuf)`
Then the call returns `Err(rule: WireType::SignalMismatch { observed: SignalType::Traces, asserted: SignalType::Logs })`
```

#### Scenario: Metrics bytes handed to validate_logs produce SignalMismatch

```
Given a byte sequence that decodes cleanly as ExportMetricsServiceRequest
When the caller invokes `validate_logs(bytes, Framing::HttpProtobuf)`
Then the call returns `Err(rule: WireType::SignalMismatch { observed: SignalType::Metrics, asserted: SignalType::Logs })`
```

#### Scenario: Bytes that decode as none of the three signals stay as ProtobufDecode

```
Given a byte sequence that fails to decode as any of the three OTLP signal types
When the caller invokes `validate_logs(bytes, Framing::HttpProtobuf)`
Then the call returns `Err(rule: WireType::ProtobufDecode)`
And the violation does NOT include a SignalMismatch — the failure is at decode level
```

### Acceptance Criteria

- [ ] When a byte sequence decodes as a different OTLP signal than the one asserted, the harness returns `Err(rule: WireType::SignalMismatch { observed, asserted })`.
- [ ] When a byte sequence decodes as the asserted signal, the harness returns `Ok(record)` immediately, and the returned record is the typed upstream value (not an intermediate state, surrogate, or harness-local wrapper). Verifiable by a Cargo unit test that pattern-matches on the return value.
- [ ] When a byte sequence decodes as none of the three signals, the harness returns `Err(rule: WireType::ProtobufDecode)`, not `SignalMismatch`.
- [ ] The slice-03 vector in the corpus produces the expected `SignalMismatch` verdict with the correct `observed` and `asserted` fields.
- [ ] `cargo test -p otlp-conformance-harness slice_03_signal_mismatch_rejected` is green.

### Outcome KPIs

- **Who**: the harness, against the signal-mismatch corpus vector.
- **Does what**: produces the `SignalMismatch` rule with the correct observed/asserted pair.
- **By how much**: 100% of signal-mismatch vectors (one in v0; more may be added).
- **Measured by**: corpus runner.
- **Baseline**: greenfield.

### Technical Notes

- Depends on US-02. The implementation re-uses the decode path established there.
- The alternative-decode strategy has a small cost (up to two extra decode attempts on the failure path). Acceptable for v0 because the failure path is, by definition, not the hot path; if profiling later shows this matters, US-03 can be revisited with a faster type-discriminator (e.g. inspecting the first tag byte).

### Dependencies

US-02.

---

## US-04 — Accept a minimally valid OTLP logs record

### Elevator Pitch

- **Before**: After US-03, the harness has three working reject paths but zero working accept paths. Until at least one signal type round-trips cleanly, the harness cannot prove the accept-path contract — that the typed record returned is the upstream `opentelemetry-proto` type, unchanged.
- **After**: The caller invokes `validate_logs(real_logs_bytes, Framing::HttpProtobuf)` and receives `Ok(record: opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest)`. The test command is `cargo test -p otlp-conformance-harness slice_04_logs_accepted`; the runtime call returns the typed record ready for downstream processing.
- **Decision enabled**: The Aperture author decides whether the harness's accept-path return type is what they want to feed downstream. The third-party engineer confirms their emitter's logs output is wire-conformant.

### Problem

A successful validation must hand back a value the caller can use directly — namely, the upstream `opentelemetry-proto` record. If the harness wrapped the record in a harness-local type, every consumer would have to convert from the harness-local type to the upstream type for any subsequent processing, defeating the entire purpose of building on top of `opentelemetry-proto`.

### Who

- **Aperture v0 author**: needs the typed record to feed into Sluice or to forward to a downstream OTel-compatible backend; cannot afford to convert types at the gateway hot path.
- **Third-party engineer**: wants their emitter validated against a real wire-format reader, not a hand-rolled mock.
- **Kaleidoscope CI**: runs the logs accept vector on every commit.

### Solution

When the byte sequence decodes cleanly as `ExportLogsServiceRequest`, the harness returns `Ok(record)` where `record` is the upstream type. The harness does no further validation in v0 (semconv-level required-attribute checks are explicitly out of scope; that work belongs to Codex in Phase 0 alongside the harness, not inside the harness).

### Domain Examples

#### 1: Aperture forwards accepted logs to an external Loki backend

Aperture receives a logs export request from Spark. It calls `validate_logs(bytes, Framing::HttpProtobuf)` and pattern-matches on `Ok(record)`. The record is then handed to Aperture's forwarding exporter, which writes it to the operator's existing Loki backend (per the Phase-1 architecture). Zero type conversions.

#### 2: Third-party engineer validates their custom emitter's first logs export

An engineer at `acme-observability` is writing a custom Rust SDK that emits OTLP logs. Their integration test calls `validate_logs(self.encode_batch(test_records), Framing::HttpProtobuf)` and asserts `result.is_ok()`. When their first attempt fails with `ProtobufDecode`, they fix the encoder; when it fails with `SignalMismatch`, they fix the routing; when it returns `Ok`, they have wire conformance.

#### 3: Kaleidoscope CI runs the logs accept vector

The corpus contains `tests/vectors/logs/accept/minimal.bin`, captured from the upstream OpenTelemetry Rust SDK emitting a single log record with the bare-minimum required attributes (resource, scope, log record body). The corpus runner asserts `validate_logs(...).is_ok()` and verifies the decoded record's structure matches what the upstream SDK encoded.

### UAT Scenarios (BDD)

#### Scenario: A minimal logs export request is accepted and returned typed

```
Given a byte sequence produced by the OpenTelemetry SDK as ExportLogsServiceRequest
When the caller invokes `validate_logs(bytes, Framing::HttpProtobuf)`
Then the call returns `Ok(record)`
And the record's type is `opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest`
And the record contains the resource, scope, and log records the SDK encoded
```

#### Scenario: The accepted record is directly usable by a downstream consumer expecting the upstream type

```
Given any byte sequence accepted by the harness
When the caller passes the returned record to a function whose parameter type is `opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest`
Then the call type-checks and runs without any explicit conversion
And the downstream function observes the same fields the SDK encoded
```

#### Scenario: The harness produces no side effects on the accept path

```
Given a valid OTLP logs export request
And a process whose stdout, stderr, and logging facade are observed
When the caller invokes `validate_logs(bytes, Framing::HttpProtobuf)`
Then the call returns Ok with the typed record
And the process has not written to stdout
And the process has not written to stderr
And no log record has been emitted by any logging facade
```

### Acceptance Criteria

- [ ] `validate_logs(valid_bytes, _)` returns `Ok(ExportLogsServiceRequest)`.
- [ ] The returned type's full path is exactly `opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest`, and the harness crate does not define or re-export a conflicting type under a harness-local name. Verified by a CI check on the public API; the choice of mechanism (`cargo expand`, `cargo doc --no-deps` grep, or `cargo public-api`) is a DESIGN-wave decision.
- [ ] The slice-04 corpus vector (`tests/vectors/logs/accept/minimal.bin`) is captured from a real OpenTelemetry SDK and is documented with the SDK and version that produced it.
- [ ] The harness writes nothing to stdout, stderr, or any logger on the accept path (assertion observed across stdout, stderr, and the logging facade).
- [ ] `cargo test -p otlp-conformance-harness slice_04_logs_accepted` is green.

### Outcome KPIs

- **Who**: the harness, against the logs accept vector.
- **Does what**: returns `Ok` with the upstream-typed `ExportLogsServiceRequest`.
- **By how much**: 100% — every accept-path vector returns Ok (zero false positives is the north star).
- **Measured by**: corpus runner asserts `Result::is_ok` for each accept vector.
- **Baseline**: greenfield.

### Technical Notes

- Depends on US-03. Reuses the decode path.
- Capturing the test vector is part of the slice's work: a small Rust program using the OpenTelemetry Rust SDK emits a logs export and writes the encoded bytes to disk; the bytes become `tests/vectors/logs/accept/minimal.bin`. The capture program lives outside the crate's compiled tests (it is a `dev-dependency` example) so that the `opentelemetry` SDK does not become a runtime dependency of the harness.

### Dependencies

US-03.

---

## US-05 — Accept a minimally valid OTLP traces record

### Elevator Pitch

- **Before**: After US-04, the harness has a working logs accept path but no working traces accept path. Aperture's traces routing in Phase 1 has nothing to validate against.
- **After**: The caller invokes `validate_traces(real_traces_bytes, Framing::HttpProtobuf)` and receives `Ok(record: opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest)`. The test command is `cargo test -p otlp-conformance-harness slice_05_traces_accepted`.
- **Decision enabled**: The Aperture author decides whether the traces accept-path symmetry holds: the same shape of public API for a different signal.

### Problem

The harness must cover all three OTLP stable signal types. Traces is the second-most-complex (after metrics) and is the right next slice because it tests whether the harness's signal-type abstraction generalises beyond logs without requiring re-architecting.

### Who

- **Aperture v0 author**: needs traces validation symmetric with logs validation.
- **Ray v0 author** (Phase 5, future Kaleidoscope component): will eventually depend on the harness for trace ingest; the contract introduced here is what Ray will consume.
- **Third-party engineer**: wants traces conformance verification.

### Solution

A second public function, `validate_traces(bytes: &[u8], framing: Framing) -> Result<ExportTraceServiceRequest, OtlpViolation>`, with the same shape and semantics as `validate_logs`. The function reuses the decode path and the violation rules; only the asserted signal type and the upstream type differ.

### Domain Examples

#### 1: Aperture validates a traces export request before forwarding

Aperture receives a traces export from Spark. It calls `validate_traces(bytes, Framing::HttpProtobuf)`. On Ok, the typed record is handed to Aperture's forwarding exporter (which sends to an external Tempo backend in Phase 1). On Err, Aperture returns HTTP 400 with the violation's diagnostic.

#### 2: Third-party engineer validates a traces emitter

An engineer at `acme-observability` ports their trace emission code to OTel and runs `validate_traces(emitter.encode_batch(spans), _)` in their CI. The first run fails with `SignalMismatch` because they wired the metrics serialiser by mistake; the next run fails with `ProtobufDecode` on a malformed span attribute; the third run returns Ok.

#### 3: Kaleidoscope CI runs the traces accept vector

The corpus contains `tests/vectors/traces/accept/minimal.bin`, captured from the OpenTelemetry Rust SDK emitting a single span with the bare-minimum required attributes (resource, scope, span). The corpus runner asserts `validate_traces(...).is_ok()`.

### UAT Scenarios (BDD)

#### Scenario: A minimal traces export request is accepted and returned typed

```
Given a byte sequence produced by the OpenTelemetry SDK as ExportTraceServiceRequest
When the caller invokes `validate_traces(bytes, Framing::HttpProtobuf)`
Then the call returns `Ok(record)`
And the record's type is `opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest`
And the record contains the resource, scope, and spans the SDK encoded
```

#### Scenario: validate_traces rejects logs bytes with SignalMismatch

```
Given a byte sequence that decodes cleanly as ExportLogsServiceRequest
When the caller invokes `validate_traces(bytes, Framing::HttpProtobuf)`
Then the call returns `Err(rule: WireType::SignalMismatch { observed: Logs, asserted: Traces })`
```

#### Scenario: validate_traces rejects empty input with EmptyInput

```
Given a zero-length byte sequence
When the caller invokes `validate_traces(&[], Framing::HttpProtobuf)`
Then the call returns `Err(rule: EmptyInput)`
And the violation's `signal_asserted` equals `SignalType::Traces`
```

### Acceptance Criteria

- [ ] `validate_traces(valid_bytes, _)` returns `Ok(ExportTraceServiceRequest)`.
- [ ] All three reject rules (`EmptyInput`, `ProtobufDecode`, `SignalMismatch`) produce the same shape for traces as for logs, with the asserted signal echoed back as `SignalType::Traces`.
- [ ] The slice-05 corpus vector (`tests/vectors/traces/accept/minimal.bin`) is captured from a real OpenTelemetry SDK.
- [ ] `cargo test -p otlp-conformance-harness slice_05_traces_accepted` is green.

### Outcome KPIs

- **Who**: the harness, against the traces accept vector.
- **Does what**: returns `Ok` with the upstream-typed `ExportTraceServiceRequest`.
- **By how much**: 100% — every accept-path traces vector returns Ok.
- **Measured by**: corpus runner.
- **Baseline**: greenfield.

### Technical Notes

- Depends on US-04. Reuses the decode path and the violation rule set.
- The capture program from US-04 is extended to emit traces in addition to logs.

### Dependencies

US-04.

---

## US-06 — Accept a minimally valid OTLP metrics record

### Elevator Pitch

- **Before**: After US-05, the harness covers logs and traces but not metrics. With one signal type missing, the harness cannot be a complete contract for Phase 0.
- **After**: The caller invokes `validate_metrics(real_metrics_bytes, Framing::HttpProtobuf)` and receives `Ok(record: opentelemetry_proto::tonic::collector::metrics::v1::ExportMetricsServiceRequest)`. The test command is `cargo test -p otlp-conformance-harness slice_06_metrics_accepted`.
- **Decision enabled**: The Aperture author decides that the harness's three-signal contract is complete enough to embed on the gateway's hot path. The third-party engineer's emitter is validated end-to-end across all three signal types.

### Problem

Metrics is the most complex of the three OTLP stable signal types. It includes point types (gauge, sum, histogram, exponential histogram, summary), aggregation temporality, and exemplars. If the harness's signal abstraction holds for metrics, it holds for everything currently in OTLP scope.

### Who

- **Aperture v0 author**: needs metrics validation to complete the gateway's three-signal coverage.
- **Pulse v1 author** (Phase 4, future Kaleidoscope component): will depend on the harness for metrics ingest.
- **Third-party engineer**: wants metrics conformance verification, including for less-common point types.

### Solution

A third public function, `validate_metrics(bytes: &[u8], framing: Framing) -> Result<ExportMetricsServiceRequest, OtlpViolation>`. Same shape as `validate_logs` and `validate_traces`. The corpus's accept vector for metrics covers a sum and a gauge in the minimal case; richer point-type coverage is deferred to follow-up vectors that may be added without bumping the crate version.

### Domain Examples

#### 1: Aperture validates a metrics export request before forwarding

Aperture receives metrics from Spark. It calls `validate_metrics(bytes, Framing::HttpProtobuf)`. On Ok, the typed record is forwarded to an external Mimir backend (Phase 1). On Err, the violation is returned to the client.

#### 2: Third-party engineer validates a Prometheus-remote-write-to-OTLP bridge

An engineer at `acme-observability` is writing a bridge from Prometheus remote-write to OTLP metrics. Their bridge's CI calls `validate_metrics(bridge.translate(prom_write_request), _)`. The first run fails with `ProtobufDecode` because they encoded a histogram bucket boundary as a varint instead of a double; the second run returns Ok.

#### 3: Kaleidoscope CI runs the metrics accept vector

The corpus contains `tests/vectors/metrics/accept/minimal.bin`, capturing one sum data point and one gauge data point. The corpus runner asserts `validate_metrics(...).is_ok()`.

### UAT Scenarios (BDD)

#### Scenario: A minimal metrics export request is accepted and returned typed

```
Given a byte sequence produced by the OpenTelemetry SDK as ExportMetricsServiceRequest
And the request contains a sum data point and a gauge data point
When the caller invokes `validate_metrics(bytes, Framing::HttpProtobuf)`
Then the call returns `Ok(record)`
And the record's type is `opentelemetry_proto::tonic::collector::metrics::v1::ExportMetricsServiceRequest`
And the record contains the resource, scope, and metric data the SDK encoded
```

#### Scenario: validate_metrics rejects traces bytes with SignalMismatch

```
Given a byte sequence that decodes cleanly as ExportTraceServiceRequest
When the caller invokes `validate_metrics(bytes, Framing::HttpProtobuf)`
Then the call returns `Err(rule: WireType::SignalMismatch { observed: Traces, asserted: Metrics })`
```

#### Scenario: validate_metrics covers the three reject rules symmetrically

```
Given the three reject test vectors (empty, malformed protobuf, signal mismatch) for metrics
When the caller invokes validate_metrics on each
Then each returns the appropriate violation rule
And the violation's `signal_asserted` equals `SignalType::Metrics` in all three cases
```

### Acceptance Criteria

- [ ] `validate_metrics(valid_bytes, _)` returns `Ok(ExportMetricsServiceRequest)`.
- [ ] All three reject rules produce the same shape for metrics as for logs and traces.
- [ ] The slice-06 corpus vector (`tests/vectors/metrics/accept/minimal.bin`) is captured from a real OpenTelemetry SDK and includes at least a sum and a gauge data point.
- [ ] `cargo test -p otlp-conformance-harness slice_06_metrics_accepted` is green.
- [ ] At end of slice-06 the public API exposes exactly three functions with the following signatures: `validate_logs(bytes: &[u8], framing: Framing) -> Result<ExportLogsServiceRequest, OtlpViolation>`; `validate_traces(bytes: &[u8], framing: Framing) -> Result<ExportTraceServiceRequest, OtlpViolation>`; `validate_metrics(bytes: &[u8], framing: Framing) -> Result<ExportMetricsServiceRequest, OtlpViolation>`. All three return the same `OtlpViolation` type on the error path.

### Outcome KPIs

- **Who**: the harness, across the three OTLP stable signal types.
- **Does what**: validates the signal type end-to-end.
- **By how much**: 3 of 3 — full coverage of the OTLP stable spec at the close of slice 06.
- **Measured by**: signal-coverage table in the crate README; slices 04, 05, 06 tests pass.
- **Baseline**: greenfield (was 0 of 3 at project start; 1 of 3 after slice 04; 2 of 3 after slice 05).

### Technical Notes

- Depends on US-05.
- Profiles is intentionally out of scope: the OpenTelemetry Profiles signal is still in development as of the spec version pinned for Phase 0. Adding profiles is a follow-up release once the upstream signal stabilises.

### Dependencies

US-05.

---

## US-07 — Lock the contract with a reference corpus and a CI gate

### Elevator Pitch

- **Before**: After US-06, the harness's public API works for every slice's hand-written test, but there is no machine-readable record of *what byte sequences are expected to produce what verdicts*. A future commit could silently change verdicts without anyone noticing until a downstream consumer broke at integration time.
- **After**: The caller (in this case Kaleidoscope CI) runs `cargo test -p otlp-conformance-harness corpus`. The corpus runner walks `tests/vectors/{logs,traces,metrics}/{accept,reject}/*.bin`, validates each against the harness, and verifies the verdict matches the sibling `.expected.json`. Any drift fails the build before the commit is merged. The CI workflow runs this on every commit touching the crate.
- **Decision enabled**: The Aperture author decides that the harness's contract is stable enough to depend on. The third-party engineer can use the corpus as canonical example input. The Kaleidoscope project as a whole has its first regression-defended contract.

### Problem

Slices 01–06 each ship a hand-written test for one rule or one accept path. The harness's contract is the union of those tests, but the union lives in disparate places. A single commit could change behaviour on one path without anyone noticing if the corresponding hand-written test wasn't run, or wasn't representative of every byte sequence consumers might send. The corpus is what makes the contract auditable: every named verdict is defended by a versioned, content-addressed byte sequence in the repository.

### Who

- **Aperture v0 author**: needs confidence that the harness's contract will not silently change.
- **Third-party engineer**: uses the corpus as example inputs *and* example outputs (the `.expected.json` siblings document what the harness will say about each vector).
- **Kaleidoscope CI**: runs the corpus on every commit affecting the crate; refuses to merge on any drift.

### Solution

Three deliverables in this slice:

1. **Corpus directory layout**: `crates/otlp-conformance-harness/tests/vectors/{logs,traces,metrics}/{accept,reject}/*.bin`, with a sibling `.expected.json` per vector declaring the expected verdict, the asserted signal and framing, the rule (for reject vectors), and a content hash of the `.bin` file.
2. **Corpus runner**: a single integration test (`tests/corpus.rs`) that walks the directory, verifies each `.bin` matches its declared content hash, runs the appropriate `validate_*` function, and asserts the verdict matches the descriptor.
3. **CI workflow contract**: the workflow itself is owned by the DEVOPS wave, but US-07 documents the requirement that `cargo test -p otlp-conformance-harness --all-targets` must run on every commit affecting the crate, and that any non-zero exit code blocks merge.

The `CorpusRegression` rule is not a runtime rule (the harness never produces it); it is a CI invariant: any vector whose verdict changes between commits without a corresponding rule-set diff is a regression.

### Domain Examples

#### 1: Kaleidoscope CI catches an accidental verdict flip

A maintainer refactors the decode path and inadvertently introduces a bug that causes one of the malformed-bytes vectors to flip from `ProtobufDecode` to `Ok`. The corpus runner asserts the expected verdict for every vector. The CI build fails; the maintainer reverts.

#### 2: Third-party engineer ships their own emitter using corpus vectors as fixtures

An engineer at `acme-observability` writes integration tests that feed the harness's accept-path vectors into their emitter's parser. They use `tests/vectors/logs/accept/minimal.bin` as a known-good input; they use `tests/vectors/logs/reject/empty.bin` as a known-bad input. The corpus is example data as well as test data.

#### 3: Aperture's CI run gates on the harness's corpus

A change to Aperture (a future Phase-1 deliverable) modifies the way Aperture buffers bytes before handing them to the harness. The Kaleidoscope CI workflow runs the harness's corpus as part of Aperture's build. The corpus passes; Aperture's change is allowed to merge. (A failure would have caught a regression in the buffering layer that violated the harness's contract.)

### UAT Scenarios (BDD)

#### Scenario: Every accept-path vector produces Ok

```
Given the corpus directory at `tests/vectors/`
When the corpus runner walks every `.bin` under `*/accept/`
Then for each vector, the appropriate `validate_*` function returns `Ok`
And the corpus runner asserts no false-positive rejections
```

#### Scenario: Every reject-path vector produces the expected rule

```
Given the corpus directory at `tests/vectors/`
When the corpus runner walks every `.bin` under `*/reject/`
Then for each vector, the appropriate `validate_*` function returns `Err`
And the violation's `rule` matches the `rule` field of the sibling `.expected.json`
```

#### Scenario: A mutated vector fails the corpus check before validation runs

```
Given a corpus vector whose `.bin` content has been edited
And whose `.expected.json` has not been updated to match the new content hash
When the corpus runner inspects the vector
Then the runner refuses to validate the vector
And the runner reports the hash mismatch as a corpus integrity error
```

#### Scenario: A newly added rule must be defended by at least one reject vector

```
Given the harness's `Rule` enum has a new variant added
When the corpus runner enumerates the variants
Then it verifies each variant has at least one reject vector targeting it
And the runner fails the build if any variant has zero defending vectors
```

### Acceptance Criteria

- [ ] The corpus directory `tests/vectors/{logs,traces,metrics}/{accept,reject}/*.bin` exists and contains, at minimum: 3 accept vectors (one per signal), 3 empty-input reject vectors (one per signal), 3 malformed-protobuf reject vectors (at least one per signal) and 3 signal-mismatch reject vectors (one per signal targeting each of the other two signals as the misrouted body — at least three vectors total).
- [ ] Each `.bin` has a sibling `.expected.json` with: `asserted_signal`, `asserted_framing`, `expected_verdict` (`Ok` or rule name), `content_hash` (hex SHA-256 of the `.bin`), `source` (a free-text field describing where the vector came from, e.g. "OpenTelemetry Rust SDK 0.27, captured 2026-05-03").
- [ ] The corpus runner verifies the content hash before running the harness against any vector.
- [ ] The corpus runner enumerates all variants of `Rule` and fails the build if any variant has zero defending reject vectors.
- [ ] `cargo test -p otlp-conformance-harness corpus` is green.
- [ ] The crate's README documents the corpus layout and the contract that consumers can rely on its stability.

### Outcome KPIs

- **Who**: the harness, against its own corpus.
- **Does what**: defends every accept path with at least one accept vector and every reject rule with at least one reject vector; refuses to validate any vector whose content has drifted from its declared hash.
- **By how much**: 100% of accept paths and 100% of reject rules defended.
- **Measured by**: the corpus runner.
- **Baseline**: greenfield (no corpus exists before this slice).

### Technical Notes

- Depends on US-01 through US-06.
- The corpus runner is a single Rust integration test file; no new dependencies beyond what the rest of the crate already pulls in.
- **Hash algorithm and storage format**: SHA-256, hex-encoded, stored in the sibling `.expected.json` alongside each `.bin` vector under the `content_hash` field. The hash is computed at vector creation time (when the `.bin` is captured or hand-crafted) and re-verified by the corpus runner before every validation run, so any mutation of the `.bin` is detected before the harness is invoked against it.
- The CI workflow is owned by DEVOPS; this story names the contract (`cargo test --all-targets` must pass before merge) without prescribing the workflow runner (GitHub Actions vs other) — that choice belongs to DEVOPS.

### Dependencies

US-01, US-02, US-03, US-04, US-05, US-06 (all of them).
