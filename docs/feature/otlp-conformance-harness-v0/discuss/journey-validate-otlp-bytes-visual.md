# Journey — Validate an OTLP byte sequence

> **Feature**: `otlp-conformance-harness-v0`
> **Persona**: Component author of a Kaleidoscope service (e.g. Aperture, Codex, any storage engine) who has just received bytes off the wire and must decide whether they conform to OTLP before processing them. Also: a third-party observability engineer running the harness against their own OTLP-emitting code; and Kaleidoscope's CI suite running the harness as a regression gate.
> **Wave**: DISCUSS (Luna) — 2026-05-03

---

## Why this journey is narrow

The brief is uncontroversial. A consumer holds a `&[u8]` and one piece of metadata (the asserted signal type: logs, traces, metrics; OTLP/gRPC framing or OTLP/HTTP/protobuf framing). They call the harness. The harness either returns a typed, well-formed OTLP record they can process, or it returns a structured violation that names which rule was broken and where in the byte sequence the failure occurred. There is no UI; the "view" is the return value of a Rust function and the output of `cargo test`.

The journey therefore has exactly two paths — accept and reject — each repeated across the three signal types currently in scope (logs, traces, metrics). The interesting design surface is in the *quality* of the rejection: the violation must carry enough information that the caller (Aperture rejecting a bad client request, or a third-party engineer debugging their emitter, or a CI run flagging a regression) can act without reading the OTLP specification themselves.

---

## Happy-path flow

```
+---------------+       +---------------+       +-------------------+       +-------------------+
| Component     | bytes | Conformance   |  ok   | Typed OTLP record | hand- | Component         |
| holds &[u8]   |------>| harness:      |------>| (LogsData /       | off   | continues with    |
| + framing     |       | validate_*    |       |  TracesData /     |------>| safe, typed input |
| assertion     |       |               |       |  MetricsData)     |       |                   |
+---------------+       +---------------+       +-------------------+       +-------------------+
       Feels:                  Feels:                   Feels:                       Feels:
       cautious                briefly tense            confident                    relieved
       (untrusted bytes)       (waiting on verdict)     (typed contract              (the contract held;
                                                        guarantees structure)        keep moving)
```

## Error-path flow

```
+---------------+       +---------------+       +---------------------------+       +-------------------+
| Component     | bytes | Conformance   |  err  | OtlpViolation:            | hand- | Component rejects |
| holds &[u8]   |------>| harness:      |------>| - rule (named)            | off   | upstream input    |
| + framing     |       | validate_*    |       | - locus (byte offset /    |------>| with the violation|
| assertion     |       |               |       |   field path)             |       | as evidence       |
+---------------+       +---------------+       | - expected vs observed    |       |                   |
                                                | - signal asserted         |       |                   |
                                                +---------------------------+       |                   |
       Feels:                  Feels:                   Feels:                       Feels:
       cautious                briefly tense            informed                    decisive
       (untrusted bytes)       (waiting on verdict)     (the violation              (no ambiguity about
                                                        names what is wrong)        what to tell the client)
```

There is no recovery within the harness. The caller decides: reject upstream, log, retry, escalate. The harness's contract ends at the violation.

---

## Emotional arc — short and even

| Point in journey   | Target emotional state     | Lever                                                                                  |
|--------------------|----------------------------|-----------------------------------------------------------------------------------------|
| Bytes arrive       | Cautious (default for any external input) | Caller has not yet trusted the bytes; the harness exists *because* trust is not yet earned. |
| Validation runs    | Briefly tense              | The function is synchronous and fast; tension is bounded by the call duration.          |
| Outcome surfaces   | Confident (Ok) **or** informed (Err) | Either path produces a structured value; neither path produces ambiguity.               |
| Caller acts        | Decisive                   | The caller knows exactly what to do next: process the typed record, or reject with the named violation. |

The arc is deliberately flat. There is no peak tension because the work is bounded: a single function call against a finite byte sequence. The hierarchy of needs (Walter) is satisfied entirely at the **functional** and **reliable** levels — the harness must work, and must work consistently. There is no usability or pleasurable layer to design for: the consumer is another component or a CI run, not a person navigating an interface.

---

## What the consumer sees — concrete output

The "screen" of this journey is the structured output of a Rust function call or a `cargo test` invocation. There is no terminal UI to mock. The two shapes the consumer sees are:

### 1. On Ok — a typed record

```rust
let record: opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest =
    otlp_conformance_harness::validate_logs(bytes, Framing::HttpProtobuf)?;
//      ^^^ no further parsing required; the type guarantees the wire shape held.
```

The caller gets the same type the upstream `opentelemetry-proto` crate exposes. The harness contributes the *guarantee* that the bytes round-tripped through the spec, not a new type to learn.

### 2. On Err — a structured violation

```text
OtlpViolation {
    rule: WireType::ProtobufDecode,
    locus: ByteOffset(42),
    expected: "valid protobuf wire bytes per opentelemetry-proto descriptor",
    observed: "invalid varint at byte 42 (tag mismatch)",
    signal_asserted: SignalType::Logs,
    framing_asserted: Framing::HttpProtobuf,
}
```

Each violation is one of a small, closed set of named rules (see `journey-validate-otlp-bytes.yaml`). Each rule has a stable identifier the caller can match on without parsing strings.

---

## Failure modes the harness must surface

These map directly to the slice list in `story-map.md`. Each is a named rule.

| Rule identifier              | What it catches                                                                  | Slice that introduces it |
|------------------------------|-----------------------------------------------------------------------------------|--------------------------|
| `EmptyInput`                 | Zero-length byte sequence.                                                        | Slice 01                 |
| `WireType::ProtobufDecode`   | Bytes are not valid protobuf at all (truncation, malformed varints, bad tags).    | Slice 02                 |
| `WireType::SignalMismatch`   | Bytes decode as a valid protobuf message but of the wrong OTLP signal type.       | Slice 03                 |
| (none — accept)              | Bytes decode and validate as the asserted signal.                                 | Slice 04 (logs), 05 (traces), 06 (metrics) |
| `CorpusRegression`           | Any reference vector in the checked-in corpus changes verdict between commits.    | Slice 07                 |

---

## Shared artefacts

This is a library, not a multi-step pipeline, but several values are still shared across the harness's surface and its consumers. Tracked formally in `shared-artifacts-registry.md`. Summary:

| Artefact                       | Source of truth                                                  | Consumers                                                        |
|--------------------------------|-------------------------------------------------------------------|-------------------------------------------------------------------|
| OTLP wire format               | `opentelemetry-proto` crate (Apache-2.0)                          | Harness, Aperture, Spark, every storage engine                    |
| Pinned semconv version         | Codex (Phase 0); for the harness, a constant in `Cargo.toml`      | Harness, Codex, Spark, Aperture                                   |
| Test-vector corpus             | `crates/otlp-conformance-harness/tests/vectors/` (Slice 07)       | Harness's own test suite, third-party engineers, Kaleidoscope CI  |
| Violation rule identifiers     | `OtlpViolation::rule` enum in the harness                         | Every caller that pattern-matches on violations                   |

---

## Integration checkpoints

| Checkpoint                                            | What is validated                                                                                       |
|--------------------------------------------------------|----------------------------------------------------------------------------------------------------------|
| Harness depends only on `opentelemetry-proto`         | No re-implementation of OTLP types; the protobuf descriptor is the single source of truth.              |
| Every accepted record returns the upstream type       | Callers never have to convert from a harness-local type to the upstream type.                           |
| Every rejection cites a named rule                    | Callers can pattern-match on `OtlpViolation::rule` without inspecting message strings.                  |
| Corpus is content-addressed and versioned             | A reference vector cannot be silently mutated; CI fails on any vector hash change without a commit.     |
| CI runs the corpus on every commit touching the crate | Conformance regressions surface within one commit, not at integration time with downstream consumers.   |

---

## What this journey is not

- **Not a service.** The harness is a Rust crate with a public API. There is no daemon, no port, no socket.
- **Not a generator.** The harness validates byte sequences but does not produce them. (Generating reference vectors is the job of the corpus build script in Slice 07, not the harness's runtime API.)
- **Not a parser the consumer can use as a primary parser.** Consumers import `opentelemetry-proto` for parsing. The harness is a *validation gate*: it parses, applies wire-spec rules, and either hands back the parsed record or a named violation. It deliberately does not introduce a competing OTLP type system.
- **Not a fuzz target.** Fuzz harnesses are valuable but downstream of v0. v0 is the contract; fuzzing is a hardening exercise on the contract once it stabilises.

---

## Changelog

- 2026-05-03 — Initial journey for `otlp-conformance-harness-v0`. Single-author, no priors.
