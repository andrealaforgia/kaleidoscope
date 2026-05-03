# Shared Artefacts Registry — `otlp-conformance-harness-v0`

> Artefacts are values that appear in more than one place across the harness's API, its consumers, and its test corpus. Every artefact has a single source of truth and a documented set of consumers. Hardcoded duplication of any artefact below is a horizontal-integration defect.

---

## otlp_wire_format

- **Source of truth**: the [`opentelemetry-proto`](https://crates.io/crates/opentelemetry-proto) Rust crate, which mirrors the protobuf schemas at [github.com/open-telemetry/opentelemetry-proto](https://github.com/open-telemetry/opentelemetry-proto).
- **Consumers**: the harness itself (decode and accept-path types); Aperture (boundary check at ingest); Spark (emission); Codex (semconv schema validation pinning); every storage engine (persistence types).
- **Owner**: upstream OpenTelemetry project. The Kaleidoscope harness pins a specific minor version in `Cargo.toml` and bumps it deliberately.
- **Integration risk**: HIGH — drift between the version pinned by the harness and the version pinned by other Kaleidoscope components causes silent acceptance of records the rest of the platform cannot process.
- **Validation**: a workspace-level `cargo metadata` check verifies that the harness and any consumer in the workspace pin compatible `opentelemetry-proto` versions. (Implementation belongs to a future workspace-hygiene story; for v0, the harness is the only consumer.)

---

## otlp_spec_version

- **Source of truth**: a constant in the harness's `Cargo.toml` metadata table, e.g. `[package.metadata.kaleidoscope.otlp] spec_version = "1.5.0"`. The constant is re-exported from `lib.rs` as `pub const OTLP_SPEC_VERSION: &str`.
- **Consumers**: the harness's own diagnostics (`OtlpViolation` carries the spec version active at validation time); the test corpus manifest (vectors are tagged with the spec version they were captured under); Codex (Phase 0 deliverable that pins the same version for semconv).
- **Owner**: harness crate metadata. Bumped with each spec-version uplift, requiring a corpus revalidation pass.
- **Integration risk**: MEDIUM — version mismatch causes confusion in violation reports but does not cause silent acceptance, because the wire-level types come from `opentelemetry-proto` regardless.
- **Validation**: corpus manifests must declare the spec version they target; the corpus runner refuses to run vectors whose declared version does not match `OTLP_SPEC_VERSION`.

---

## violation_rule_set

- **Source of truth**: the `OtlpViolation::Rule` enum defined in the harness crate (single file: `src/violation.rs`).
- **Consumers**: every caller pattern-matching on violations (Aperture's reject branch, third-party engineers' debugging code, the corpus runner's expected-verdict matcher).
- **Owner**: harness crate. New rules are additive within a minor version; existing rules cannot be renamed without a major version bump.
- **Integration risk**: HIGH — adding a rule without bumping the crate's minor version causes downstream `match` arms to become non-exhaustive at compile time (Rust prevents the silent failure, but the breakage propagates as a compile error to consumers).
- **Validation**: the harness exports the rule set via a `pub` enum. Consumers depending on exhaustive matching declare so explicitly with `#[deny(non_exhaustive_omitted_patterns)]` if they choose to.

---

## test_vector_corpus

- **Source of truth**: `crates/otlp-conformance-harness/tests/vectors/` directory, containing `{signal_type}/{verdict}/{vector_name}.bin` byte sequences with `{vector_name}.expected.json` sibling descriptors.
- **Consumers**: the harness's own `corpus` integration test; third-party observability engineers running the harness against their emitter (the corpus is example input as well as test fixture); Kaleidoscope CI.
- **Owner**: harness crate, slice 07 introduces it.
- **Integration risk**: HIGH — a vector whose bytes are mutated without a corresponding rule change is a regression; a vector whose expected verdict is mutated without a rule change is also a regression.
- **Validation**: each `.bin` file's content hash is recorded in the sibling `.expected.json`; the corpus runner verifies the hash before validating, refusing to run if the bytes have been mutated since the expected verdict was recorded.

---

## asserted_signal_type

- **Source of truth**: caller's own routing/parsing context (Aperture knows the signal from the HTTP path or gRPC method; the corpus runner reads it from the `.expected.json` descriptor).
- **Consumers**: the matching `validate_*` function dispatches; the resulting `OtlpViolation` echoes the asserted signal back so the caller has it in the diagnostic.
- **Owner**: caller. The harness never infers signal type — signal-type inference is explicitly out of scope for v0.
- **Integration risk**: MEDIUM — a caller asserting the wrong signal type produces a `WireType::SignalMismatch` violation, which is the correct outcome (the harness catches the error) but only if the caller has its own routing right in the first place.
- **Validation**: Aperture's routing and the corpus runner's descriptor are both authoritative declarations; mismatch surfaces as a violation, not as silent corruption.

---

## asserted_framing

- **Source of truth**: caller's own context (HTTP `Content-Type`, gRPC framing layer).
- **Consumers**: the harness uses framing to choose between OTLP/gRPC and OTLP/HTTP/protobuf decoding paths; framing is echoed in the violation.
- **Owner**: caller.
- **Integration risk**: LOW — for v0, both framings parse the same protobuf-encoded `ExportFooServiceRequest` payload; the framing affects only how the request is delimited on the wire (gRPC's length-prefixed framing versus HTTP's body-as-message). The harness validates the message body in both cases.
- **Validation**: framing is a `pub enum` with two variants in v0; non-exhaustive on purpose to allow future framings without breaking callers.

---

## crate_public_surface

- **Source of truth**: the `pub` items in `crates/otlp-conformance-harness/src/lib.rs`. Documented in the crate's rustdoc.
- **Consumers**: every Kaleidoscope component depending on the harness; third-party callers; the harness's own integration tests.
- **Owner**: harness crate. Public surface changes follow Rust SemVer.
- **Integration risk**: HIGH — accidental public-surface widening creates support burden the harness should not carry; accidental narrowing breaks downstream consumers.
- **Validation**: the harness uses `cargo public-api` (or `cargo-semver-checks`) in CI to flag any change to the public surface that is not accompanied by an explicit version bump. Implementation deferred to DESIGN wave; the requirement is named here.

---

## CI invariants

These are not data artefacts but they share the property that they cross-cut the crate and its consumers:

| Invariant                                                          | Source of truth                                                | Validation                                                    |
|--------------------------------------------------------------------|----------------------------------------------------------------|----------------------------------------------------------------|
| Every accept-path vector must round-trip without violation         | Slice 04–06 tests; corpus accept directory                     | Corpus runner asserts `Result::is_ok` for every accept vector |
| Every reject-path vector must produce its declared rule            | Slice 01–03 tests; corpus reject directory                     | Corpus runner asserts `OtlpViolation::rule` matches            |
| False-positive rate is zero                                        | Combination of every accept-path test (none may regress)        | CI fails on any accept-path vector flipping to reject          |
| The harness emits no telemetry of its own                          | Crate-level lint + integration test capturing stdout/stderr     | Test asserts no writes to `io::stdout`, `io::stderr`           |
