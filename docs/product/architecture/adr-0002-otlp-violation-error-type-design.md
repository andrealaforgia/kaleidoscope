# ADR-0002 — `OtlpViolation` error-type design

- **Status**: Accepted
- **Date**: 2026-05-03
- **Author**: `nw-solution-architect` (Morgan)
- **Supersedes**: none
- **Superseded by**: none

## Context

`OtlpViolation` is the only error type the harness exposes. Every consumer (Aperture, Sluice, every storage engine, every third-party emitter) pattern-matches on it, so its shape is a load-bearing decision.

The DISCUSS wave fixes the **fields** of `OtlpViolation` (US-01..US-06 plus the journey YAML at step 3):

- `rule: Rule`
- `locus: ByteOffset`
- `expected: <human-readable string>`
- `observed: <human-readable string>`
- `signal_asserted: SignalType`
- `framing_asserted: Framing`

DISCUSS does **not** decide:

1. Whether `Rule` is a flat enum (`EmptyInput | ProtobufDecode | SignalMismatch { ... }`) or a nested enum (`EmptyInput | WireType(WireTypeRule)`). The user stories use the nested form (`Rule::WireType(WireTypeRule::ProtobufDecode)`) in ACs and Gherkin scenarios, suggesting the nested form, but this is presentation, not yet a binding decision.
2. Whether `OtlpViolation` and `Rule` are `#[non_exhaustive]`.
3. Whether `OtlpViolation` implements `std::error::Error`, `Display`, `Debug`, and how rich `Display` is.
4. Whether `OtlpViolation` carries a `source` causal chain (e.g. wrapping `prost::DecodeError`).

The shared-artefacts registry (`violation_rule_set`) flags this surface as HIGH integration risk: every consumer matches on it, so the shape must be both expressive (today's rules cover three rules in two categories) and evolvable (US-02's technical notes already foresee richer locus reporting; the wave-decisions document foresees multi-violation reporting in v0.1).

## Decision

### Rule discrimination — nested enum

```rust
#[non_exhaustive]
pub enum Rule {
    EmptyInput,
    WireType(WireTypeRule),
}

#[non_exhaustive]
pub enum WireTypeRule {
    ProtobufDecode,
    SignalMismatch { observed: SignalType, asserted: SignalType },
}
```

Nesting matches the user-stories' naming exactly (`Rule::WireType(WireTypeRule::ProtobufDecode)`, `Rule::WireType(WireTypeRule::SignalMismatch { observed, asserted })`) and creates an obvious place for future rule families (e.g. a future `Rule::Semantic(SemanticRule::MissingRequiredAttribute)` if Codex's semconv checks ever migrate into the harness — they will not for v0, but the namespace is reserved by the design).

### `#[non_exhaustive]` everywhere additive

Every public enum and `OtlpViolation` itself carry `#[non_exhaustive]`:

- `Rule`, `WireTypeRule`, `Framing`, `SignalType`, `ByteOffset` — all `#[non_exhaustive]`.
- `OtlpViolation` (the struct) — also `#[non_exhaustive]` so future fields (multi-violation, `caused_at` timestamp, etc.) are additive.

This makes adding a new variant or field a **non-breaking minor-version change**. Consumers that want exhaustive matching opt in with `#[deny(non_exhaustive_omitted_patterns)]` (per `shared-artifacts-registry.md > violation_rule_set`).

### Trait implementations

```rust
impl std::fmt::Debug for OtlpViolation { /* derived */ }
impl std::fmt::Display for OtlpViolation { /* single-line, ~120 chars max */ }
impl std::error::Error for OtlpViolation {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> { /* see below */ }
}
```

`Display` produces one line, terse, structured-but-readable, suitable for an HTTP 400 response body or a log message:

```text
otlp violation: rule=WireType::ProtobufDecode signal=Logs framing=HttpProtobuf locus=byte 50 expected="valid protobuf wire bytes per opentelemetry-proto descriptor" observed="unexpected EOF in length-delimited field"
```

Multi-line/structured output is the consumer's responsibility — they have all the fields they need on the value itself (`{:?}` for full debug; their own formatter for JSON; `Display` for one-line).

### Causal chain via `source`

```rust
#[non_exhaustive]
pub struct OtlpViolation {
    pub rule: Rule,
    pub locus: ByteOffset,
    pub expected: Cow<'static, str>,
    pub observed: Cow<'static, str>,
    pub signal_asserted: SignalType,
    pub framing_asserted: Framing,
    // Crate-private: only set when wrapping a prost::DecodeError, so consumers can
    // walk the chain via std::error::Error::source() if they want raw prost details.
    // Boxed because std::error::Error is a trait object.
    source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
}
```

The `source` field is **crate-private** (no `pub`), accessed only through `std::error::Error::source`. This satisfies US-02 AC 3 ("the violation does not expose `prost::DecodeError` directly in its public type") because the field's *type* in the public interface is the trait-object, not `prost::DecodeError` — consumers walking the chain see `&dyn Error`, not `&prost::DecodeError`. A consumer that wants to downcast via `error.source()?.downcast_ref::<prost::DecodeError>()` can — that is the standard Rust escape hatch and is not a contract violation.

Field types use `Cow<'static, str>` for `expected` and `observed`: most call sites use static literals (`"non-empty OTLP body"`, `"0 bytes"`) and pay zero allocations; the `prost::DecodeError`-derived path uses `Cow::Owned` and pays one allocation per error.

## Alternatives Considered

### Option A — Nested enum, `#[non_exhaustive]`, `Error` impl, single-line `Display`, `source` chain (RECOMMENDED, accepted)

Detailed above.

**Pros**:
- Matches the user-stories' presentation (`Rule::WireType(WireTypeRule::ProtobufDecode)`).
- Reserves rule-family namespaces for v0.1 evolution.
- `#[non_exhaustive]` makes additive evolution non-breaking.
- Standard Rust error idiom (`std::error::Error` + `Display` + `Debug`).
- `source` chain lets consumers downcast to `prost::DecodeError` for raw details without exposing it in the public surface.

**Cons**:
- Pattern matching is verbose (`Rule::WireType(WireTypeRule::ProtobufDecode)`). Mitigated by the closed rule set being small (three rules in v0); consumers can write helper functions.
- One pointer-sized field for `source` even when the violation has no cause (e.g. `EmptyInput`). Acceptable: 8 bytes per violation on the failure path, which is not the hot path.

### Option B — Flat enum

```rust
pub enum Rule {
    EmptyInput,
    ProtobufDecode,
    SignalMismatch { observed: SignalType, asserted: SignalType },
}
```

**Pros**:
- Simpler pattern matching at the call site (`Rule::ProtobufDecode` vs `Rule::WireType(WireTypeRule::ProtobufDecode)`).
- One enum to evolve.

**Cons**:
- Diverges from the user-stories' naming, requiring a rename across every Gherkin scenario and AC.
- No room for rule families. If v0.1 adds, say, a `Framing::ProtobufDecode` variant for gRPC framing decode (separate from wire-byte decode), the flat namespace becomes confusing.
- Pollutes the rule-name space — all variants are top-level peers, regardless of conceptual category.

**Rejected** for the divergence-from-user-stories cost and the loss of namespace.

### Option C — Trait-object error type

```rust
pub trait Violation: std::error::Error { /* ... */ }
pub fn validate_logs(...) -> Result<..., Box<dyn Violation>>;
```

**Pros**:
- Maximally extensible — third parties could implement custom violations.

**Cons**:
- Resume-driven over-engineering: zero customer for third-party violations in v0.
- Defeats pattern matching (the entire point of the closed-rule discipline).
- `Box<dyn ...>` adds an allocation per error.
- Diverges hard from the locked function signatures in US-06 AC 5 (`Result<..., OtlpViolation>` is concrete).

**Rejected** outright. Pattern matching on a closed enum is the contract; abandoning it would defeat US-01..US-06's whole point.

### Option D — `#[non_exhaustive]` only on enums, struct fields all `pub`

**Pros**:
- Slightly simpler.

**Cons**:
- Adding a field to `OtlpViolation` becomes a major-version breaking change.
- The wave-decisions document (W-internal "downstream consumers want a richer violation type") foresees additive struct evolution as a known v0.1 use case.

**Rejected** for the major-bump cost on what should be a minor-version evolution.

### Option E — No `source` chain; `prost::DecodeError`'s details discarded

**Pros**:
- Slimmer struct.
- Slightly stricter encapsulation.

**Cons**:
- Loses information that some consumers may want for debugging (the `prost::DecodeError`'s message, derived field path, etc.).
- The DISCUSS wave's risks-and-mitigations table flags `prost::DecodeError`'s diagnostic quality as an open question. Keeping `source` lets the harness's own tests assert against the underlying prost diagnostic; throwing it away forecloses that.

**Rejected** because the cost (one optional `Box`) is small and the information is genuinely useful when present.

## Consequences

### Positive

- Pattern matching at the call site is exhaustive within a closed rule set; new rules force consumers to acknowledge them via `#[non_exhaustive]`'s opt-in escape hatch.
- The error type composes with `?` in any function returning a type that `From<OtlpViolation>` is implementable for.
- `std::error::Error` impl plays well with `anyhow`, `eyre`, and similar consumer-side error-aggregation libraries.
- The `source` chain preserves `prost::DecodeError` details for consumers that want them, without forcing the type into the public surface.
- The struct is `Send + Sync + Clone + Debug + Display + Error`; safe to pass across threads, copy into responses, log, or compare.

### Negative

- Pattern-matching verbosity (`Rule::WireType(WireTypeRule::ProtobufDecode)`) — accepted as the cost of namespace clarity.
- One optional `Box<dyn Error>` field per `OtlpViolation` even when unused — accepted.
- `#[non_exhaustive]` requires consumers to either accept the `_` arm or opt into exhaustive matching with a `deny` lint. Accepted as the price of additive evolution.

### Trade-off ATAM

This decision is a trade-off point for **Maintainability — Modifiability** (positive: additive evolution is non-breaking via `#[non_exhaustive]`) versus **Usability — Operability for the Consumer** (slightly negative: consumers must always handle the catch-all arm). The trade-off is intentional and the bias is towards modifiability because the user stories explicitly anticipate additive evolution (multi-violation in v0.1, richer locus reporting later).

It is a sensitivity point for **Functional Suitability — Correctness**: the closed-rule discipline is what lets every consumer pattern-match safely on violations. Breaking it would degrade correctness across every downstream consumer simultaneously.
