# Definition of Ready Validation — `otlp-conformance-harness-v0`

> 9-item hard gate per `nw-leanux-methodology` (DoR items 1–8) and `nw-outcome-kpi-framework` (item 9). Every user story must pass every item with evidence before handoff to DESIGN. Item 0 (Elevator Pitch presence and quality) is owned by the reviewer (Dimension 0); it is shown here for completeness because the brief made the Elevator Pitch a mandatory part of every story.

---

## US-01 — Reject empty input with a structured violation

| # | DoR Item | Status | Evidence/Issue |
|---|----------|--------|----------------|
| 0 | Elevator Pitch present (Before / After / Decision enabled) | PASS | Section "Elevator Pitch" in `user-stories.md`. After-line names a runtime entry point (`validate_logs(...)`) and a Cargo-test entry point (`cargo test -p otlp-conformance-harness slice_01_empty_rejected`). |
| 1 | Problem statement clear, in domain language | PASS | "A Kaleidoscope component receives bytes from an external source. Before doing anything else with those bytes, the component must reject obvious garbage..." Names the domain (OTLP boundary check) and the pain (component authors must invent their own validation). |
| 2 | User/persona identified with specific characteristics | PASS | Three personas named with role and context: Aperture v0 author (Phase 1 component), third-party observability engineer at `acme-observability`, Kaleidoscope CI. |
| 3 | At least 3 domain examples with real data | PASS | Three examples: Aperture rejecting an empty POST body from a misconfigured Spark client; `acme-observability`'s emitter regression caught at CI time; Kaleidoscope's own corpus vector at `tests/vectors/logs/reject/empty.bin`. Each names real (project-grounded) actors and concrete data. |
| 4 | UAT scenarios in Given/When/Then (3-7) | PASS | 3 scenarios: empty input rejected; rejection symmetric across signals; no side effects on the reject path. |
| 5 | AC derived from UAT | PASS (re-verified iteration 2) | 6 acceptance criteria, each tied to a scenario or to the Solution section. AC 4 was tightened in iteration 2 per Sentinel's suggestion 5 to enumerate the three observable channels (stdout, stderr, logging facade) explicitly, giving the DISTILL author a concrete assertion target. |
| 6 | Right-sized | PASS | "Wall-clock: under a day. Conceptual difficulty: low." 3 UAT scenarios, single demonstrable behaviour. |
| 7 | Technical notes identify constraints | PASS | "Depends on the harness crate scaffolding (Cargo.toml, lib.rs, the `Framing` and `SignalType` enums, the `OtlpViolation` struct). All of those are introduced by this slice." Plus the System Constraints section at the top of `user-stories.md`. |
| 8 | Dependencies tracked | PASS | "None. This is the first slice." |
| 9 | Outcome KPIs defined with measurable targets | PASS | KPI table: 100% of empty-body vectors produce `Rule::EmptyInput`, measured by the corpus runner. |

**DoR Status: PASSED**

---

## US-02 — Reject malformed protobuf with a structured violation

| # | DoR Item | Status | Evidence/Issue |
|---|----------|--------|----------------|
| 0 | Elevator Pitch | PASS | Before/After/Decision-enabled, with concrete entry points (function call and `cargo test slice_02_malformed_protobuf_rejected`). |
| 1 | Problem statement clear | PASS | Names the pain: every consumer would otherwise translate `prost::DecodeError` into something useful for itself. |
| 2 | User/persona | PASS | Aperture v0 author, third-party engineer, Kaleidoscope CI — same triad, with role-specific context per story. |
| 3 | 3+ domain examples with real data | PASS | Three examples: truncated logs body; varint-corruption bug at `acme-observability`; corpus vectors `truncated.bin`, `bad_varint.bin`, `bad_tag.bin` with concrete byte offsets. |
| 4 | UAT scenarios | PASS (re-verified iteration 2) | 5 scenarios (within the 3–7 ceiling): truncated-body byte locus is between 40 and 60 inclusive; truncated-body `observed` field contains one of a named decode-error set ("unexpected EOF", "wire type error", "missing length-delimited data"); invalid varint rejected with the same named-category constraint on `observed`; bad tag rejected; prost type is not leaked. The original truncated-body scenario was split per Sentinel's iteration-1 finding (high) to be mutation-resistant: the byte-offset assertion now forces a meaningful locus computation (no always-zero), and the named decode-error categories prevent generic "error occurred" mutations. |
| 5 | AC derived from UAT | PASS | 5 ACs covering rule emission, byte-locus reporting, prost encapsulation, corpus coverage, and test-command greenness. |
| 6 | Right-sized | PASS | "Wall-clock: under a day." 5 UAT scenarios after the iteration-2 split — within the 3–7 ceiling. |
| 7 | Technical notes | PASS | Names the primary uncertainty (best-effort byte locus from `prost::DecodeError`) and the fallback (`ByteOffset::Unknown`). |
| 8 | Dependencies tracked | PASS | "US-01." |
| 9 | Outcome KPIs | PASS | 100% of malformed-bytes vectors produce `ProtobufDecode`, measured by corpus runner. |

**DoR Status: PASSED**

---

## US-03 — Reject valid protobuf of the wrong signal type

| # | DoR Item | Status | Evidence/Issue |
|---|----------|--------|----------------|
| 0 | Elevator Pitch | PASS | Before/After/Decision-enabled with the runtime call and `cargo test slice_03_signal_mismatch_rejected`. |
| 1 | Problem statement | PASS | Names the routing-error-becomes-data-corruption-error pain. |
| 2 | User/persona | PASS | Aperture v0 author, `acme-observability` engineer copy-pasting, Kaleidoscope CI. |
| 3 | 3+ domain examples | PASS | Misrouted Spark client; `acme-observability`'s metrics-vs-logs serialiser swap; corpus vector `traces_misrouted.bin`. |
| 4 | UAT scenarios | PASS | 3 scenarios: traces-as-logs rejected; metrics-as-logs rejected; no-decode-fallback to ProtobufDecode preserved. |
| 5 | AC derived from UAT | PASS (re-verified iteration 2) | 5 ACs covering each path. AC 2 was rewritten in iteration 2 per Sentinel's blocking finding: the previous internal-state assertion ("does not enter the alternative-decode path") is replaced by an observable, runtime-testable claim — on a matching signal the harness returns `Ok(record)` immediately and the returned record is the typed upstream value (not an intermediate state, surrogate, or harness-local wrapper). Verifiable by a Cargo unit test that pattern-matches on the return value. |
| 6 | Right-sized | PASS | "A couple of hours; piggybacks on slice 02's decode path." 3 scenarios. |
| 7 | Technical notes | PASS | Discusses the alternative-decode strategy and its cost; explicitly defers a faster type-discriminator to a follow-up. |
| 8 | Dependencies tracked | PASS | "US-02." |
| 9 | Outcome KPIs | PASS | 100% of signal-mismatch vectors produce the rule with correct observed/asserted, measured by corpus runner. |

**DoR Status: PASSED**

---

## US-04 — Accept a minimally valid OTLP logs record

| # | DoR Item | Status | Evidence/Issue |
|---|----------|--------|----------------|
| 0 | Elevator Pitch | PASS | Before/After/Decision-enabled. |
| 1 | Problem statement | PASS | Names the pain: harness must hand back a usable upstream type, not a wrapper. |
| 2 | User/persona | PASS | Aperture v0 author forwarding to Loki; `acme-observability` validating their custom Rust SDK; Kaleidoscope CI. |
| 3 | 3+ domain examples | PASS | Three examples grounded in real components and the real OpenTelemetry Rust SDK. |
| 4 | UAT scenarios | PASS (re-verified iteration 2) | 3 scenarios: minimal record accepted; runtime-observable downstream usability (the returned record is passed to a function whose parameter type is the upstream `ExportLogsServiceRequest` and the call type-checks and runs without conversion); no side effects on accept path. Scenario 2 was reframed in iteration 2 per Sentinel's high finding: the previous compile-time-only assertion ("the type is not re-exported under a harness-local name") is now a runtime check on observable downstream usability, with the type-path identity check moved to AC 2 as a CI invariant. |
| 5 | AC derived from UAT | PASS (re-verified iteration 2) | 5 ACs covering Ok return, type-path identity (verified by a CI check on the public API; mechanism choice — `cargo expand`, `cargo doc --no-deps` grep, or `cargo public-api` — deferred to DESIGN), vector capture method, no side effects (assertion observed across stdout, stderr, and the logging facade), and test greenness. AC 2 is new in iteration 2; AC 4 was tightened in iteration 2 to enumerate the three observable channels per Sentinel's suggestion 5. |
| 6 | Right-sized | PASS | "Wall-clock: under a day. Conceptual difficulty: low; the work is the test fixture, not the code." 3 UAT scenarios. |
| 7 | Technical notes | PASS | Names the corpus-capture program design (a `dev-dependency` example, not a runtime dep). |
| 8 | Dependencies tracked | PASS | "US-03." |
| 9 | Outcome KPIs | PASS | 100% of accept-path logs vectors return Ok — the false-positive-rate north star. |

**DoR Status: PASSED**

---

## US-05 — Accept a minimally valid OTLP traces record

| # | DoR Item | Status | Evidence/Issue |
|---|----------|--------|----------------|
| 0 | Elevator Pitch | PASS | Before/After/Decision-enabled. |
| 1 | Problem statement | PASS | Names the pain: traces is the second-most-complex signal; symmetry test for the harness's signal abstraction. |
| 2 | User/persona | PASS | Aperture v0 author, future Ray v0 author (Phase 5), third-party engineer. |
| 3 | 3+ domain examples | PASS | Three examples: Aperture forwarding to Tempo; `acme-observability` emitter port to OTel; corpus accept vector. |
| 4 | UAT scenarios | PASS | 3 scenarios: traces accepted; logs-as-traces rejected with SignalMismatch; empty input rejected with EmptyInput. |
| 5 | AC derived from UAT | PASS | 4 ACs covering Ok return, reject-rule symmetry, corpus capture, test greenness. |
| 6 | Right-sized | PASS | "A half-day's work." 3 UAT scenarios. |
| 7 | Technical notes | PASS | "Reuses the decode path and the violation rule set. The capture program from US-04 is extended to emit traces." |
| 8 | Dependencies tracked | PASS | "US-04." |
| 9 | Outcome KPIs | PASS | 100% of traces accept vectors return Ok. |

**DoR Status: PASSED**

---

## US-06 — Accept a minimally valid OTLP metrics record

| # | DoR Item | Status | Evidence/Issue |
|---|----------|--------|----------------|
| 0 | Elevator Pitch | PASS | Before/After/Decision-enabled. |
| 1 | Problem statement | PASS | Names the pain: metrics is the most complex of the three; full coverage of the OTLP stable spec depends on it. |
| 2 | User/persona | PASS | Aperture v0 author, future Pulse v1 author (Phase 4), third-party engineer porting Prometheus remote-write to OTLP. |
| 3 | 3+ domain examples | PASS | Three concrete examples including a Prometheus-remote-write-to-OTLP bridge. |
| 4 | UAT scenarios | PASS | 3 scenarios: minimal metrics accepted; traces-as-metrics rejected with SignalMismatch; reject-rule symmetry across all three rules. |
| 5 | AC derived from UAT | PASS (re-verified iteration 2) | 5 ACs covering Ok return, reject-rule symmetry, corpus capture (with sum and gauge), test greenness, and the explicit public-API contract. AC 5 was rewritten in iteration 2 per Sentinel's blocking finding: the previous "same return-shape pattern" wording (untestable without inventing the contract) is replaced by the three exact function signatures (`validate_logs(bytes: &[u8], framing: Framing) -> Result<ExportLogsServiceRequest, OtlpViolation>`, `validate_traces(...) -> Result<ExportTraceServiceRequest, OtlpViolation>`, `validate_metrics(...) -> Result<ExportMetricsServiceRequest, OtlpViolation>`) plus the explicit assertion that all three return the same `OtlpViolation` type on the error path. |
| 6 | Right-sized | PASS | "A half-day's work." 3 UAT scenarios. |
| 7 | Technical notes | PASS | Profiles deferral named explicitly with the reason (OTel signal still in development). |
| 8 | Dependencies tracked | PASS | "US-05." |
| 9 | Outcome KPIs | PASS | 3-of-3 signal-coverage breadth — the metric reaches its v0 target at the close of this slice. |

**DoR Status: PASSED**

---

## US-07 — Lock the contract with a reference corpus and a CI gate

| # | DoR Item | Status | Evidence/Issue |
|---|----------|--------|----------------|
| 0 | Elevator Pitch | PASS | Before/After/Decision-enabled, with the corpus runner test command and the CI workflow contract. |
| 1 | Problem statement | PASS | Names the pain: hand-written tests scatter the contract; without a corpus, regressions can hide between tests. |
| 2 | User/persona | PASS | Aperture v0 author, `acme-observability` engineer using vectors as fixtures, Kaleidoscope CI. |
| 3 | 3+ domain examples | PASS | Three examples: maintainer-introduced verdict flip caught by CI; third-party engineer using vectors as fixtures; Aperture's CI run gating on the harness's corpus. |
| 4 | UAT scenarios | PASS | 4 scenarios: every accept vector returns Ok; every reject vector returns its declared rule; mutated vectors fail the integrity check; new rules must be defended by reject vectors. |
| 5 | AC derived from UAT | PASS | 6 ACs covering directory layout, descriptor format with content hash, hash verification, rule-coverage enumeration, test greenness, and README documentation. |
| 6 | Right-sized | PASS | "Under a day for the harness side; the CI workflow is a few lines of YAML." 4 UAT scenarios — within the 3-7 ceiling. |
| 7 | Technical notes | PASS (re-verified iteration 2) | Names the runner-neutral CI contract (workflow runner choice deferred to DEVOPS). Iteration 2 added the explicit hash algorithm and storage format per Sentinel's suggestion 1: SHA-256, hex-encoded, stored under `content_hash` in the sibling `.expected.json`, computed at vector creation and re-verified by the corpus runner before every validation run. |
| 8 | Dependencies tracked | PASS | "US-01, US-02, US-03, US-04, US-05, US-06 (all of them)." |
| 9 | Outcome KPIs | PASS | 100% of accept paths and 100% of reject rules defended; KPI 6 in `outcome-kpis.md`. |

**DoR Status: PASSED**

---

## Aggregate verdict

| Story | DoR Status |
|-------|------------|
| US-01 | PASSED |
| US-02 | PASSED |
| US-03 | PASSED |
| US-04 | PASSED |
| US-05 | PASSED |
| US-06 | PASSED |
| US-07 | PASSED |

All seven stories pass the 9-item Definition of Ready hard gate. Handoff to DESIGN is unblocked, pending peer review.

### Iteration 2 re-verification summary

Iteration 2 applied four substantive fixes plus three non-blocking suggestions (1, 2, 5) per `peer-review-iteration-1-acceptance-designer.md`. The DoR re-verification covered the items whose evidence changed:

| Story | Items re-verified | Outcome |
|-------|-------------------|---------|
| US-01 | item 5 (AC tightened — three observable channels enumerated) | PASS |
| US-02 | item 4 (scenario split — mutation-resistant byte locus and `observed`-field categories), item 6 (right-sized re-checked: 5 of 7 scenarios) | PASS |
| US-03 | item 5 (AC 2 rewritten as observable claim) | PASS |
| US-04 | item 4 (scenario 2 reframed as runtime-observable), item 5 (AC 2 added for type-path identity via CI; AC 4 tightened) | PASS |
| US-06 | item 5 (AC 5 rewritten with exact signatures) | PASS |
| US-07 | item 7 (technical notes name SHA-256 and storage format) | PASS |
| US-05 | unchanged | PASS |

All seven stories remain DoR PASS after iteration 2.

## Reviewer-facing checks (Dimension 0 quick scan)

For the reviewer's convenience, the table below summarises the Elevator Pitch checks the reviewer applies first:

| Story | Section present? | After-line entry point real? | "Sees" line concrete? | Decision enabled? |
|-------|------------------|------------------------------|------------------------|-------------------|
| US-01 | Yes              | `validate_logs(&[], _)` and `cargo test slice_01_empty_rejected` | Returns `Err(OtlpViolation { rule: EmptyInput, ... })` with concrete fields | Aperture author decides whether to embed the harness in their boundary check |
| US-02 | Yes              | `validate_logs(corrupted_bytes, _)` and `cargo test slice_02_malformed_protobuf_rejected` | Returns `Err(...)` with byte locus | Aperture author decides whether the reject path is rich enough for a 400-class response |
| US-03 | Yes              | `validate_logs(traces_bytes, _)` and `cargo test slice_03_signal_mismatch_rejected` | Returns `Err(...)` with `observed` and `asserted` signals | Aperture author decides whether the harness can be the only check between routing and storage |
| US-04 | Yes              | `validate_logs(real_logs_bytes, _)` and `cargo test slice_04_logs_accepted` | Returns `Ok(ExportLogsServiceRequest)` — upstream type | Aperture author decides whether the accept-path return type fits their downstream pipeline |
| US-05 | Yes              | `validate_traces(real_traces_bytes, _)` and `cargo test slice_05_traces_accepted` | Returns `Ok(ExportTraceServiceRequest)` | Aperture author confirms traces accept-path symmetry |
| US-06 | Yes              | `validate_metrics(real_metrics_bytes, _)` and `cargo test slice_06_metrics_accepted` | Returns `Ok(ExportMetricsServiceRequest)` | Aperture author confirms three-signal coverage is complete |
| US-07 | Yes              | `cargo test -p otlp-conformance-harness corpus` and the CI workflow contract | Corpus runner walks the directory and asserts verdict per descriptor | Aperture author and third-party engineer decide whether the contract is stable enough to depend on |

Slice-level check: not every slice is `@infrastructure`. US-01 through US-06 each deliver a user-visible (caller-visible) behavioural outcome via the public API; US-07 delivers a contract-stability outcome. Per Dimension 0's slice-level rule, the slice set has user-visible value at every step.
