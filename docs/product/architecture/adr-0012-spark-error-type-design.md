# ADR-0012 — `SparkError` error-type design

- **Status**: Accepted
- **Date**: 2026-05-06
- **Author**: `nw-solution-architect` (Morgan)
- **Feature**: `spark` v0
- **Supersedes**: none
- **Superseded by**: none

## Context

`SparkError` is the only error type Spark exposes. Every application
that calls `spark::init` pattern-matches on it (or propagates it via
`?` to `main`), so its shape is load-bearing.

DISCUSS `wave-decisions.md > D2` locks the **closed variant set** at v0:

```
MissingRequiredAttribute { name: String }
InvalidEndpoint { endpoint: String, reason: String }
ExporterInitFailed { reason: String }
GlobalAlreadyInitialised
```

DISCUSS does **not** lock:

1. Whether `SparkError` derives via `thiserror` or hand-rolls
   `Display`/`Error` impls.
2. The `#[non_exhaustive]` posture (DISCUSS implies it; DESIGN locks it).
3. Whether the variants carry causal-chain information via `source()`
   (e.g. wrapping `url::ParseError` for `InvalidEndpoint`, wrapping
   `opentelemetry::trace::TraceError` for `ExporterInitFailed`).
4. Whether `SparkError` implements `Clone`, `PartialEq`, `Eq`, or other
   convenience traits.
5. The exact `Display` shape for each variant.

`shared-artifacts-registry.md > spark_error_variants` classifies the
variant set as **HIGH integration risk** — pattern-match exhaustiveness
on consumer side breaks if variants are renamed.

Harness ADR-0002 (`OtlpViolation` design) is the precedent. Spark's
shape diverges in one structural way: `SparkError` is a flat enum (one
variant per failure class), not a nested rule enum. The reason is the
domain: `OtlpViolation` carries a finite tree of wire-conformance rules
that may grow (semantic rules in v0.1+); `SparkError` carries a finite
set of init-time refusals that are inherently flat (each is one of
"missing input", "invalid input", "underlying SDK refused", "global
state collision").

## Decision

### Variant set + `thiserror` derive

```rust
use std::fmt;

#[derive(Debug)]
#[non_exhaustive]
pub enum SparkError {
    /// A required attribute was absent or empty when `spark::init`
    /// validated the configuration.
    ///
    /// `name` is the OpenTelemetry semantic-conventions key ("service.name"
    /// or "tenant.id") so the application's error handler can map it
    /// to a configuration field.
    MissingRequiredAttribute { name: String },

    /// The resolved endpoint URI failed to parse, or its scheme was
    /// neither `http` nor `https`.
    ///
    /// `endpoint` is the literal value Spark attempted to use; `reason`
    /// is a human-readable parse-failure message ultimately sourced
    /// from `url::ParseError` or from Spark's own scheme check.
    InvalidEndpoint { endpoint: String, reason: String },

    /// The upstream `opentelemetry-otlp` exporter constructor returned
    /// an error (TLS configuration, transport setup, runtime not
    /// available, etc.).
    ///
    /// `reason` carries the upstream error's `Display` form. The
    /// causal chain is exposed via `source()`.
    ExporterInitFailed { reason: String, source: Option<Box<dyn std::error::Error + Send + Sync + 'static>> },

    /// `spark::init` was called twice in the same process, OR
    /// `opentelemetry::global::set_tracer_provider` had already been
    /// called by some other code in this process before `spark::init`
    /// ran.
    ///
    /// Carries no payload — the diagnostic is the variant name.
    GlobalAlreadyInitialised,
}

impl fmt::Display for SparkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingRequiredAttribute { name } => {
                write!(f, "spark: missing required attribute: {name}")
            }
            Self::InvalidEndpoint { endpoint, reason } => {
                write!(f, "spark: invalid endpoint {endpoint:?}: {reason}")
            }
            Self::ExporterInitFailed { reason, .. } => {
                write!(f, "spark: exporter initialisation failed: {reason}")
            }
            Self::GlobalAlreadyInitialised => {
                f.write_str("spark: opentelemetry global tracer provider already initialised")
            }
        }
    }
}

impl std::error::Error for SparkError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::ExporterInitFailed { source: Some(s), .. } => Some(s.as_ref()),
            _ => None,
        }
    }
}
```

The `thiserror` derive macro is not strictly necessary for this shape
(the `Display` impl is already explicit, the `source()` is explicit,
and `Debug` is `#[derive(Debug)]`). But declaring `[dependencies]
thiserror = "2"` in `Cargo.toml` keeps the door open for future
variants (e.g. `#[error(transparent)] Url(#[from] url::ParseError)`)
without re-architecting. ADR-0013 §1 details the dependency.

### `#[non_exhaustive]` discipline

```rust
#[non_exhaustive]
pub enum SparkError { /* ... */ }

// Each struct-like variant payload is also non-exhaustive at the
// type level via construction-site discipline: variants are constructed
// only inside the `error.rs` module, so adding a field is a private
// concern. Pattern-matching consumers get the `#[non_exhaustive]` enum
// guard (which forces a `_ =>` arm) and the variant-fields list comes
// from the public Display contract above.
```

Exhaustive matching by external consumers requires an explicit `_ => ...`
arm — this is the language-level enforcement of "additions are
non-breaking". Harness ADR-0002's `#[non_exhaustive]` discipline applies
verbatim.

Adding a future variant (e.g. `SparkError::InvalidConfiguration` for a
class of misconfiguration not covered by the four current variants) is
a **non-breaking minor-version change**; renaming or removing a variant
is **major**. `cargo semver-checks` (Gate 3 of ADR-0011) catches this.

### `From` impls for ergonomic propagation

```rust
impl From<url::ParseError> for SparkError {
    fn from(e: url::ParseError) -> Self {
        Self::InvalidEndpoint { endpoint: String::new(), reason: e.to_string() }
    }
}
```

The `From<url::ParseError>` impl is **rejected** for this exact form
because it loses the `endpoint` value (the parse-error type does not
carry the original input). Instead Spark's `init.rs` constructs
`InvalidEndpoint` explicitly inside its own validation loop:

```rust
// in init.rs (illustrative; software-crafter writes the actual code)
let parsed = url::Url::parse(&resolved_endpoint).map_err(|e| {
    SparkError::InvalidEndpoint {
        endpoint: resolved_endpoint.clone(),
        reason: e.to_string(),
    }
})?;
```

For `ExporterInitFailed`, Spark wraps the upstream error explicitly:

```rust
// in init.rs (illustrative)
let exporter = opentelemetry_otlp::new_exporter()
    .tonic()
    .with_endpoint(resolved_endpoint)
    .build_span_exporter()
    .map_err(|e| SparkError::ExporterInitFailed {
        reason: e.to_string(),
        source: Some(Box::new(e)),
    })?;
```

This gives consumers `Display` for the human-readable line **and**
`source()` for chained-error inspection (`anyhow`, `eyre`, `tracing`-as-
formatter all walk the chain).

### Trait derives — `Debug` only on the public surface

```rust
#[derive(Debug)]
#[non_exhaustive]
pub enum SparkError { /* variants */ }
```

- `Debug` — yes. Every error needs `{:?}` for `expect(...).unwrap()`.
- `Clone` — **no**. `ExporterInitFailed::source` is `Box<dyn Error +
  Send + Sync>`; not all upstream errors are `Clone`. Callers that need
  multiple copies of the diagnostic can `error.to_string()` once and
  share the string.
- `PartialEq` / `Eq` — **no**. Same reason as `Clone`. Tests that
  pattern-match check the variant + its named fields explicitly,
  which is more robust than equality on the whole enum (e.g. fields
  added later don't break existing assertions).
- `Hash` — **no**. Errors are not map keys.
- `serde::Serialize` — **no**. Spark v0 is a Rust-native crate; an
  application that wants to serialise the error to JSON does it itself
  via the `Display` line.

This minimum-trait posture matches harness ADR-0002 "no extra trait
impls without a customer".

### `Display` content for each variant — locked

The `Display` strings above are part of the v0 contract. They are
asserted verbatim by the integration tests' substring checks (Slice 02:
"the error message contains 'tenant.id'"). Renames are version-bump-
able; tweaks (e.g. capitalisation, punctuation) are forbidden during
v0.

The `journey-spark-visual.md` mockups (Step 2 tui_mockup) show the
exact `Debug` form Cargo's `expect()` print would produce; that form is
also stable across the v0 contract.

## Alternatives Considered

### Option A — Hand-rolled enum + explicit `Display` + explicit `Error` (RECOMMENDED, accepted)

Detailed above. The `thiserror` crate is in the dependency tree but the
explicit `impl Display` is preferred over `#[error("...")]` attribute
strings for two reasons: the substring-assertion tests want a single
read site for the error format, and the OTel SDK's own
`opentelemetry::trace::TraceError` has a similar shape (no `thiserror`,
explicit Display) — Spark matches its substrate's idiom.

**Pros**:
- Explicit `Display` strings are review-grep-able; an ADR change to
  the `Display` shape is a one-file edit.
- `source()` chain is explicit; no derive-attribute confusion about
  which field is `#[source]`.
- The `thiserror` dependency is held in reserve for future variants
  whose payloads are structured (a future
  `#[error(transparent)] Url(#[from] url::ParseError)` would land
  cleanly).

**Cons**:
- ~ 30 lines of hand-written `Display` and `Error`. Acceptable.

### Option B — Pure `thiserror` derive

```rust
#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum SparkError {
    #[error("spark: missing required attribute: {name}")]
    MissingRequiredAttribute { name: String },
    #[error("spark: invalid endpoint {endpoint:?}: {reason}")]
    InvalidEndpoint { endpoint: String, reason: String },
    #[error("spark: exporter initialisation failed: {reason}")]
    ExporterInitFailed {
        reason: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },
    #[error("spark: opentelemetry global tracer provider already initialised")]
    GlobalAlreadyInitialised,
}
```

**Pros**:
- 4 fewer lines.
- Standard idiom; many Rust crates do this.

**Cons**:
- The `#[error("...")]` attribute strings are buried in the variant
  attribute syntax; grep-ability is worse than the explicit `Display`.
- The `#[source]` attribute on the `Option<Box<dyn Error + Send +
  Sync>>` field works but is one of the trickier `thiserror` cases
  (you have to remember `Option<Box<...>>` to make `source()` unwrap
  the option correctly).

**Rejected** because the explicit-Display option is barely more code
and is more legible at review time. The `thiserror` dep stays in the
tree as a future-proof reserve.

### Option C — One generic `SparkError(String)` newtype

```rust
pub struct SparkError(String);
impl Display for SparkError { /* prints the string */ }
```

**Pros**:
- Smallest possible surface.

**Cons**:
- Loses pattern-matching. The whole point of the closed variant set
  (DISCUSS D2) is that an application can write
  ```rust
  match e {
      SparkError::MissingRequiredAttribute { name } => /* ask user */,
      SparkError::InvalidEndpoint { .. } => /* unreachable in our app */,
      ...
  }
  ```
  A `String` newtype reduces this to substring matching, which is
  brittle.

**Rejected** outright. Same reasoning as Aperture's
`ApertureError(pub String)` (the Slice-01 stub) is being replaced by
the rich enum in subsequent slices: "the simple newtype is a
distill-state placeholder, not the v0 contract".

### Option D — Variants carry typed source errors directly

```rust
pub enum SparkError {
    InvalidEndpoint { endpoint: String, source: url::ParseError },
    ExporterInitFailed { source: opentelemetry::trace::TraceError },
    // ...
}
```

**Pros**:
- Maximum information preserved.

**Cons**:
- Couples Spark's public surface to upstream error types. If
  `url::ParseError` adds a variant (or `opentelemetry::trace::TraceError`
  changes its shape), Spark's surface changes too — Gate 3
  (`cargo semver-checks`) breaks.
- Forces consumers to add a `url` dep just to pattern-match Spark's
  errors.

**Rejected**. The `Box<dyn Error + Send + Sync>` source pattern is the
upstream-hygiene-respecting answer.

## Consequences

### Positive

- The four DISCUSS-locked variants are exactly the public surface; no
  more, no less.
- `#[non_exhaustive]` posture makes additions non-breaking;
  `cargo semver-checks` enforces the rule.
- `Display` strings are explicit and grep-able; substring-assertion
  tests in Slice 02 have a single read site.
- The `source()` chain is preserved for `ExporterInitFailed`, so
  downstream `tracing` formatters and `anyhow::Error::context` walks
  produce useful diagnostics.
- The minimum-trait-derive posture (`Debug` only) avoids API breakage
  if upstream errors change their trait surface.

### Negative

- The `thiserror` dep is in the tree without being used by the derive
  macro at v0. Acceptable: future variants will use it; the cost is
  one transitively-resolved crate.
- The `Display` strings are part of the contract: changing capitalisation
  or punctuation breaks the test substring assertions and is a v0
  contract change. Acceptable: the assertions are deliberate.

### Trade-off ATAM

This decision is a sensitivity point for **Maintainability —
Modifiability** (positive: `#[non_exhaustive]` makes additions
non-breaking) and for **Reliability — Maturity** (positive: explicit
`source()` chain integrates with the wider Rust error ecosystem
without coupling Spark to any one upstream error type).

It is a trade-off point against **Functional Suitability —
Appropriateness** (negative: variants do not carry strongly-typed
upstream errors, so consumers cannot pattern-match on, e.g., the
specific `url::ParseError` kind). Accepted because the upstream
hygiene benefit dominates: every Kaleidoscope crate that handles
errors will face the same trade-off, and the Box-source pattern is
the cross-crate idiom Aegis, Loom, Codex, Sieve will inherit.

## Self-Application of Earned Trust (principle 12)

The `#[non_exhaustive]` contract is enforced by:

1. **Subtype check** — Rust's exhaustiveness check on the consumer
   side. Without `_ => ...` arm, the consumer's match fails to compile
   the moment Spark adds a variant. This is a compile-time guarantee
   from the language.
2. **Structural check** — `cargo semver-checks` (Gate 3) walks the
   variant list and refuses minor-bump commits that remove or rename
   a variant.
3. **Behavioural check** — Slice 02's integration test asserts every
   variant by name AND by its named fields. A pattern-match that
   compiles but misses a field (e.g. a future variant adds a `cause`
   field) is caught by the test.

The `Display` substring contract is enforced by Slice 02's tests:
the test asserts `error.to_string().contains("tenant.id")`, etc. A
silent change to the `Display` shape (e.g. someone "tidies" the
formatter) breaks the test.
