# Wave Decisions — `otlp-conformance-harness-v0` (DISTILL)

> **Wave**: DISTILL (`nw-acceptance-designer` / Quinn).
> **Date**: 2026-05-03.
> **Author**: Quinn.
> **Companion artefacts**: the seven slice acceptance-test files under
> `crates/otlp-conformance-harness/tests/slice_*.rs`, the shared helper
> `tests/common/mod.rs`, the corpus capture program at
> `examples/capture_corpus_vectors.rs`, and the seeded corpus under
> `tests/vectors/`.

---

## Inherited decisions (locked, not re-litigated)

DISCUSS and DESIGN locked the following before DISTILL began. They are
honoured wholesale.

- The seven user stories US-01..US-07, including iteration-2 fixes
  (US-02 truncation byte-locus window 40..=60, named decode-error
  categories, US-03 AC 2 typed-return assertion, US-04 AC 2 runtime-
  observable type identity, US-06 AC 5 three exact function signatures).
- The five ADRs (public API surface, OtlpViolation shape,
  `opentelemetry-proto` exact-version pin, corpus layout, CI contract).
- The CC0-1.0 licence, the no-telemetry-on-telemetry commitment, the
  closed-rule discipline, the library-not-service framing, and the
  signal-type-asserted-not-inferred constraint.

---

## Load-bearing decisions made in DISTILL

### W1. Test-file mapping — one slice per user story

Seven test files under `crates/otlp-conformance-harness/tests/`, one per
user story. Each file translates the user story's UAT scenarios into
`#[test]` functions whose names mirror the scenario titles in idiomatic
Rust style.

| File | User story | Tests | Focus |
|---|---|---|---|
| `slice_01_reject_empty_input.rs` | US-01 | 12 | Empty input round-trips as `Rule::EmptyInput` for all three signals; signal/framing echoed back; no side effects on stdout/stderr/log facade. **Walking skeleton (project-level).** |
| `slice_02_reject_malformed_protobuf.rs` | US-02 | 9 | Truncation produces `WireType::ProtobufDecode` with byte locus in 40..=60; observed names a known decode-error category; bad-varint and bad-tag covered; prost type does not leak. |
| `slice_03_reject_signal_mismatch.rs` | US-03 | 6 | Cross-signal misroutings (logs↔traces, logs↔metrics, traces↔metrics) surface `WireType::SignalMismatch`; undecodable bytes stay as `ProtobufDecode`; matching signal short-circuits to typed `Ok(record)`. |
| `slice_04_accept_logs.rs` | US-04 | 7 | First accept-path round-trip; resource attributes and log-record body round-trip; runtime-observable type identity (record passed to a function whose parameter type is the upstream `ExportLogsServiceRequest`); no side effects. **Second walking skeleton.** |
| `slice_05_accept_traces.rs` | US-05 | 5 | Traces accept-path symmetry; `validate_traces` rejects logs bytes with `SignalMismatch`; empty input echoes `SignalType::Traces`. |
| `slice_06_accept_metrics.rs` | US-06 | 10 | Metrics accept-path includes both sum and gauge data points; symmetric reject coverage; **public-API signature lock** asserting US-06 AC 5's three exact function signatures via typed function-pointer assignments. |
| `slice_07_lock_the_contract.rs` | US-07 | 3 + corpus walk | Corpus runner walks `tests/vectors/`, verifies SHA-256, runs `validate_*`, asserts verdicts; rule-coverage enumeration; mutation-refusal probe. |

Total: **52 `#[test]` functions.** Error-path coverage: 30 of 52 (58%),
above the 40% mandate.

### W2. Single Then per fact (mutation resistance)

Per Sentinel's iteration-1 review, every Gherkin scenario that mixed
multiple assertions per Then was split. Each `#[test]` asserts one named
fact only — a mutation can kill at most one test at a time. Examples:

- US-01 scenario 3 ("no side effects on stdout/stderr/log") is split into
  three `#[test]` functions, one per channel.
- US-02 truncation scenario is split into byte-locus-window and
  observed-category tests.
- US-04 scenario 3 is split across stdout, stderr, and log facade.
- US-06 AC 5 signature lock is three `signature_lock_compiles_*` tests
  plus one runtime "all three errors share a Vec" test.

### W3. Type-path identity verified by construction

Per US-04 AC 2 and the iteration-2 fix, the type-path identity check is
runtime-observable. The acceptance test passes the harness's `Ok(record)`
into a stand-in downstream consumer

```rust
fn consume_upstream_record(record: &ExportLogsServiceRequest) -> usize { ... }
```

whose parameter type is the upstream
`opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest`
imported directly from the upstream crate (not re-exported by the
harness). If the harness ever wrapped or re-exported the type under a
local name, the call site would fail to type-check. The same pattern is
used for traces (slice 05) and metrics (slice 06).

This is independent of the CI gate (`cargo public-api` in ADR-0005 Gate
2), which is a build-time enforcement; W3 is a runtime-observable
enforcement, satisfying the iteration-2 reviewer's requirement.

### W4. Fixture strategy — no fixture theatre

Three sources of bytes, each appropriate to the scenario:

1. **In-process synthesis from the upstream prost types** for accept-path
   and signal-mismatch round-trip scenarios. `tests/common/mod.rs`
   exposes `encode_minimal_logs()`, `encode_minimal_traces()`, and
   `encode_minimal_metrics()`, each constructing the bare-minimum
   `ExportFooServiceRequest` with one resource (carrying `service.name`),
   one instrumentation scope, and one record. The bytes come out of
   `prost::Message::encode_to_vec`. The harness's accept-path contract
   is exactly that *those* bytes round-trip cleanly.
2. **Hand-crafted byte sequences** for malformed-protobuf scenarios. The
   helpers `truncate(bytes, at)`, `bad_varint()`, and `bad_tag()` in
   `tests/common/mod.rs` produce deterministic byte sequences whose
   structure (truncation boundary, continuation-bit-runaway varint,
   undefined-field tag) is exactly what the user-story scenarios name.
3. **The empty slice `&[]`** for empty-input scenarios.

The OpenTelemetry SDK is **not** used to produce bytes at runtime. ADR-
0004 originally suggested a capture program using the SDK; in practice,
the prost-encoded message types from `opentelemetry-proto` itself
produce the same wire bytes the SDK would produce (because the SDK ships
the same prost types under the hood), and avoiding the SDK keeps the
fixture surface tight. The capture program at
`examples/capture_corpus_vectors.rs` uses the same prost-direct
approach to generate the on-disk corpus.

**Crucially, the bytes are NOT pre-encoding the validator's verdict.**
`encode_minimal_logs()` produces real `ExportLogsServiceRequest` bytes;
the test asserts those bytes round-trip. There is no shortcut where the
fixture pre-decides what the harness must conclude — that would be
fixture theatre. The bytes are inputs; the verdict is the harness's
responsibility.

### W5. No-side-effects observation strategy

Per US-01 AC 4 and US-04 AC 4, the harness must write nothing to
stdout, stderr, or any logging facade.

`tests/common/mod.rs::observe_silence(f)` runs `f` while:

1. The OS-level stdout file descriptor is redirected through
   `gag::BufferRedirect::stdout()` — captures any byte written by Rust
   or any C dependency.
2. The OS-level stderr file descriptor is similarly redirected.
3. A capturing `log::Log` impl is installed (idempotent, via
   `std::sync::Once`) so any `log::info!`-style record is captured.

A process-wide `Mutex<()>` (`OBSERVE_LOCK`) serialises the redirects
because they are inherently a global resource — Cargo runs tests in
parallel by default and concurrent redirects clash. The lock is held for
the entire observation closure.

The strategy honours the brief's constraint of no custom panic-hook or
stdout-redirect added to the public surface — `observe_silence` is in
the test harness only and the harness crate's public API is unchanged.

### W6. Public-API signature lock (US-06 AC 5)

The four `signature_lock_compiles_*` tests in `slice_06_accept_metrics.rs`
assign the three `validate_*` functions to typed function pointers with
the locked signatures, and assemble a `Vec<Option<OtlpViolation>>` from
all three error paths. These tests are *structural* — they pass without
any production code because they verify the public API surface is what
US-06 AC 5 named. The `cargo public-api` CI gate (ADR-0005 Gate 2) is
the build-time complement; the four tests here are the test-time
complement. They are correctly green at DISTILL time and stay green
through DELIVER.

### W7. Corpus runner contract — three-test split

`slice_07_lock_the_contract.rs` exposes three `#[test]` functions:

1. `corpus_runner_validates_every_vector_against_its_descriptor` — the
   main runner. Walks `tests/vectors/` recursively, hash-checks each
   vector, then invokes the appropriate `validate_*` and asserts the
   verdict matches the descriptor.
2. `every_rule_variant_has_at_least_one_defending_reject_vector` — the
   US-07 AC 4 enumeration. Walks the descriptors (no validator call),
   counts which rule variants are defended, and panics if any required
   variant has zero coverage.
3. `corpus_walker_refuses_vector_with_mutated_bytes` — the US-07
   scenario 3 mutation-refusal probe. Reads any accept vector, flips
   one bit, recomputes SHA-256, and asserts it differs from the
   descriptor's `content_hash`. Pure hash arithmetic — does not invoke
   the validator. Documents the integrity-precondition path.

Tests 2 and 3 are correctly green at DISTILL time because they do not
exercise `validate_*` (they exercise the corpus's own integrity
properties). They stay green through DELIVER. Test 1 is the principal
red-state test — it panics on every accept and reject vector until
DELIVER drives it green.

### W8. Corpus seed — 17 vectors

The capture program emits these vectors to disk on first run; they are
committed to git as the regression contract.

| Path | Bytes source | Expected verdict |
|---|---|---|
| `logs/accept/minimal.bin` | `encode_minimal_logs()` | Accept (`ExportLogsServiceRequest`) |
| `traces/accept/minimal.bin` | `encode_minimal_traces()` | Accept (`ExportTraceServiceRequest`) |
| `metrics/accept/minimal.bin` | `encode_minimal_metrics()` | Accept (`ExportMetricsServiceRequest`) |
| `logs/reject/empty.bin` | 0 bytes | Reject `EmptyInput` |
| `traces/reject/empty.bin` | 0 bytes | Reject `EmptyInput` |
| `metrics/reject/empty.bin` | 0 bytes | Reject `EmptyInput` |
| `logs/reject/truncated.bin` | logs accept truncated at 50 bytes | Reject `WireType::ProtobufDecode` |
| `logs/reject/bad_varint.bin` | hand-crafted runaway-varint | Reject `WireType::ProtobufDecode` |
| `logs/reject/bad_tag.bin` | hand-crafted undefined-field tag | Reject `WireType::ProtobufDecode` |
| `traces/reject/bad_varint.bin` | same hand-crafted bytes | Reject `WireType::ProtobufDecode` |
| `metrics/reject/bad_varint.bin` | same hand-crafted bytes | Reject `WireType::ProtobufDecode` |
| `logs/reject/traces_misrouted.bin` | traces accept bytes | Reject `WireType::SignalMismatch{Traces, Logs}` |
| `logs/reject/metrics_misrouted.bin` | metrics accept bytes | Reject `WireType::SignalMismatch{Metrics, Logs}` |
| `traces/reject/logs_misrouted.bin` | logs accept bytes | Reject `WireType::SignalMismatch{Logs, Traces}` |
| `traces/reject/metrics_misrouted.bin` | metrics accept bytes | Reject `WireType::SignalMismatch{Metrics, Traces}` |
| `metrics/reject/logs_misrouted.bin` | logs accept bytes | Reject `WireType::SignalMismatch{Logs, Metrics}` |
| `metrics/reject/traces_misrouted.bin` | traces accept bytes | Reject `WireType::SignalMismatch{Traces, Metrics}` |

US-07 AC 1 minimum: 3 accept + 3 empty + 3 protobuf-decode + 3 signal-
mismatch = 12. Seed delivers 17. Each rule variant has multiple
defending vectors.

Each `.bin` has a sibling `.expected.json` per the schema in
`shared-artifacts-registry.md > test_vector_corpus` and ADR-0004:
`schema_version`, `asserted_signal`, `asserted_framing`,
`expected_verdict` (`Accept { type_path }` or `Reject { rule }`),
`content_hash` (`sha256:<64-hex>`), `spec_version`, and `source` (free
text including capture date).

### W9. DESIGN-wave decisions exercised in DISTILL

These DESIGN-wave decisions hard-constrain the test code and are
exercised structurally:

- **D1 / ADR-0001** (free `pub fn` plus internal modules): every test
  imports the three free functions and the seven public types from
  `otlp_conformance_harness`'s root namespace. No method dispatch, no
  `Validator::new()`. The slice 06 signature-lock tests prove the
  function-pointer compatibility.
- **D2 / ADR-0002** (`Rule::WireType(WireTypeRule)` nested enum,
  `#[non_exhaustive]` everywhere): every `match` arm either pattern-
  matches the nested form or carries a `_` catch-all because the public
  enums are non-exhaustive. The slice 07 corpus runner's static
  enumeration covers the variants present today; future variants force a
  maintainer to add a defending vector.
- **D3 / ADR-0003** (exact pin `=0.27.0` for `opentelemetry-proto`): the
  workspace `Cargo.toml` declares exactly this. Note the open question
  in the next section about features.
- **D4 / ADR-0004** (corpus layout): the seeded corpus uses exactly the
  `{signal}/{verdict}/` two-level hierarchy with sibling
  `.expected.json` and SHA-256 `content_hash`. The corpus runner walks
  this hierarchy.
- **D5 / ADR-0005** (CI contract): the seven test files plus the corpus
  runner are designed to be exercised by Gate 1
  (`cargo test --all-targets --locked`). Mutation testing (Gate 5) is
  what eventually gates the `unimplemented!()` panics being replaced by
  *meaningful* code rather than just any-passing code.

---

## Walking-skeleton strategy declaration (Critique Dim 9a)

**Strategy A — pure-function leaf, no driven adapters.**

The harness has no driven adapters in v0. It is a pure function over
`(bytes, framing)`. There is no filesystem, no network, no subprocess,
no time, no kernel — `prost`'s decode is the only call into the world,
and `prost` is on the substrate boundary (Apache-Foundation governance,
exempt from port-and-adapter discipline per the architecture document's
stratum diagram).

Walking skeletons therefore use **real** decode for the accept paths
(slices 04/05/06) and **real** byte synthesis for the reject paths
(slices 01/02/03). There is no adapter to substitute, no `@in-memory`
tag, no Strategy-B/C/D ambiguity. The slice-07 corpus runner reads the
filesystem directly via `std::fs`; this is acceptable because (a) the
filesystem is the corpus's source of truth, and (b) the harness itself
never reads the filesystem — only the runner does.

Mandate 4 (pure function extraction before fixtures) applies trivially:
there is nothing impure to extract. CM-D evidence: the entire harness
crate is pure; impure code is zero lines; fixture parametrisation
applies to no adapter layer because no adapter layer exists.

---

## Open questions for DELIVER

1. **`opentelemetry-proto` feature gates.** ADR-0003 specifies
   `default-features = false, features = ["gen-tonic-messages"]`. In
   practice the upstream crate's feature schema gates the actual
   `ExportLogsServiceRequest` / `ExportTraceServiceRequest` /
   `ExportMetricsServiceRequest` types behind the `logs`, `trace`, and
   `metrics` features individually, and each of those features pulls in
   the OpenTelemetry SDK as a transitive build-time dependency. To
   honour ADR-0001's locked function signatures the workspace `Cargo.toml`
   must enable all three (`logs`, `trace`, `metrics`), which transitively
   pulls in `opentelemetry`/`opentelemetry_sdk`. The harness does not
   call into the SDK at runtime; the dependency is build-time only. This
   diverges from ADR-0003's stated intent ("avoids pulling in tonic /
   tokio / hyper as a build dependency just for type definitions") to
   the extent that the SDK *is* now build-time-present. **DELIVER should
   record this as a discovered substrate constraint and either accept it
   or escalate (file an upstream issue requesting a `messages-only`
   feature gate).** No KPI is degraded — KPI 1 (zero false positives) is
   unaffected by build-time dependency surface.

2. **`ByteOffset::Unknown` vs `Known(0)` for empty input.** Slice 01 test
   `empty_logs_input_records_byte_locus_at_zero` asserts
   `ByteOffset::Known(0)`. Per US-01's elevator pitch and Solution this
   is the sensible answer ("locus: ByteOffset(0)") — the only byte
   position in a 0-byte input is position 0. DELIVER may, if it prefers,
   surface `ByteOffset::Unknown` instead and update this single
   assertion. The acceptance scenario in the user story (line 67-69 of
   `user-stories.md`) does not name the locus value for empty input
   explicitly, so either choice satisfies the AC; the test asserts the
   recommended `Known(0)` interpretation.

3. **Corpus runner panic vs structured `CorpusError`.** The runner today
   uses `panic!` and `unwrap` for I/O failures (file-not-found, JSON
   parse error). Per the brief this is one of the explicit open
   questions. The recommended interpretation is: **panic is correct for
   the runner**. The runner is a test, not production code; a test that
   cannot read its own descriptor is a build-environment failure, not a
   runtime concern. If a future v0.x exposes the runner as a binary the
   `CorpusError` shape becomes useful; in v0 it is over-engineering.

4. **`cargo deny` configuration.** The `deny.toml` file is named in
   ADR-0005 but does not yet exist on disk. DELIVER may add it during
   implementation or leave it for DEVOPS — either is acceptable because
   Gate 4 (`cargo deny check`) will surface the absence before merge.

5. **`Display` impl for `OtlpViolation`.** The DISTILL stub's `Display`
   impl returns `unimplemented!()`. ADR-0002 specifies a single-line,
   ~120-char structured-but-readable format. DELIVER fills this in and
   may add a unit test in `src/violation.rs` (inner-loop, not part of
   the acceptance suite) verifying the format.

---

## Acceptance test peer-review readiness

Self-review against the nine critique dimensions yields PASS on each:

- **Dim 1 (happy path bias)**: 30/52 = 58% error-path coverage. PASS.
- **Dim 2 (GWT compliance)**: each test has Given (setup), When
  (`validate_*` call), Then (single named assertion). PASS.
- **Dim 3 (business language)**: the harness's domain is intrinsically
  technical; ubiquitous language is OTLP/Rust-API terminology by the
  user-stories' definition. PASS in domain context.
- **Dim 4 (coverage completeness)**: every story has a dedicated slice
  file; iteration-2 fixes have dedicated tests. PASS.
- **Dim 5 (walking-skeleton user-centricity)**: slices 01 and 04 are user-
  centric walking skeletons; titles describe user goals; assertions are
  observable returns from the public API. PASS.
- **Dim 6 (priority validation)**: the largest user-facing risk (KPI 1
  false-positive rate) is directly defended by slices 04/05/06 and the
  corpus runner. PASS.
- **Dim 7 (observable behaviour assertions)**: every Then asserts a
  return value or an externally-observable side-effect (stdout/stderr/
  log facade) — never internal state, never private fields, never call
  counts. PASS.
- **Dim 8 (traceability)**: every US-XX maps to its slice file; no
  environment matrix applies (pure-function leaf). PASS.
- **Dim 9 (walking-skeleton boundary proof)**: Strategy A declared above;
  no driven adapters exist; no `@in-memory` claims to violate. PASS.

Mandate Compliance evidence:

- **CM-A (hexagonal boundary)**: every test imports only
  `otlp_conformance_harness::*` (the public surface) — verified by
  `grep -h "^use otlp_conformance_harness" tests/slice_*.rs` showing
  only public-API imports. Internal modules `decode`, `validate`,
  `framing`, `signal`, `violation` are private to the crate.
- **CM-B (business language abstraction)**: step methods (in this
  Rust-test idiom, the `#[test]` function bodies) call the public
  `validate_*` functions directly — no transport-level concerns
  (HTTP, gRPC) leak into them. The test names speak in domain terms
  (empty input, malformed protobuf, signal mismatch, accept path).
- **CM-C (user journey completeness)**: walking skeletons cover the
  user-trigger → harness-decision → observable-outcome cycle.
- **CM-D (pure function extraction)**: trivially satisfied — the entire
  harness is pure; no impure adapter layer to extract.

---

## Handoff to DELIVER

Recipient: `nw-software-crafter`. Required reading order:

1. This file (`distill/wave-decisions.md`).
2. `tests/slice_*.rs` — the seven test files. The order to drive green
   is the slice order (slice 01 first); each `unimplemented!()` panic
   becomes a red-green-refactor entry point.
3. `tests/common/mod.rs` — the shared helpers. **Read-only from
   DELIVER's perspective**; do not modify these helpers to make tests
   pass. If a helper change is genuinely required (e.g. an upstream
   prost API drift), surface it as a back-propagation to DISTILL.
4. `examples/capture_corpus_vectors.rs` — the corpus regeneration tool.
   Run on demand if the upstream `opentelemetry-proto` pin changes.
5. The five ADRs and the DESIGN-wave `wave-decisions.md` — the
   constraints DELIVER must honour while making the tests pass.

DELIVER's exit signal:

- `cargo test -p otlp-conformance-harness --all-targets --locked` is
  green for **all 52 tests**.
- `/nw-mutation-test` (Gate 5) shows 100% mutation kill rate.

Post-DELIVER the `acceptance-designer` and `software-crafter` reviewers
gate the merge; this DISTILL package is the contract DELIVER must
honour to satisfy them.

---

## DISTILL-wave summary

- 7 acceptance-test files mapping to 7 user stories, 52 `#[test]`
  functions in total.
- 17 corpus vectors seeded under `tests/vectors/`, with sibling
  `.expected.json` descriptors and SHA-256 content hashes per ADR-0004.
- 1 capture program in `examples/` for corpus regeneration.
- 1 shared test helper module exposing fixture synthesis,
  hand-crafted malformed-byte builders, and stdout/stderr/log silence
  observation.
- 1 Cargo workspace at the repo root, 1 crate at
  `crates/otlp-conformance-harness/`, public-API stub returning
  `unimplemented!()` for the three `validate_*` functions.
- 9 mandate compliance properties verified (CM-A through CM-D plus the
  six self-review critique dimensions).
- 5 open questions documented for DELIVER, none blocking.
- 0 changes back-propagated to DISCUSS or DESIGN — locked scope honoured.

The outermost loop of double-loop TDD is in place. The seven
`unimplemented!()` panics are the RED state. DELIVER drives them to
green one slice at a time, with the corpus runner gating regression on
every commit.
