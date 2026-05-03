# Story Map — `otlp-conformance-harness-v0`

> **Persona**: Component author of a Kaleidoscope service (Aperture, Codex, Spark, every storage engine), a third-party observability engineer running the harness against their own emitter, or Kaleidoscope's CI suite. All three consume the harness via Cargo dependency.
>
> **Goal**: Decide, for a given byte sequence and asserted OTLP signal/framing, whether the bytes conform to the OTLP wire specification — and if they do, hand back the typed `opentelemetry-proto` record; if not, hand back a structured violation naming the rule that was broken.
>
> **Walking-skeleton position**: Per the brief, this feature **is** the first concrete slice of Kaleidoscope's overall walking skeleton. The roadmap places the OTLP conformance harness in Phase 0 alongside Codex v0 and Spark v0; the harness is the contract every later component hangs off. There is therefore no walking skeleton wrapped around the harness — the harness's own first slice (Slice 01) is the walking skeleton for the whole project.

---

## Backbone

The user activity is single: **validate a byte sequence against the OTLP wire specification**. Decomposing that activity into the chronological steps a consumer takes:

| Activity 1 — Reject obvious garbage | Activity 2 — Reject malformed protobuf | Activity 3 — Reject mis-typed protobuf | Activity 4 — Accept conforming bytes | Activity 5 — Lock the contract |
|---|---|---|---|---|
| Slice 01: empty input | Slice 02: malformed protobuf | Slice 03: wrong signal type | Slice 04: logs accepted | Slice 07: corpus + CI gate |
|  |  |  | Slice 05: traces accepted |  |
|  |  |  | Slice 06: metrics accepted |  |

Each cell is a thin vertical slice: one slice = one Cargo test = one named violation rule (or one accept path) = one learning hypothesis.

---

## Slice catalogue (elephant carpaccio)

Each slice ships a *single* test passing against a real byte sequence (production-shaped, produced by the OpenTelemetry reference SDK or hand-crafted protobuf), exercises a *single* code path, and validates a *single* learning hypothesis. Slices ship in order; each presupposes the previous.

### Slice 01 — Empty input is rejected with a structured violation

- **Hypothesis**: A zero-length byte sequence is the simplest invariant we can name and the cheapest violation rule to define. If we cannot ship this, we cannot ship anything.
- **Public surface introduced**: `validate_logs(&[u8], Framing) -> Result<ExportLogsServiceRequest, OtlpViolation>`. (The other two `validate_*` functions appear in slices 05 and 06.)
- **Rule introduced**: `EmptyInput`.
- **Test**: `cargo test -p otlp-conformance-harness slice_01_empty_rejected`.
- **Wall-clock**: under a day. Conceptual difficulty: low. No integration surface beyond declaring the crate.
- **Carpaccio taste tests**:
  - One Cargo test? Yes.
  - Real `opentelemetry-proto` byte sequence? The byte sequence is `&[]`; the realism is that this is the first thing a misconfigured client sends.
  - Structured error or success type? Yes — `OtlpViolation { rule: EmptyInput, ... }`.
  - Named learning hypothesis? Yes — see above.

### Slice 02 — Malformed protobuf is rejected with a structured violation

- **Hypothesis**: We can lean on `prost`'s decoder (used by `opentelemetry-proto`) to detect malformed wire bytes and translate its decode error into our `WireType::ProtobufDecode` rule with a useful byte offset. If this hypothesis fails — if `prost` does not give us enough locus information — we will need to write our own protobuf-level error mapping, which would expand the scope of v0.
- **Rule introduced**: `WireType::ProtobufDecode`.
- **Test inputs**: A truncated OTLP logs export request, a sequence with an invalid varint, a sequence with a tag pointing at an undefined field.
- **Test**: `cargo test -p otlp-conformance-harness slice_02_malformed_protobuf_rejected`.
- **Wall-clock**: under a day. Conceptual difficulty: low–medium (depends on how `prost` surfaces errors).
- **Carpaccio taste tests**: All four pass.

### Slice 03 — Valid protobuf of the wrong signal type is rejected

- **Hypothesis**: A byte sequence that decodes cleanly as `ExportTraceServiceRequest` but is handed to `validate_logs` should be rejected with `WireType::SignalMismatch`. The decoder does not detect this — we have to add the asserted-type check ourselves. Validates that the asserted-signal contract is enforceable.
- **Rule introduced**: `WireType::SignalMismatch`.
- **Test input**: A real `ExportTraceServiceRequest` byte sequence, passed to `validate_logs`.
- **Test**: `cargo test -p otlp-conformance-harness slice_03_signal_mismatch_rejected`.
- **Wall-clock**: a couple of hours; the slice piggybacks on slice 02's decode path.
- **Carpaccio taste tests**: All four pass.

### Slice 04 — A minimally valid OTLP logs record is accepted and returned typed

- **Hypothesis**: The harness can accept a real OTLP logs export request — produced by the upstream OpenTelemetry SDK — and return the upstream `ExportLogsServiceRequest` type unchanged. This is the first happy path; it proves the harness's accept-path contract.
- **Test input**: A logs export request encoded by the OpenTelemetry Rust SDK or the Go SDK and captured to a `.bin` file under `tests/vectors/logs/minimal.bin`.
- **Test**: `cargo test -p otlp-conformance-harness slice_04_logs_accepted`.
- **Wall-clock**: under a day. Conceptual difficulty: low; the work is the test fixture, not the code.
- **Carpaccio taste tests**: All four pass.

### Slice 05 — A minimally valid OTLP traces record is accepted

- **Hypothesis**: The accept path generalises from one signal to another with only a type-parameterisation change. Validates that the contract is symmetrical across signals.
- **Public surface introduced**: `validate_traces(&[u8], Framing) -> Result<ExportTraceServiceRequest, OtlpViolation>`.
- **Test**: `cargo test -p otlp-conformance-harness slice_05_traces_accepted`.
- **Wall-clock**: a half-day.
- **Carpaccio taste tests**: All four pass.

### Slice 06 — A minimally valid OTLP metrics record is accepted

- **Hypothesis**: The accept path generalises a third time. By this point the harness covers all three signal types currently in the OTLP stable spec (logs, traces, metrics; profiles are still in development per OTel and explicitly out of v0).
- **Public surface introduced**: `validate_metrics(&[u8], Framing) -> Result<ExportMetricsServiceRequest, OtlpViolation>`.
- **Test**: `cargo test -p otlp-conformance-harness slice_06_metrics_accepted`.
- **Wall-clock**: a half-day.
- **Carpaccio taste tests**: All four pass.

### Slice 07 — Reference test-vector corpus checked in and exercised by CI

- **Hypothesis**: Each violation rule and each accept path must be defended by at least one reference test vector that lives in the repository, is content-addressed, and is exercised by every CI run. Validates that future regressions surface within a single commit, not at integration time. This is the slice that makes the contract stable enough for Aperture, Codex, and the storage engines to depend on.
- **Rule introduced**: `CorpusRegression` (a meta-rule: any vector changing verdict between commits without a corresponding change to the rule set is a regression).
- **Deliverables**: `crates/otlp-conformance-harness/tests/vectors/{logs,traces,metrics}/{accept,reject}/*.bin`, each with a `.expected.json` sibling describing the expected verdict; a `corpus.rs` test that walks the directory and asserts the expected verdict for each vector; a CI workflow that runs `cargo test -p otlp-conformance-harness --all-targets`.
- **Test**: `cargo test -p otlp-conformance-harness corpus`.
- **Wall-clock**: under a day for the harness side; the CI workflow is a few lines of YAML.
- **Carpaccio taste tests**: All four pass; this slice is the one that pins them for the whole crate.

---

## Walking skeleton

Slice 01 is itself the walking skeleton. It introduces:

1. The crate at `crates/otlp-conformance-harness/`.
2. The `Cargo.toml` declaring the dependency on `opentelemetry-proto`.
3. The first public function (`validate_logs`).
4. The first violation rule (`EmptyInput`).
5. The first Cargo test exercising the public surface end to end.

Every later slice strengthens this skeleton without changing its shape.

---

## Priority Rationale

Priority order is determined by outcome impact and dependency, not by feature grouping. The brief commits to this sequence; the rationale below records why this order is the right one:

1. **Slice 01 first** because it is the riskiest assumption — *can we even ship a Cargo crate that exposes a typed validation API on top of `opentelemetry-proto`?* If the answer is no, every later slice is moot. Validating this with the smallest possible rule is the cheapest way to derisk the whole feature.

2. **Slices 02 and 03 next** because the reject path is what makes the harness valuable to Aperture (the gateway). Aperture's job is to refuse non-conforming traffic at the boundary; until the harness has a populated reject path, Aperture cannot ship its boundary check on Phase-1 schedule. The reject rules are also smaller-surface than the accept rules: defining what is rejected is easier than defining what is accepted, because rejection requires only one named rule whereas acceptance requires the full upstream type to round-trip cleanly.

3. **Slices 04, 05, 06 next** in signal-type order (logs → traces → metrics) because logs is the simplest OTLP signal (flat structure, fewest required attributes) and metrics is the most complex (point types, exemplars, aggregation temporality). Building accept-path complexity in this order makes each successive slice a small delta on the previous.

4. **Slice 07 last** because the corpus has nothing to defend until rules 01–06 are stable. Shipping the corpus before the rules would force corpus rewrites every time a rule was refined; shipping it after locks the contract once the rules are settled.

There is no MoSCoW classification for this feature: every slice is **Must Have**. The harness is the contract Aperture, Codex, Spark, and every storage engine depend on; partial coverage of OTLP signal types means partial coverage of Kaleidoscope's wire surface, which is not a shippable state for Phase 0.

---

## Scope Assessment: PASS — 7 stories, 1 bounded context, ships in well under two weeks of wall-clock

Carpaccio gate evaluation:

| Signal                                                            | Verdict                                                         |
|--------------------------------------------------------------------|------------------------------------------------------------------|
| >10 user stories?                                                  | No — 7 stories.                                                  |
| >3 bounded contexts or modules?                                    | No — single Cargo crate.                                         |
| Walking skeleton requires >5 integration points?                   | No — single dependency (`opentelemetry-proto`).                  |
| Estimated wall-clock >2 weeks?                                     | No — sum of slices is under two weeks of calendar time.          |
| Multiple independent user outcomes shippable separately?           | No — every slice serves the same outcome (a stable validation contract). |

The feature is right-sized for Phase 0 and proceeds to requirements crafting without splitting.

---

## Backlog suggestions (story IDs assigned in `user-stories.md`)

| Slice | Story ID  | Priority | Outcome link                              | Dependencies |
|-------|-----------|----------|--------------------------------------------|--------------|
| 01    | US-01     | P1       | False-positive rate stays at 0%           | None         |
| 02    | US-02     | P2       | Spec-coverage breadth (1/3 reject rules)  | US-01        |
| 03    | US-03     | P3       | Spec-coverage breadth (2/3 reject rules)  | US-02        |
| 04    | US-04     | P4       | Spec-coverage breadth (1/3 signals)       | US-02        |
| 05    | US-05     | P5       | Spec-coverage breadth (2/3 signals)       | US-04        |
| 06    | US-06     | P6       | Spec-coverage breadth (3/3 signals)       | US-04        |
| 07    | US-07     | P7       | Corpus size + CI regression invariant     | US-01..US-06 |
